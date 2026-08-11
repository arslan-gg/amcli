use amcli_graph::Graph;
use amcli_model::Model;
use amcli_validate::{Fixability, Level, Severity, fix_safe, validate};

fn model(src: &str) -> Model {
    Model::from_bytes(src.as_bytes().to_vec(), "t.archimate").unwrap()
}

fn wrap(body: &str) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<archimate:model xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
            " xmlns:archimate=\"http://www.archimatetool.com/archimate\"",
            " name=\"T\" id=\"m1\" version=\"5.0.0\">\n{}</archimate:model>\n"
        ),
        body
    )
}

fn report(src: &str, level: Level) -> amcli_validate::Report {
    let m = model(src);
    let g = Graph::build(&m);
    validate(&m, &g, level)
}

fn codes(r: &amcli_validate::Report) -> Vec<&str> {
    let mut v: Vec<&str> = r.findings.iter().map(|f| f.code).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn a_clean_model_reports_nothing() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationFunction\" name=\"F\" id=\"f\"/>\n",
        "    <element xsi:type=\"archimate:DataObject\" name=\"D\" id=\"d\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Relations\" id=\"fr\" type=\"relations\">\n",
        "    <element xsi:type=\"archimate:AccessRelationship\" id=\"r\" source=\"f\" target=\"d\" accessType=\"3\"/>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::All);
    assert!(r.is_clean(), "{:?}", r.findings);
    assert_eq!(r.warnings(), 0, "{:?}", r.findings);
}

#[test]
fn a_relationship_the_standard_forbids_is_an_error_that_names_the_alternative() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:DataObject\" name=\"D\" id=\"d\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"C\" id=\"c\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Relations\" id=\"fr\" type=\"relations\">\n",
        "    <element xsi:type=\"archimate:ServingRelationship\" id=\"bad\" source=\"d\" target=\"c\"/>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::Rules);
    let f = r.findings.iter().find(|f| f.code == "REL2001").expect("the violation is reported");
    assert_eq!(f.severity, Severity::Error);
    assert!(f.message.contains("permitted here: Association"), "{}", f.message);
    assert_eq!(f.fixability, Fixability::Destructive, "removing modelling is never automatic");
    assert!(f.fix.as_deref().unwrap().contains("--type Association"));

    // The finding points at a place in the file, not just at an id.
    assert!(f.line > 0, "a finding without a location is hard to act on");
}

#[test]
fn duplicate_relationships_and_mixed_junctions_are_caught() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationProcess\" name=\"A\" id=\"a\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationProcess\" name=\"B\" id=\"b\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Other\" id=\"fo\" type=\"other\">\n",
        "    <element xsi:type=\"archimate:Junction\" name=\"J\" id=\"j\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Relations\" id=\"fr\" type=\"relations\">\n",
        "    <element xsi:type=\"archimate:TriggeringRelationship\" id=\"t1\" source=\"a\" target=\"b\"/>\n",
        "    <element xsi:type=\"archimate:TriggeringRelationship\" id=\"t2\" source=\"a\" target=\"b\"/>\n",
        "    <element xsi:type=\"archimate:TriggeringRelationship\" id=\"t3\" source=\"a\" target=\"j\"/>\n",
        "    <element xsi:type=\"archimate:FlowRelationship\" id=\"t4\" source=\"j\" target=\"b\"/>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::Rules);
    assert!(codes(&r).contains(&"REL2002"), "duplicate relationship");
    let j = r.findings.iter().find(|f| f.code == "REL2003").expect("mixed junction");
    assert!(j.message.contains("mixes Triggering with Flow"), "{}", j.message);
}

/// The check nothing else does: a view that points at something no longer there.
#[test]
fn dangling_view_references_are_errors_because_archi_will_not_open_the_model() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\"/>\n",
        "  <folder name=\"Views\" id=\"fv\" type=\"diagrams\">\n",
        "    <element xsi:type=\"archimate:ArchimateDiagramModel\" name=\"V\" id=\"v\">\n",
        "      <child xsi:type=\"archimate:DiagramObject\" id=\"o1\" archimateElement=\"ghost\">\n",
        "        <bounds x=\"0\" y=\"0\" width=\"120\" height=\"55\"/>\n",
        "        <sourceConnection xsi:type=\"archimate:Connection\" id=\"c1\" source=\"o1\" target=\"o2\" archimateRelationship=\"phantom\"/>\n",
        "      </child>\n",
        "    </element>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::Integrity);
    let found = codes(&r);
    assert!(found.contains(&"REF3020"), "missing element behind a diagram object: {found:?}");
    assert!(found.contains(&"REF3021"), "missing relationship behind a connection: {found:?}");
    // The connection's target object is not on the view; its source is.
    assert!(found.contains(&"REF3023"), "connection pointing outside the view: {found:?}");
    assert_eq!(r.errors(), 3);
}

