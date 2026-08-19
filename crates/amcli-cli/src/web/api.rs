//! What the page asks the server for, and the JSON it gets back.
//!
//! One blob describes the model — every folder, element, relationship and
//! view, referring to each other by array index so a five-thousand-concept
//! model is about a megabyte and one fetch. Documentation and properties are
//! left out of it, because in a real model they outweigh everything else, and
//! fetched per concept when a detail panel opens. Views are rendered on demand
//! by the same renderer `view render` uses, so the page shows the drawing the
//! file holds, not a reinterpretation of it.
//!
//! JSON is written by hand here as everywhere else in the CLI: the shapes are
//! fixed and ours, and `output::quote` is the only escaping needed.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use amcli_graph::Graph;
use amcli_model::{ConceptId, ConceptKind, ElementType, Model, RelType};
use amcli_view::layout::{Algorithm, Item, Lanes, fit_size, place_with};
use amcli_view::notation::{self, Deco, Figure};

use super::http::{Request, Response};
use super::state::State;
use crate::output::quote;

/// The page, compiled in. Everything under `assets/` must be listed here — a
/// test walks the directory and fails on a file that is not — and nothing else
/// is ever served, so there is no path to traverse.
pub const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("assets/index.html")),
    ("/app.css", "text/css; charset=utf-8", include_str!("assets/app.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("assets/app.js")),
    ("/dom.js", "text/javascript; charset=utf-8", include_str!("assets/dom.js")),
    ("/store.js", "text/javascript; charset=utf-8", include_str!("assets/store.js")),
    ("/router.js", "text/javascript; charset=utf-8", include_str!("assets/router.js")),
    ("/panzoom.js", "text/javascript; charset=utf-8", include_str!("assets/panzoom.js")),
    ("/notation.js", "text/javascript; charset=utf-8", include_str!("assets/notation.js")),
    ("/pages/views.js", "text/javascript; charset=utf-8", include_str!("assets/pages/views.js")),
    (
        "/pages/elements.js",
        "text/javascript; charset=utf-8",
        include_str!("assets/pages/elements.js"),
    ),
    (
        "/pages/relations.js",
        "text/javascript; charset=utf-8",
        include_str!("assets/pages/relations.js"),
    ),
    ("/pages/graph.js", "text/javascript; charset=utf-8", include_str!("assets/pages/graph.js")),
    ("/pages/stats.js", "text/javascript; charset=utf-8", include_str!("assets/pages/stats.js")),
    ("/pages/detail.js", "text/javascript; charset=utf-8", include_str!("assets/pages/detail.js")),
];

pub fn route(req: &Request, state: &State) -> Response {
    let path = req.path.as_str();
    if let Some((_, mime, body)) = ASSETS.iter().find(|(p, _, _)| *p == path) {
        return Response::new(200, mime, *body);
    }
    if path == "/index.html" {
        return Response::new(200, ASSETS[0].1, ASSETS[0].2);
    }

    let snap = state.current();
    match path {
        "/api/model" => Response::json(200, snap.model_json.to_string()),
        "/api/status" => {
            Response::json(200, status_json(&snap.checksum, snap.loaded, state.last_error()))
        }
        "/api/icons.svg" => {
            Response::new(200, "image/svg+xml; charset=utf-8", amcli_render::icon_defs())
        }
        "/api/layout" => layout(&snap.model, req),
        _ => {
            if let Some(id) = path.strip_prefix("/api/concept/") {
                return concept(&snap.model, id);
            }
            if let Some(rest) = path.strip_prefix("/api/view/") {
                if let Some(id) = rest.strip_suffix(".svg") {
                    return view(&snap.model, id, "svg");
                }
                if let Some(id) = rest.strip_suffix(".json") {
                    return view(&snap.model, id, "json");
                }
                if let Some(id) = rest.strip_suffix(".png") {
                    return view(&snap.model, id, "png");
                }
            }
            Response::error(404, "no such page")
        }
    }
}

fn status_json(checksum: &str, loaded: u64, error: Option<String>) -> String {
    format!(
        "{{\"checksum\":{},\"loaded\":{loaded},\"error\":{}}}",
        quote(checksum),
        error.as_deref().map(quote).unwrap_or_else(|| "null".into())
    )
}

fn concept(m: &Model, id: &str) -> Response {
    let Some(cid) = m.concept_by_id(id) else {
        return Response::error(404, "no such concept");
    };
    let c = m.concept(cid);
    let mut s = String::from("{");
    let _ = write!(s, "\"id\":{},\"doc\":{}", quote(&c.id), opt(m.documentation(c.node)));
    s.push_str(",\"properties\":");
    pairs(&mut s, &m.properties(c.node));
    s.push('}');
    Response::json(200, s)
}

fn view(m: &Model, id: &str, format: &str) -> Response {
    let Some(v) = m.view_by_id(id) else {
        return Response::error(404, "no such view");
    };
    let scene = amcli_view::compile(m, v);
    if format == "json" {
        return Response::json(200, amcli_render::scene_json(&scene));
    }
    if format == "png" {
        // Twice the view's size: crisp on a high-density screen and in a slide.
        let o = amcli_render::Options { scale: 2.0, ..Default::default() };
        return match amcli_render::png(&scene, &o) {
            Ok(bytes) => Response::new(200, "image/png", bytes),
            Err(e) => Response::error(500, &e),
        };
    }
    // Unsized: the page fits the drawing to its pane and zooms by viewBox.
    let o = amcli_render::Options { sized: false, ..Default::default() };
    Response::new(200, "image/svg+xml; charset=utf-8", amcli_render::svg(&scene, &o))
}

/// The most elements the page may ask to have laid out at once.
///
/// The layered layout is superlinear, and beyond a few hundred boxes a drawing
/// stops being something anyone reads anyway. This is where the wait is still
/// under a second on a model the size of a real one.
const LAYOUT_CAP: usize = 600;

/// Where the boxes go, for the graph page.
///
/// The page chooses *what* to draw — it holds the whole model already, so
/// filtering on a keystroke costs nothing — and asks here *where*, because the
/// answer is [`amcli_view::layout`], the same code `view auto` runs. That is
/// the whole point: the graph is not a second opinion about how a model looks,
/// it is a view that has not been saved.
fn layout(m: &Model, req: &Request) -> Response {
    let (elements, relations) = indexed(m);
    let Some(want) = ranges(&req.param("e"), elements.len()) else {
        return Response::error(400, "`e` must be element indices, e.g. `0-9,12`");
    };
    if want.is_empty() {
        return Response::error(400, "nothing to lay out");
    }
    if want.len() > LAYOUT_CAP {
        return Response::error(413, &format!("more than {LAYOUT_CAP} elements"));
    }
    let algo = match req.param("algo").as_str() {
        "" => Algorithm::default(),
        other => match Algorithm::parse(other) {
            Some(a) => a,
            None => return Response::error(400, Algorithm::NAMES),
        },
    };
    // A view keeps every line clear of every box, and pays for it in width:
    // five hundred edges each reserving a lane in each of a dozen rows made a
    // drawing of the whole model twenty-four times wider than its own bound
    // allows, and every line across it that long. The graph is not a drawing
    // anyone saves, so it asks for the other side of that trade unless told
    // otherwise.
    let lanes = if req.param("lanes") == "reserved" { Lanes::Reserved } else { Lanes::Free };
    let hide = req.param("hiderel");
    let hidden: HashSet<&str> = hide.split(',').filter(|s| !s.is_empty()).collect();

    // Sized exactly as `view auto` sizes them, so a box on the graph is the
    // box the same concept would get on a view.
    let items: Vec<Item> = want
        .iter()
        .map(|&i| {
            let c = m.concept(elements[i]);
            let (w, h) = match &c.kind {
                ConceptKind::Element(e) => e.info().default_wh,
                _ => (120, 55),
            };
            let (w, h) = if (w, h) == (120, 55) { fit_size(&c.name) } else { (w, h) };
            Item { id: c.id.clone(), name: c.name.clone(), w, h }
        })
        .collect();

    // Every relationship of a type still shown whose two ends are both in the
    // set, as positions within it.
    let g = Graph::build(m);
    let mut at: HashMap<u32, usize> = HashMap::with_capacity(want.len());
    for (p, &i) in want.iter().enumerate() {
        at.insert(elements[i].0, p);
    }
    let mut drawn: Vec<(usize, usize, usize)> = Vec::new();
    for (ri, rid) in relations.iter().enumerate() {
        let ConceptKind::Relationship(rt) = &m.concept(*rid).kind else { continue };
        if hidden.contains(rel_short(*rt)) {
            continue;
        }
        let Some((s, t)) = g.ends(*rid) else { continue };
        if let (Some(&a), Some(&b)) = (at.get(&s.0), at.get(&t.0)) {
            drawn.push((ri, a, b));
        }
    }

    let edges: Vec<(usize, usize)> = drawn.iter().map(|&(_, a, b)| (a, b)).collect();
    let placed = place_with(&items, &edges, algo, lanes);

    let mut s = String::with_capacity(32 * want.len());
    let _ = write!(s, "{{\"algorithm\":{},\"nodes\":[", quote(placed.algorithm.as_str()));
    for (i, r) in placed.rects.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "[{},{},{},{}]", r.x, r.y, r.w, r.h);
    }
    s.push_str("],\"edges\":[");
    for (i, (ri, a, b)) in drawn.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "[{ri},{a},{b}]");
    }
    s.push_str("]}");
    Response::json(200, s)
}

