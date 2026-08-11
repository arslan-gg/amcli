//! Random edit sequences against the real corpus. The invariants below are the
//! ones that must hold no matter what a caller does:
//!
//! 1. the output always re-parses;
//! 2. what we read back equals what we had in memory before writing;
//! 3. a subtree nothing touched is still byte-identical to the source.

use amcli_xml::{Doc, NodeBuilder, NodeId};
use proptest::prelude::*;

/// Loaded once. Proptest selects an *index* into this, so a failing case prints
/// a file name instead of dumping a few hundred kilobytes of bytes.
static CORPUS: std::sync::LazyLock<Vec<(String, Vec<u8>)>> = std::sync::LazyLock::new(corpus);

fn corpus() -> Vec<(String, Vec<u8>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    for dir in ["tests/corpus", "assets/archi"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(ext, "archimate" | "xml") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read");
            if bytes.starts_with(b"PK") || bytes.len() > 64 * 1024 {
                continue; // zipped models and the 229 KB matrix are covered elsewhere
            }
            out.push((path.display().to_string(), bytes));
        }
    }
    assert!(!out.is_empty(), "no corpus files found");
    out
}

/// A flat snapshot of everything the model layer can observe, so we can compare
/// a document against itself across a write/re-read cycle.
type NodeSnapshot = (String, Vec<(String, String)>, String);

fn snapshot(doc: &Doc) -> Vec<NodeSnapshot> {
    doc.descendants(doc.root())
        .into_iter()
        .map(|n| {
            let attrs = doc
                .attr_names(n)
                .into_iter()
                .map(|a| (a.to_string(), doc.attr(n, a).unwrap_or_default()))
                .collect();
            (doc.name(n).to_string(), attrs, doc.text(n))
        })
        .collect()
}

#[derive(Debug, Clone)]
enum Op {
    SetAttr { node: usize, name: String, value: String },
    RemoveAttr { node: usize, attr: usize },
    SetText { node: usize, text: String },
    AppendChild { node: usize, name: String },
    RemoveSubtree { node: usize },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    // Values deliberately include the characters that force escaping.
    let text = prop_oneof![
        Just("plain".to_string()),
        Just("a & b < c > d".to_string()),
        Just("Ünïcode Ñáme".to_string()),
        Just("line\nbreak\ttab".to_string()),
        Just("\"quoted\" and 'single'".to_string()),
        Just(String::new()),
    ];
    let name = prop_oneof![
        Just("id".to_string()),
        Just("name".to_string()),
        Just("xsi:type".to_string()),
        Just("brandNew".to_string()),
    ];
    prop_oneof![
        (0usize..200, name.clone(), text.clone()).prop_map(|(node, name, value)| Op::SetAttr {
            node,
            name,
            value
        }),
        (0usize..200, 0usize..6).prop_map(|(node, attr)| Op::RemoveAttr { node, attr }),
        (0usize..200, text).prop_map(|(node, text)| Op::SetText { node, text }),
        (0usize..200, name).prop_map(|(node, name)| Op::AppendChild { node, name }),
        (0usize..200).prop_map(|node| Op::RemoveSubtree { node }),
    ]
}

fn apply(doc: &mut Doc, nodes: &[NodeId], op: &Op) {
    let pick = |i: usize| nodes[i % nodes.len()];
    match op {
        Op::SetAttr { node, name, value } => doc.set_attr(pick(*node), name, value),
        Op::RemoveAttr { node, attr } => {
            let n = pick(*node);
            let names: Vec<String> = doc.attr_names(n).into_iter().map(String::from).collect();
            if !names.is_empty() {
                doc.remove_attr(n, &names[*attr % names.len()]);
            }
        }
        Op::SetText { node, text } => {
            let n = pick(*node);
            // Replacing text is only meaningful without element children.
            if doc.children(n).next().is_none() {
                let _ = doc.set_text(n, text);
            }
        }
        Op::AppendChild { node, name } => {
            // A refusal (mixed content) is a valid outcome, not a failure.
            let _ =
                doc.append_child(pick(*node), NodeBuilder::new(name.clone()).attr("id", "id-gen"));
        }
        Op::RemoveSubtree { node } => {
            let n = pick(*node);
            if n != doc.root() {
                doc.remove_subtree(n);
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn edits_survive_a_write_and_reread(
        pick in 0usize..64,
        ops in prop::collection::vec(op_strategy(), 1..12),
    ) {
        let (path, src) = &CORPUS[pick % CORPUS.len()];
        let mut doc = Doc::parse(src.clone()).unwrap_or_else(|e| panic!("{path}: {e}"));
        let nodes = doc.descendants(doc.root());
        for op in &ops {
            apply(&mut doc, &nodes, op);
        }

        let before = snapshot(&doc);
        let out = doc.to_bytes();
        let reread = Doc::parse(out.clone())
            .unwrap_or_else(|e| panic!("{path}: output does not re-parse: {e}\nops: {ops:?}"));
        prop_assert_eq!(snapshot(&reread), before, "{}: semantics changed across write", path);
    }

    /// Doing nothing must write the file back unchanged, whatever it contains.
    #[test]
    fn no_ops_is_the_identity(pick in 0usize..64) {
        let (path, src) = &CORPUS[pick % CORPUS.len()];
        let doc = Doc::parse(src.clone()).unwrap();
        prop_assert_eq!(&doc.to_bytes(), src, "{} changed with no edits", path);
    }

    /// An untouched sibling subtree keeps its exact bytes even when the document
    /// around it is being rebuilt.
    #[test]
    fn untouched_subtrees_keep_their_bytes(
        pick in 0usize..64,
        which in 0usize..200,
    ) {
        let (path, src) = &CORPUS[pick % CORPUS.len()];
        let mut doc = Doc::parse(src.clone()).unwrap();
        let nodes = doc.descendants(doc.root());
        let target = nodes[which % nodes.len()];

        // Everything on the path to the target may legitimately be rebuilt;
        // every other subtree must come out byte for byte.
        let touched: Vec<NodeId> = {
            let mut t = doc.descendants(target);
            let mut cur = Some(target);
            while let Some(n) = cur {
                t.push(n);
                cur = doc.parent(n);
            }
            t
        };
        let witnesses: Vec<(NodeId, Vec<u8>)> = nodes
            .iter()
            .filter(|n| !touched.contains(n))
            .filter_map(|&n| doc.source_bytes(n).map(|b| (n, b.to_vec())))
            .filter(|(_, b)| b.len() > 8)
            .take(24)
            .collect();

        doc.set_attr(target, "amcli-probe", "1");
        let out = doc.to_bytes();
        for (n, bytes) in witnesses {
            prop_assert!(
                find(&out, &bytes).is_some(),
                "{}: subtree {:?} was reformatted:\n{}",
                path,
                n,
                String::from_utf8_lossy(&bytes)
            );
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}
