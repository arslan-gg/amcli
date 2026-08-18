//! Turning a stored view into something drawable.
//!
//! The geometry is already in the file — Archi records every bound and every
//! bendpoint — so nothing here lays anything out. Compiling a view is resolving
//! what is written into absolute coordinates: parent-relative origins summed,
//! `-1` sizes replaced by the figure default, and relative bendpoints turned
//! into a polyline.
//!
//! Laying out a *new* view is a separate job; see [`layout`].

pub mod geometry;
pub mod icons;
pub mod layout;
pub mod notation;

use amcli_model::{ConceptKind, ElementType, Model, RelType, ViewId};
use amcli_xml::NodeId;

pub use geometry::{Bendpoint, Pt, Rect};
pub use notation::{Deco, Figure, Rgb};

/// A view resolved into absolute coordinates and concrete styling. Nothing
/// downstream needs the model or the document again.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub view_id: String,
    pub view_name: String,
    pub viewpoint: String,
    /// Bounding box of everything drawn.
    pub content: Rect,
    /// Painter's order: tree pre-order, so a child covers its parent.
    pub nodes: Vec<Node>,
    /// Drawn after every node. In GEF the connection layer sits above the
    /// primary layer, so an edge is never hidden behind a box it crosses.
    pub edges: Vec<Edge>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    /// The concept this displays, when it displays one.
    pub concept_id: Option<String>,
    pub figure: Figure,
    pub label: String,
    /// A note's body text.
    pub content: String,
    pub abs: Rect,
    pub depth: usize,
    pub fill: Rgb,
    pub line: Rgb,
    /// 0..255; Archi tracks fill and line opacity separately.
    pub alpha: u8,
    pub line_alpha: u8,
    pub line_width: u32,
    /// 1 left, 2 centre, 4 right.
    pub text_align: u8,
    pub type_name: String,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub id: String,
    pub relationship_id: Option<String>,
    pub label: String,
    pub points: Vec<Pt>,
    pub dash: Option<&'static str>,
    pub source_deco: Deco,
    pub target_deco: Deco,
    pub line: Rgb,
    pub line_width: u32,
}

/// Resolve a view into a scene.
pub fn compile(m: &Model, view: ViewId) -> Scene {
    let v = m.view(view);
    let mut scene = Scene {
        view_id: v.id.clone(),
        view_name: v.name.clone(),
        viewpoint: v.viewpoint.clone(),
        ..Default::default()
    };

    // Objects are indexed by id while walking, because a connection names its
    // endpoints by id and may be declared before either of them.
    let mut bounds_of: std::collections::HashMap<String, Rect> = Default::default();
    let children: Vec<NodeId> = m.doc.children(v.node).collect();
    for c in children {
        walk(m, c, 0, 0, 0, &mut scene, &mut bounds_of);
    }

    collect_edges(m, v.node, &bounds_of, &mut scene);

    scene.content = scene
        .nodes
        .iter()
        .map(|n| n.abs)
        .chain(
            scene
                .edges
                .iter()
                .flat_map(|e| e.points.iter().map(|p| Rect { x: p.x, y: p.y, w: 0, h: 0 })),
        )
        .reduce(|a, b| a.union(b))
        .unwrap_or_default();

    scene
}

fn walk(
    m: &Model,
    node: NodeId,
    ox: i32,
    oy: i32,
    depth: usize,
    scene: &mut Scene,
    bounds_of: &mut std::collections::HashMap<String, Rect>,
) {
    if m.doc.local_name(node) != "child" {
        return;
    }
    let id = m.doc.attr(node, "id").unwrap_or_default();
    let xsi = m.doc.attr(node, "xsi:type").unwrap_or_default();
    let bare = xsi.trim_start_matches("archimate:");
    let concept_id = m.doc.attr(node, "archimateElement");
    let concept = concept_id.as_deref().and_then(|c| m.concept_by_id(c)).map(|c| m.concept(c));

    let (dw, dh) = default_size(bare, concept.map(|c| &c.kind));
    let b = read_bounds(m, node, dw, dh);
    // Child coordinates are relative to the parent's origin.
    let abs = Rect { x: ox + b.x, y: oy + b.y, w: b.w, h: b.h };

    let (figure, fill) = match (bare, concept.map(|c| &c.kind)) {
        (_, Some(ConceptKind::Element(e))) => {
            let f = if *e == ElementType::Junction {
                notation::BLACK
            } else {
                notation::layer_fill(e.info().layer)
            };
            (notation::figure_of(*e), f)
        }
        ("Note", _) => (Figure::Note, notation::WHITE),
        ("Group", _) => (Figure::Tabbed, notation::WHITE),
        _ => (Figure::Rect, notation::WHITE),
    };

    let explicit_fill = m.doc.attr(node, "fillColor").and_then(|s| parse_hex(&s));
    let fill = explicit_fill.unwrap_or(fill);
    let line =
        m.doc.attr(node, "lineColor").and_then(|s| parse_hex(&s)).unwrap_or_else(|| match figure {
            // Archi derives an element's border from its fill; notes, groups
            // and connections use the plain default.
            Figure::Note | Figure::Tabbed => notation::DEFAULT_LINE,
            _ => fill.derived_line(),
        });

    scene.nodes.push(Node {
        id: id.clone(),
        concept_id: concept_id.clone(),
        figure,
        label: concept
            .map(|c| c.name.clone())
            .or_else(|| m.doc.attr(node, "name"))
            .unwrap_or_default(),
        content: m.doc.child_named(node, "content").map(|n| m.doc.text(n)).unwrap_or_default(),
        abs,
        depth,
        fill,
        line,
        alpha: m.doc.attr(node, "alpha").and_then(|s| s.parse().ok()).unwrap_or(255),
        line_alpha: m.doc.attr(node, "lineAlpha").and_then(|s| s.parse().ok()).unwrap_or(255),
        line_width: m.doc.attr(node, "lineWidth").and_then(|s| s.parse().ok()).unwrap_or(1),
        text_align: m.doc.attr(node, "textAlignment").and_then(|s| s.parse().ok()).unwrap_or(2),
        type_name: concept.map(|c| c.kind.name().to_string()).unwrap_or_else(|| bare.to_string()),
    });
    bounds_of.insert(id, abs);

    for c in m.doc.children(node).collect::<Vec<_>>() {
        walk(m, c, abs.x, abs.y, depth + 1, scene, bounds_of);
    }
}