/// A list of indices written as ranges — `0-271`, or `3,7-9,12`. The page
/// sends one per element it wants drawn, so the whole model is six characters
/// rather than a kilobyte of them. `None` if anything in it is not an index
/// below `max`, which is also what keeps a hand-typed URL from asking for an
/// enormous allocation.
fn ranges(s: &str, max: usize) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for part in s.split(',').filter(|p| !p.is_empty()) {
        let (lo, hi) = match part.split_once('-') {
            Some((a, b)) => (a.parse::<usize>().ok()?, b.parse::<usize>().ok()?),
            None => {
                let n = part.parse::<usize>().ok()?;
                (n, n)
            }
        };
        if lo > hi || hi >= max {
            return None;
        }
        out.extend(lo..=hi);
    }
    Some(out)
}

/// The elements and the relationships, each in the order `/api/model` numbers
/// them. One definition, because an index the page sends back has to mean the
/// same concept it meant on the way out.
fn indexed(m: &Model) -> (Vec<ConceptId>, Vec<ConceptId>) {
    let mut elements = Vec::new();
    let mut relations = Vec::new();
    for (cid, c) in m.concepts_with_ids() {
        match &c.kind {
            ConceptKind::Relationship(_) => relations.push(cid),
            // A relationship of a type this build does not know has no
            // notation to draw and no ends the matrix vouches for; it is
            // left out rather than drawn wrong. Unknown elements stay in.
            ConceptKind::Unknown { is_relationship: true, .. } => {}
            _ => elements.push(cid),
        }
    }
    (elements, relations)
}

