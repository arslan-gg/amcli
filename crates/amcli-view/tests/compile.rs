//! Compiling a real Archi view: what the file records must be what comes out.

use amcli_model::Model;
use amcli_view::{Figure, compile};

fn corpus(name: &str) -> Model {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name);
    Model::open(p).unwrap()
}

fn view_named(m: &Model, name: &str) -> amcli_model::ViewId {
    m.views_with_ids().find(|(_, v)| v.name == name).map(|(i, _)| i).unwrap()
}

#[test]
fn nested_coordinates_are_summed_into_absolute_positions() {
    let m = corpus("testmodel1.archimate");
    let scene = compile(&m, view_named(&m, "2 Test Bounds and Images"));

    // The group sits at (156,204); the actor inside it at (36,42) relative.
    let group = scene.nodes.iter().find(|n| n.id == "f5b333fe").unwrap();
    assert_eq!((group.abs.x, group.abs.y), (156, 204));
    assert_eq!(group.depth, 0);

    let actor = scene.nodes.iter().find(|n| n.id == "eac5adf1").unwrap();
    assert_eq!((actor.abs.x, actor.abs.y), (192, 246), "156+36, 204+42");
    assert_eq!(actor.depth, 1);
    assert_eq!((actor.abs.w, actor.abs.h), (120, 55));
}

#[test]
fn missing_bounds_attributes_take_their_defaults() {
    let m = corpus("testmodel1.archimate");
    let scene = compile(&m, view_named(&m, "2 Test Bounds and Images"));

    // `<bounds width="193" height="85"/>` — x and y default to zero.
    let note = scene.nodes.iter().find(|n| n.id == "b8013607").unwrap();
    assert_eq!((note.abs.x, note.abs.y), (0, 0));
    assert_eq!((note.abs.w, note.abs.h), (193, 85));
    assert_eq!(note.figure, Figure::Note);
}

#[test]
fn colours_come_from_the_layer_and_the_border_is_derived_from_the_fill() {
    let m = corpus("testmodel1.archimate");
    let scene = compile(&m, view_named(&m, "2 Test Bounds and Images"));
    let actor = scene.nodes.iter().find(|n| n.id == "eac5adf1").unwrap();

    assert_eq!(actor.fill.hex(), "#ffffb5", "the Business layer fill");
    // Archi derives an element's border from its fill at 0.7. A renderer that
    // draws black borders looks wrong on every diagram.
    assert_eq!(actor.line.hex(), "#b2b27e");
}

#[test]
fn a_bendpoint_moves_the_line_where_archi_puts_it() {
    let m = corpus("testmodel1.archimate");
    let scene = compile(&m, view_named(&m, "3 Test Bounds and Images with Connections"));

    let edge = scene.edges.iter().find(|e| e.id == "6cb40cfb").unwrap();
    assert_eq!(edge.points.len(), 3, "two ends and one bendpoint");
    // The stored bendpoint is (516,285)/(516,177) relative to the two centres,
    // which puts the middle point well to the right of both boxes.
    assert!(edge.points[1].x > 400, "{:?}", edge.points);
    assert!(scene.content.w > 400, "the routed line widens the content box");
}

#[test]
fn behaviour_elements_are_rounded_and_motivation_elements_are_octagons() {
    use amcli_model::ElementType::*;
    use amcli_view::notation::figure_of;

    assert_eq!(figure_of(ApplicationComponent), Figure::Rect);
    assert_eq!(figure_of(DataObject), Figure::Rect);
    assert_eq!(figure_of(ApplicationService), Figure::RoundedRect);
    assert_eq!(figure_of(BusinessProcess), Figure::RoundedRect);
    assert_eq!(figure_of(BusinessEvent), Figure::RoundedRect);
    assert_eq!(figure_of(Goal), Figure::Octagon);
    assert_eq!(figure_of(Driver), Figure::Octagon);
    assert_eq!(figure_of(Junction), Figure::Circle);
    assert_eq!(figure_of(Grouping), Figure::Tabbed);
}

#[test]
fn access_direction_follows_the_access_type() {
    use amcli_model::RelType;
    use amcli_view::Deco;
    use amcli_view::notation::rel_style;

    // 0 is WRITE, not read: the arrow points at the data.
    let w = rel_style(RelType::Access, Some(0), false);
    assert_eq!(w.target, Deco::OpenArrow);
    assert_eq!(w.source, Deco::None);

    let r = rel_style(RelType::Access, Some(1), false);
    assert_eq!(r.source, Deco::OpenArrow, "a read points back at the function");
    assert_eq!(r.target, Deco::None);

    let rw = rel_style(RelType::Access, Some(3), false);
    assert_eq!((rw.source, rw.target), (Deco::OpenArrow, Deco::OpenArrow));

    // An undirected association draws no arrowhead at all.
    assert_eq!(rel_style(RelType::Association, None, false).target, Deco::None);
    assert_eq!(rel_style(RelType::Association, None, true).target, Deco::HalfArrow);
}

#[test]
fn every_view_in_the_corpus_compiles_without_losing_an_object() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut views = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("archimate") {
            continue;
        }
        let m = Model::open(&path).unwrap();
        for (id, v) in m.views_with_ids() {
            let scene = compile(&m, id);
            let objects = m
                .doc
                .descendants(v.node)
                .into_iter()
                .filter(|n| m.doc.local_name(*n) == "child")
                .count();
            assert_eq!(scene.nodes.len(), objects, "{path:?} view `{}`", v.name);
            views += 1;
        }
    }
    assert!(views >= 10, "only compiled {views} views");
}

#[test]
fn svg_output_is_byte_stable() {
    let m = corpus("testmodel1.archimate");
    let scene = compile(&m, view_named(&m, "2 Test Bounds and Images"));
    let o = amcli_render::Options::default();
    assert_eq!(amcli_render::svg(&scene, &o), amcli_render::svg(&scene, &o));

    let svg = amcli_render::svg(&scene, &o);
    assert!(svg.starts_with("<svg xmlns="));
    assert!(svg.contains(r#"viewBox="-10 -10"#), "the margin is applied: {}", &svg[..80]);
    // Edges are emitted after every node, because in GEF the connection layer
    // sits above the primary layer.
    assert!(svg.find("class=\"nodes\"") < svg.find("class=\"edges\""));
    assert!(!svg.contains("-0"), "negative zero is normalised away");
}
