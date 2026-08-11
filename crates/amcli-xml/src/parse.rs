use std::sync::Arc;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::{Attr, AttrOrigin, Doc, EmitStyle, Name, NodeData, NodeId, NodeState, Span};

#[derive(Debug, thiserror::Error)]
pub enum XmlError {
    #[error("not valid UTF-8 at byte {0}: amcli only handles UTF-8 encoded XML")]
    NotUtf8(usize),
    #[error("malformed XML at byte {position}: {source}")]
    Malformed {
        position: u64,
        #[source]
        source: quick_xml::Error,
    },
    #[error("no root element")]
    NoRoot,
    #[error("unexpected end of document: <{0}> was never closed")]
    Unclosed(String),
}

pub(crate) fn parse(src: Arc<[u8]>) -> Result<Doc, XmlError> {
    if let Err(e) = std::str::from_utf8(&src) {
        return Err(XmlError::NotUtf8(e.valid_up_to()));
    }

    let mut reader = Reader::from_reader(&src[..]);
    let cfg = reader.config_mut();
    cfg.trim_text(false);
    cfg.expand_empty_elements = false;
    cfg.check_end_names = true;

    let mut nodes: Vec<NodeData> = Vec::new();
    let mut stack: Vec<NodeId> = Vec::new();
    let mut root: Option<NodeId> = None;
    // Where the next node's `lead` begins: the end of the previous sibling, or
    // the end of the enclosing start tag.
    let mut cursor: usize = 0;
    let mut last: usize = 0;

    loop {
        let ev = reader
            .read_event()
            .map_err(|source| XmlError::Malformed { position: reader.buffer_position(), source })?;
        let start = last;
        let end = reader.buffer_position() as usize;
        last = end;

        match ev {
            Event::Start(e) => {
                let id = NodeId(nodes.len() as u32);
                nodes.push(new_node(
                    &src,
                    &e,
                    Span::new(cursor, start),
                    Span::new(start, end),
                    stack.last().copied(),
                    false,
                ));
                if let Some(&p) = stack.last() {
                    nodes[p.idx()].children.push(id);
                } else if root.is_none() {
                    root = Some(id);
                }
                stack.push(id);
                cursor = end;
            }
            Event::Empty(e) => {
                let id = NodeId(nodes.len() as u32);
                nodes.push(new_node(
                    &src,
                    &e,
                    Span::new(cursor, start),
                    Span::new(start, end),
                    stack.last().copied(),
                    true,
                ));
                if let Some(&p) = stack.last() {
                    nodes[p.idx()].children.push(id);
                } else if root.is_none() {
                    root = Some(id);
                }
                cursor = end;
            }
            Event::End(_) => {
                let id = stack.pop().expect("check_end_names guarantees a match");
                let n = &mut nodes[id.idx()];
                n.tail = Span::new(cursor, start);
                n.span = Span::new(n.open.start as usize, end);
                cursor = end;
            }
            Event::Eof => break,
            // Text, comments, CDATA, PIs, the declaration and the DOCTYPE are
            // never nodes. They sit inside a `lead` or `tail` span and are
            // therefore preserved byte for byte without being understood.
            _ => {}
        }
    }

    if let Some(open) = stack.last() {
        let name = match &nodes[open.idx()].name {
            Name::Src(s) => String::from_utf8_lossy(s.slice(&src)).into_owned(),
            Name::New(s) => s.to_string(),
        };
        return Err(XmlError::Unclosed(name));
    }

    let root = root.ok_or(XmlError::NoRoot)?;
    let prologue = nodes[root.idx()].lead;
    let epilogue = Span::new(nodes[root.idx()].span.end as usize, src.len());
    let style = detect_style(&src);

    Ok(Doc { src, nodes, root, prologue, epilogue, style })
}

fn new_node(
    src: &[u8],
    e: &BytesStart<'_>,
    lead: Span,
    open: Span,
    parent: Option<NodeId>,
    self_closing: bool,
) -> NodeData {
    NodeData {
        name: Name::Src(subspan(src, e.name().as_ref()).unwrap_or_default()),
        attrs: read_attrs(src, e, open),
        children: Vec::new(),
        parent,
        lead,
        // For an empty element the start tag is the whole element; a Start event
        // gets its real extent when the matching End arrives.
        span: open,
        open,
        tail: Span::new(open.end as usize, open.end as usize),
        self_closing,
        text_override: None,
        state: NodeState::Pristine,
        subtree_dirty: false,
        removed: false,
    }
}

fn read_attrs(src: &[u8], e: &BytesStart<'_>, open: Span) -> Vec<Attr> {
    let mut out = Vec::new();
    // `with_checks(false)`: duplicate attributes are a validation concern, not a
    // parsing one. Reporting them is the model layer's job, and refusing to read
    // a file we could otherwise repair would be unhelpful.
    for a in e.attributes().with_checks(false) {
        let Ok(a) = a else { continue };
        let (Some(name), Some(value)) = (subspan(src, a.key.as_ref()), value_span(src, &a)) else {
            continue;
        };
        // Walk back over the whitespace before the name so that removing the
        // attribute also removes its separator.
        let mut fs = name.start as usize;
        while fs > open.start as usize + 1 && src[fs - 1].is_ascii_whitespace() {
            fs -= 1;
        }
        // One past the closing quote.
        let fe = (value.end as usize + 1).min(open.end as usize);
        out.push(Attr {
            origin: AttrOrigin::Src { full: Span::new(fs, fe), name, value },
            value_override: None,
            removed: false,
        });
    }
    out
}

/// The attribute value as a span, when quick-xml handed back a borrowed slice.
fn value_span(src: &[u8], a: &quick_xml::events::attributes::Attribute<'_>) -> Option<Span> {
    match &a.value {
        std::borrow::Cow::Borrowed(v) => subspan(src, v),
        // An owned value means quick-xml normalised something; we have no source
        // range for it, so the attribute is skipped rather than mis-spanned.
        std::borrow::Cow::Owned(_) => None,
    }
}

/// Offset of a borrowed sub-slice within the source buffer.
fn subspan(src: &[u8], sub: &[u8]) -> Option<Span> {
    let base = src.as_ptr() as usize;
    let p = sub.as_ptr() as usize;
    if p < base || p + sub.len() > base + src.len() {
        return None;
    }
    Some(Span::new(p - base, p - base + sub.len()))
}

/// Infer the indent unit and line ending from the first indented element.
fn detect_style(src: &[u8]) -> EmitStyle {
    let mut style = EmitStyle::default();
    let mut i = 0;
    let mut found_eol = false;
    while i < src.len() {
        if src[i] != b'\n' {
            i += 1;
            continue;
        }
        if !found_eol {
            style.eol =
                if i > 0 && src[i - 1] == b'\r' { b"\r\n".to_vec() } else { b"\n".to_vec() };
            found_eol = true;
        }
        let ws_start = i + 1;
        let mut j = ws_start;
        while j < src.len() && (src[j] == b' ' || src[j] == b'\t') {
            j += 1;
        }
        // A run of whitespace that actually indents an element is one level.
        if j > ws_start && j < src.len() && src[j] == b'<' {
            style.indent = src[ws_start..j].to_vec();
            return style;
        }
        i += 1;
    }
    style
}