/// How a relationship type is named in the JSON: `AccessRelationship` as
/// `Access`.
fn rel_short(rt: RelType) -> &'static str {
    rt.info().xsi.trim_start_matches("archimate:").trim_end_matches("Relationship")
}

/// The whole model, once per load. See the module docs for the shape.
pub fn model_json(m: &Model, checksum: &str) -> String {
    let g = Graph::build(m);
    let mut s = String::with_capacity(64 * 1024);

    s.push('{');
    let _ = write!(
        s,
        "\"model\":{{\"name\":{},\"path\":{},\"id\":{},\"version\":{},\"checksum\":{},\"zipped\":{},\"purpose\":{},\"properties\":",
        quote(&m.name()),
        quote(&m.path().display().to_string()),
        quote(&m.model_id()),
        quote(&m.version()),
        quote(checksum),
        m.is_zipped(),
        opt(m.purpose()),
    );
    pairs(&mut s, &m.properties(m.doc.root()));
    s.push('}');

    // Folders, by array index; the parent is an index too.
    let mut folder_idx: HashMap<u32, usize> = HashMap::new();
    for (i, (fid, _)) in m.folders_with_ids().enumerate() {
        folder_idx.insert(fid.0, i);
    }
    s.push_str(",\"folders\":[");
    for (i, (_, f)) in m.folders_with_ids().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"id\":{},\"name\":{},\"path\":{},\"type\":{},\"parent\":{}}}",
            quote(&f.id),
            quote(&f.name),
            quote(&f.path),
            quote(f.folder_type.as_str()),
            f.parent
                .and_then(|p| folder_idx.get(&p.0))
                .map(|i| i.to_string())
                .unwrap_or_else(|| "null".into())
        );
    }
    s.push(']');

    // Elements first, relationships second, each numbered within its own
    // array by `indexed`; a relationship's ends are element indices.
    let (elements, relations) = indexed(m);
    let elem_idx: HashMap<u32, usize> =
        elements.iter().enumerate().map(|(i, c)| (c.0, i)).collect();
    let rel_idx: HashMap<u32, usize> =
        relations.iter().enumerate().map(|(i, c)| (c.0, i)).collect();
    let mut view_idx: HashMap<u32, usize> = HashMap::new();
    for (i, (vid, _)) in m.views_with_ids().enumerate() {
        view_idx.insert(vid.0, i);
    }
    // Which elements and relationships each view draws, inverted from the
    // per-concept list the graph already keeps.
    let mut on_view_elems: Vec<Vec<usize>> = vec![Vec::new(); view_idx.len()];
    let mut on_view_rels: Vec<Vec<usize>> = vec![Vec::new(); view_idx.len()];

    s.push_str(",\"elements\":[");
    let mut first = true;
    for (cid, c) in m.concepts_with_ids() {
        if c.kind.is_relationship() {
            continue;
        }
        if !first {
            s.push(',');
        }
        first = false;
        let idx = elem_idx[&cid.0];
        for v in g.views_of(cid) {
            if let Some(vi) = view_idx.get(&v.0) {
                on_view_elems[*vi].push(idx);
            }
        }
        let has_doc = m.documentation(c.node).map(|d| !d.trim().is_empty()).unwrap_or(false);
        let _ = write!(
            s,
            "{{\"id\":{},\"name\":{},\"type\":{},\"layer\":{},\"folder\":{},\"doc\":{},\"props\":{}}}",
            quote(&c.id),
            quote(&c.name),
            quote(c.kind.name()),
            quote(c.kind.layer().map(|l| l.as_str()).unwrap_or("Other")),
            folder_idx.get(&c.folder.0).map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
            has_doc,
            m.properties(c.node).len()
        );
    }
    s.push(']');

    s.push_str(",\"relations\":[");
    first = true;
    for (cid, c) in m.concepts_with_ids() {
        let ConceptKind::Relationship(rt) = &c.kind else { continue };
        if !first {
            s.push(',');
        }
        first = false;
        let idx = rel_idx[&cid.0];
        for v in g.views_of(cid) {
            if let Some(vi) = view_idx.get(&v.0) {
                on_view_rels[*vi].push(idx);
            }
        }
        let (src, tgt) = match g.ends(cid) {
            Some((a, b)) => (end_index(&elem_idx, a.0), end_index(&elem_idx, b.0)),
            None => (-1, -1),
        };
        let access = m.doc.attr(c.node, "accessType").and_then(|a| a.parse::<i64>().ok());
        let directed = m.doc.attr(c.node, "directed").as_deref() == Some("true");
        let has_doc = m.documentation(c.node).map(|d| !d.trim().is_empty()).unwrap_or(false);
        let _ = write!(
            s,
            "{{\"id\":{},\"type\":{},\"name\":{},\"src\":{src},\"tgt\":{tgt},\"srcId\":{},\"tgtId\":{},\"access\":{},\"directed\":{directed},\"folder\":{},\"doc\":{},\"props\":{}}}",
            quote(&c.id),
            quote(rel_short(*rt)),
            quote(&c.name),
            quote(c.source.as_deref().unwrap_or("")),
            quote(c.target.as_deref().unwrap_or("")),
            access.map(|a| a.to_string()).unwrap_or_else(|| "null".into()),
            folder_idx.get(&c.folder.0).map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
            has_doc,
            m.properties(c.node).len()
        );
    }
    s.push(']');

    s.push_str(",\"views\":[");
    for (i, (vid, v)) in m.views_with_ids().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let vi = view_idx[&vid.0];
        let _ = write!(
            s,
            "{{\"id\":{},\"name\":{},\"viewpoint\":{},\"sketch\":{},\"folder\":{},\"elements\":{},\"relations\":{}}}",
            quote(&v.id),
            quote(&v.name),
            quote(&v.viewpoint),
            v.is_sketch,
            folder_idx.get(&v.folder.0).map(|i| i.to_string()).unwrap_or_else(|| "null".into()),
            nums(&on_view_elems[vi]),
            nums(&on_view_rels[vi]),
        );
    }
    s.push(']');

    // The notation, so the page draws its own figures the way the renderer
    // draws a view: same fills, same outlines, same icons, same line ends.
    s.push_str(",\"types\":{");
    for (i, e) in ElementType::ALL.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let info = e.info();
        let fill = if *e == ElementType::Junction {
            notation::BLACK
        } else {
            notation::layer_fill(info.layer)
        };
        let _ = write!(
            s,
            "{}:{{\"layer\":{},\"figure\":{},\"fill\":{},\"icon\":{}}}",
            quote(info.short),
            quote(info.layer.as_str()),
            quote(figure_name(notation::figure_of(*e))),
            quote(&fill.hex()),
            opt(amcli_view::icons::icon(*e).map(str::to_string)),
        );
    }
    s.push('}');

    s.push_str(",\"relTypes\":{");
    for (i, r) in RelType::ALL.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // Access and Association vary by attribute; the page adjusts those
        // two from `access` and `directed` on the relationship itself.
        let st = notation::rel_style(*r, Some(2), false);
        let _ = write!(
            s,
            "{}:{{\"dash\":{},\"source\":{},\"target\":{}}}",
            quote(rel_short(*r)),
            opt(st.dash.map(str::to_string)),
            quote(deco_name(st.source)),
            quote(deco_name(st.target)),
        );
    }
    s.push('}');

    s.push_str(",\"decos\":{");
    for (i, d) in [
        Deco::FilledDiamond,
        Deco::HollowDiamond,
        Deco::FilledArrow,
        Deco::OpenArrow,
        Deco::HollowTriangle,
        Deco::HalfArrow,
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            s.push(',');
        }
        let (pts, filled) = notation::deco_points(*d);
        let pts: Vec<String> = pts.iter().map(|(x, y)| format!("[{x},{y}]")).collect();
        let _ = write!(
            s,
            "{}:{{\"points\":[{}],\"filled\":{filled}}}",
            quote(deco_name(*d)),
            pts.join(",")
        );
    }
    s.push('}');

    s.push('}');
    s
}