#[test]
fn a_stale_target_connections_mirror_is_reported() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"A\" id=\"a\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"B\" id=\"b\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Relations\" id=\"fr\" type=\"relations\">\n",
        "    <element xsi:type=\"archimate:ServingRelationship\" id=\"r\" source=\"a\" target=\"b\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Views\" id=\"fv\" type=\"diagrams\">\n",
        "    <element xsi:type=\"archimate:ArchimateDiagramModel\" name=\"V\" id=\"v\">\n",
        "      <child xsi:type=\"archimate:DiagramObject\" id=\"o1\" archimateElement=\"a\">\n",
        "        <bounds x=\"0\" y=\"0\" width=\"120\" height=\"55\"/>\n",
        "        <sourceConnection xsi:type=\"archimate:Connection\" id=\"c1\" source=\"o1\" target=\"o2\" archimateRelationship=\"r\"/>\n",
        "      </child>\n",
        "      <child xsi:type=\"archimate:DiagramObject\" id=\"o2\" targetConnections=\"c1 c-gone\" archimateElement=\"b\">\n",
        "        <bounds x=\"200\" y=\"0\" width=\"120\" height=\"55\"/>\n",
        "      </child>\n",
        "    </element>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::Integrity);
    let f = r.findings.iter().find(|f| f.code == "REF3022").expect("the mirror is stale");
    assert_eq!(f.fixability, Fixability::Safe, "a derived value has exactly one right answer");

    // And --fix repairs it, because there is nothing to decide.
    let mut m = model(&src);
    let done = fix_safe(&mut m);
    assert_eq!(done.recomputed_views, ["v"]);
    let g = Graph::build(&m);
    assert!(!codes(&validate(&m, &g, Level::Integrity)).contains(&"REF3022"));
    assert!(
        String::from_utf8(m.to_bytes().unwrap()).unwrap().contains(r#"targetConnections="c1""#)
    );
}

#[test]
fn fix_removes_orphaned_visuals_and_leaves_everything_else_alone() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"A\" id=\"a\"/>\n",
        "  </folder>\n",
        "  <folder name=\"Views\" id=\"fv\" type=\"diagrams\">\n",
        "    <element xsi:type=\"archimate:ArchimateDiagramModel\" name=\"V\" id=\"v\">\n",
        "      <child xsi:type=\"archimate:DiagramObject\" id=\"keep\" archimateElement=\"a\">\n",
        "        <bounds x=\"0\" y=\"0\" width=\"120\" height=\"55\"/>\n",
        "      </child>\n",
        "      <child xsi:type=\"archimate:DiagramObject\" id=\"orphan\" archimateElement=\"ghost\">\n",
        "        <bounds x=\"200\" y=\"0\" width=\"120\" height=\"55\"/>\n",
        "      </child>\n",
        "    </element>\n",
        "  </folder>\n",
    ));
    let mut m = model(&src);
    let done = fix_safe(&mut m);
    assert_eq!(done.orphan_objects, ["orphan"]);

    let out = String::from_utf8(m.to_bytes().unwrap()).unwrap();
    assert!(!out.contains("orphan"));
    assert!(out.contains(r#"id="keep""#), "the healthy object is untouched");

    let g = Graph::build(&m);
    assert!(validate(&m, &g, Level::Integrity).is_clean());
}

#[test]
fn hygiene_findings_are_warnings_not_errors() {
    let src = wrap(concat!(
        "  <folder name=\"Application\" id=\"fa\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"Alone\" id=\"a\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"\" id=\"b\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"Twin\" id=\"c\"/>\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"Twin\" id=\"d\"/>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::All);
    let found = codes(&r);
    assert!(found.contains(&"LNT4001"), "orphan");
    assert!(found.contains(&"LNT4002"), "unnamed");
    assert!(found.contains(&"LNT4004"), "duplicate name");
    assert!(r.is_clean(), "hygiene is advice, not breakage");

    // Running only the earlier layers says nothing about hygiene.
    assert!(codes(&report(&src, Level::Integrity)).is_empty());
}

#[test]
fn an_unknown_type_is_noted_but_never_treated_as_broken() {
    let src = wrap(concat!(
        "  <folder name=\"Technology &amp; Physical\" id=\"ft\" type=\"technology\">\n",
        "    <element xsi:type=\"archimate:Bogus1\" name=\"E\" id=\"e\"/>\n",
        "  </folder>\n",
    ));
    let r = report(&src, Level::All);
    let f = r.findings.iter().find(|f| f.code == "TYP1001").expect("noted");
    assert_eq!(f.severity, Severity::Info);
    assert!(r.is_clean(), "a model this build does not fully understand is not a broken model");
}

/// Real Archi output is referentially sound: no dangling reference, no stale
/// mirror, no duplicate id anywhere in the corpus.
#[test]
fn real_archi_output_is_referentially_sound() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut checked = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("archimate") {
            continue;
        }
        let m = Model::open(&path).unwrap();
        let g = Graph::build(&m);
        let r = validate(&m, &g, Level::Integrity);
        let refs: Vec<&amcli_validate::Finding> =
            r.findings.iter().filter(|f| f.code.starts_with("REF")).collect();
        assert!(refs.is_empty(), "{path:?} reports {refs:#?}");
        checked += 1;
    }
    assert_eq!(checked, 9);
}

/// Referential soundness and ArchiMate legality are different things, and this
/// corpus proves it: `testDeleteHandler` is a hand-built fixture for exercising
/// deletion, and it carries two relationships the 3.2 matrix forbids — Access
/// from an Artifact to a Contract, and Specialization from an
/// ApplicationComponent to an ApplicationCollaboration. The file loads in Archi
/// perfectly well. That is exactly the class of drift this validator exists to
/// surface, and it is why a rule violation is reported rather than refused at
/// load.
#[test]
fn rule_violations_exist_in_the_wild_and_are_reported() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus/testDeleteHandler.archimate");
    let m = Model::open(&path).unwrap();
    let g = Graph::build(&m);
    let r = validate(&m, &g, Level::Rules);

    let mut violations: Vec<&str> =
        r.findings.iter().filter(|f| f.code == "REL2001").map(|f| f.entity.as_str()).collect();
    violations.sort();
    assert_eq!(violations, ["d934bb5f", "ff805459"]);
    for f in r.findings.iter().filter(|f| f.code == "REL2001") {
        assert!(f.line > 0, "each violation points at a line in the file");
        assert!(f.fix.is_some(), "and carries a command that addresses it");
    }
}