fn collect_edges(
    m: &Model,
    view_node: NodeId,
    bounds_of: &std::collections::HashMap<String, Rect>,
    scene: &mut Scene,
) {
    for n in m.doc.descendants(view_node) {
        if m.doc.local_name(n) != "sourceConnection" {
            continue;
        }
        let id = m.doc.attr(n, "id").unwrap_or_default();
        let src = m.doc.attr(n, "source").unwrap_or_default();
        let tgt = m.doc.attr(n, "target").unwrap_or_default();
        let rel_id = m.doc.attr(n, "archimateRelationship");

        // A connection may end on another connection. Those have no bounds of
        // their own, so the edge is skipped rather than drawn to the origin.
        let (Some(sb), Some(tb)) = (bounds_of.get(&src), bounds_of.get(&tgt)) else {
            scene
                .warnings
                .push(format!("connection {id} ends on something with no bounds; not drawn"));
            continue;
        };

        let bendpoints: Vec<Bendpoint> = m
            .doc
            .children(n)
            .filter(|c| m.doc.local_name(*c) == "bendpoint")
            .map(|c| Bendpoint {
                start_x: attr_i32(m, c, "startX"),
                start_y: attr_i32(m, c, "startY"),
                end_x: attr_i32(m, c, "endX"),
                end_y: attr_i32(m, c, "endY"),
            })
            .collect();

        let points = geometry::route(*sb, *tb, &bendpoints);
        if points.first() == points.last() && bendpoints.is_empty() {
            scene
                .warnings
                .push(format!("connection {id} is a self-loop with no bendpoints; it draws as a point, which is what Archi does"));
        }

        let rel = rel_id.as_deref().and_then(|r| m.concept_by_id(r)).map(|c| m.concept(c));
        let (rel_type, access, directed, label) = match rel {
            Some(c) => (
                match &c.kind {
                    ConceptKind::Relationship(r) => Some(*r),
                    _ => None,
                },
                m.doc.attr(c.node, "accessType").and_then(|s| s.parse::<i64>().ok()),
                m.doc.attr(c.node, "directed").as_deref() == Some("true"),
                c.name.clone(),
            ),
            None => (None, None, false, String::new()),
        };

        let style = rel_type
            .map(|r| notation::rel_style(r, access, directed))
            .unwrap_or(notation::RelStyle { dash: None, source: Deco::None, target: Deco::None });

        scene.edges.push(Edge {
            id,
            relationship_id: rel_id,
            label,
            points,
            dash: style.dash,
            source_deco: style.source,
            target_deco: style.target,
            line: m
                .doc
                .attr(n, "lineColor")
                .and_then(|s| parse_hex(&s))
                .unwrap_or(notation::DEFAULT_LINE),
            line_width: m.doc.attr(n, "lineWidth").and_then(|s| s.parse().ok()).unwrap_or(1),
        });
    }
}

fn attr_i32(m: &Model, n: NodeId, name: &str) -> i32 {
    m.doc.attr(n, name).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// `<bounds>` is a child element, and `-1` means "the figure's default size".
fn read_bounds(m: &Model, node: NodeId, dw: i32, dh: i32) -> Rect {
    let Some(b) = m.doc.child_named(node, "bounds") else {
        return Rect { x: 0, y: 0, w: dw, h: dh };
    };
    let w = attr_or(m, b, "width", -1);
    let h = attr_or(m, b, "height", -1);
    Rect {
        x: attr_or(m, b, "x", 0),
        y: attr_or(m, b, "y", 0),
        w: if w >= 0 { w } else { dw },
        h: if h >= 0 { h } else { dh },
    }
}

fn attr_or(m: &Model, n: NodeId, name: &str, default: i32) -> i32 {
    m.doc.attr(n, name).and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn default_size(bare: &str, kind: Option<&ConceptKind>) -> (i32, i32) {
    if let Some(ConceptKind::Element(e)) = kind {
        return e.info().default_wh;
    }
    match bare {
        "Note" => geometry::NOTE_SIZE,
        "Group" => geometry::GROUP_SIZE,
        "DiagramModelImage" => geometry::IMAGE_SIZE,
        _ => geometry::ELEMENT_SIZE,
    }
}

fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    Some(Rgb(
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

/// Relationship types that carry containment, used when deciding what a
/// generated view should nest.
pub const NESTING: [RelType; 2] = [RelType::Composition, RelType::Aggregation];