fn end_index(elem_idx: &HashMap<u32, usize>, slot: u32) -> i64 {
    elem_idx.get(&slot).map(|i| *i as i64).unwrap_or(-1)
}

fn figure_name(f: Figure) -> &'static str {
    match f {
        Figure::Rect => "rect",
        Figure::RoundedRect => "rounded",
        Figure::Octagon => "octagon",
        Figure::Circle => "circle",
        Figure::Tabbed => "tabbed",
        Figure::Note => "note",
    }
}

fn deco_name(d: Deco) -> &'static str {
    match d {
        Deco::None => "none",
        Deco::FilledDiamond => "diamond-filled",
        Deco::HollowDiamond => "diamond-hollow",
        Deco::FilledArrow => "arrow-filled",
        Deco::OpenArrow => "arrow-open",
        Deco::HollowTriangle => "triangle-hollow",
        Deco::Ball => "ball",
        Deco::HalfArrow => "half-arrow",
    }
}

fn opt(v: Option<String>) -> String {
    v.map(|s| quote(&s)).unwrap_or_else(|| "null".into())
}

fn nums(v: &[usize]) -> String {
    let mut s = String::from("[");
    for (i, n) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "{n}");
    }
    s.push(']');
    s
}

fn pairs(s: &mut String, kv: &[(String, String)]) {
    s.push('[');
    for (i, (k, v)) in kv.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "[{},{}]", quote(k), quote(v));
    }
    s.push(']');
}
