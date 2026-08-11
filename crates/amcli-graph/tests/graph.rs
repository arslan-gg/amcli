use amcli_graph::{Dir, EdgeFilter, Graph, Resolution, Selector};
use amcli_model::{ConceptId, Model, RelType};

/// A small model with a known shape:
///
/// ```text
///   Payment API --Assignment--> Authorize --Access--> Payment Record
///                                   |
///                              Realization
///                                   v
///                          Card Authorization --Serving--> Settle Transaction
///
///   Ping <--Flow--> Pong          (a two-node cycle)
///   Lonely Component              (no relationships at all)
///   Payment API (BusinessProcess) (a deliberate duplicate name)
/// ```
fn fixture() -> Model {
    let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<archimate:model xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:archimate="http://www.archimatetool.com/archimate" name="Fixture" id="m1" version="5.0.0">
  <folder name="Business" id="fb" type="business">
    <element xsi:type="archimate:BusinessProcess" name="Settle Transaction" id="settle"/>
    <element xsi:type="archimate:BusinessProcess" name="Payment API" id="dupe"/>
  </folder>
  <folder name="Application" id="fa" type="application">
    <element xsi:type="archimate:ApplicationComponent" name="Payment API" id="api">
      <documentation>Public facade for card and SEPA payments.</documentation>
      <property key="owner" value="team-payments"/>
    </element>
    <element xsi:type="archimate:ApplicationFunction" name="Authorize" id="authz"/>
    <element xsi:type="archimate:ApplicationService" name="Card Authorization" id="cardauth"/>
    <element xsi:type="archimate:DataObject" name="Payment Record" id="record">
      <documentation>Stores every authorization attempt for reconciliation.</documentation>
    </element>
    <element xsi:type="archimate:ApplicationComponent" name="Lonely Component" id="lonely"/>
    <element xsi:type="archimate:ApplicationComponent" name="Ping" id="ping"/>
    <element xsi:type="archimate:ApplicationComponent" name="Pong" id="pong"/>
  </folder>
  <folder name="Relations" id="fr" type="relations">
    <element xsi:type="archimate:AssignmentRelationship" id="r1" source="api" target="authz"/>
    <element xsi:type="archimate:AccessRelationship" id="r2" source="authz" target="record" accessType="3"/>
    <element xsi:type="archimate:RealizationRelationship" id="r3" source="authz" target="cardauth"/>
    <element xsi:type="archimate:ServingRelationship" id="r4" source="cardauth" target="settle"/>
    <element xsi:type="archimate:FlowRelationship" id="r5" source="ping" target="pong"/>
    <element xsi:type="archimate:FlowRelationship" id="r6" source="pong" target="ping"/>
    <element xsi:type="archimate:AssociationRelationship" id="r7" source="api" target="nowhere"/>
  </folder>
  <folder name="Views" id="fv" type="diagrams"/>
</archimate:model>
"#;
    Model::from_bytes(src.as_bytes().to_vec(), "fixture.archimate").unwrap()
}

fn id(m: &Model, s: &str) -> ConceptId {
    m.concept_by_id(s).unwrap_or_else(|| panic!("no concept {s}"))
}

fn names(m: &Model, ids: impl IntoIterator<Item = ConceptId>) -> Vec<String> {
    let mut v: Vec<String> = ids.into_iter().map(|c| m.concept(c).name.clone()).collect();
    v.sort();
    v
}

#[test]
fn adjacency_follows_the_relationships() {
    let m = fixture();
    let g = Graph::build(&m);

    assert_eq!(g.degree(id(&m, "authz")), (1, 2), "one in, two out");
    let out = g.neighbors(id(&m, "authz"), Dir::Out, &EdgeFilter::default());
    assert_eq!(names(&m, out.iter().map(|a| a.other)), ["Card Authorization", "Payment Record"]);

    let both = g.neighbors(id(&m, "authz"), Dir::Both, &EdgeFilter::default());
    assert_eq!(both.len(), 3);

    let only_access = g.neighbors(id(&m, "authz"), Dir::Out, &EdgeFilter::only([RelType::Access]));
    assert_eq!(names(&m, only_access.iter().map(|a| a.other)), ["Payment Record"]);
}

#[test]
fn a_relationship_pointing_at_a_missing_id_is_reported_not_swallowed() {
    let m = fixture();
    let g = Graph::build(&m);
    // r7 targets an id that does not exist. Dropping it silently would hide a
    // real defect; refusing to load the model would be worse.
    assert_eq!(g.dangling().len(), 1);
    assert_eq!(m.concept(g.dangling()[0]).id, "r7");
    assert_eq!(g.degree(id(&m, "api")), (0, 1), "the dangling edge adds no degree");
}

#[test]
fn k_hop_returns_an_induced_subgraph_so_cycles_stay_visible() {
    let m = fixture();
    let g = Graph::build(&m);

    let sub = g.k_hop(&[id(&m, "ping")], 1, Dir::Both, &EdgeFilter::default(), 500);
    assert_eq!(names(&m, sub.nodes.iter().map(|(c, _)| *c)), ["Ping", "Pong"]);
    // Both flows are in the answer. A BFS tree would have kept only the one it
    // arrived by, and the cycle would be invisible.
    assert_eq!(sub.edges.len(), 2, "both directions of the cycle are present");

    let sub2 = g.k_hop(&[id(&m, "api")], 2, Dir::Out, &EdgeFilter::default(), 500);
    assert_eq!(
        names(&m, sub2.nodes.iter().map(|(c, _)| *c)),
        ["Authorize", "Card Authorization", "Payment API", "Payment Record"]
    );
    let depth_of =
        |n: &str| sub2.nodes.iter().find(|(c, _)| m.concept(*c).name == n).map(|(_, d)| *d);
    assert_eq!(depth_of("Payment API"), Some(0));
    assert_eq!(depth_of("Authorize"), Some(1));
    assert_eq!(depth_of("Payment Record"), Some(2));
}

#[test]
fn traversal_limits_are_reported_rather_than_hidden() {
    let m = fixture();
    let g = Graph::build(&m);
    let sub = g.k_hop(&[id(&m, "api")], 9, Dir::Both, &EdgeFilter::default(), 2);
    assert!(sub.truncated, "a silent cap reads as `that is all there is`");
    assert!(sub.nodes.len() <= 3);
}

#[test]
fn paths_are_found_and_reported_with_the_relationships_crossed() {
    let m = fixture();
    let g = Graph::build(&m);

    let p = g
        .shortest_path(id(&m, "api"), id(&m, "settle"), Dir::Out, &EdgeFilter::default())
        .expect("api reaches settle");
    assert_eq!(p.len(), 3, "three relationships crossed");
    assert_eq!(
        p.nodes.iter().map(|c| m.concept(*c).name.as_str()).collect::<Vec<_>>(),
        ["Payment API", "Authorize", "Card Authorization", "Settle Transaction"]
    );
    assert_eq!(m.concept(p.edges[0]).id, "r1");

    // Direction matters: nothing flows back the other way.
    assert!(
        g.shortest_path(id(&m, "settle"), id(&m, "api"), Dir::Out, &EdgeFilter::default())
            .is_none()
    );
    assert!(
        g.shortest_path(id(&m, "settle"), id(&m, "api"), Dir::Both, &EdgeFilter::default())
            .is_some()
    );
    assert!(
        g.shortest_path(id(&m, "api"), id(&m, "lonely"), Dir::Both, &EdgeFilter::default())
            .is_none()
    );

    let (paths, truncated) =
        g.all_paths(id(&m, "api"), id(&m, "settle"), 6, 10, Dir::Out, &EdgeFilter::default());
    assert_eq!(paths.len(), 1);
    assert!(!truncated);
}

#[test]
fn cycles_are_detected() {
    let m = fixture();
    let g = Graph::build(&m);
    let cycles = g.cycles(&EdgeFilter::default());
    assert_eq!(cycles.len(), 1);
    assert_eq!(names(&m, cycles[0].clone()), ["Ping", "Pong"]);

    // Restricting the edge types can make a cycle disappear, which is the point
    // of the filter.
    assert!(g.cycles(&EdgeFilter::only([RelType::Access])).is_empty());
}

#[test]
fn impact_says_why_each_concept_is_included() {
    let m = fixture();
    let g = Graph::build(&m);
    let (hits, _) = g.impact(&[id(&m, "record")], Dir::In, None, &EdgeFilter::default(), 500);

    let by_name: Vec<&str> = hits.iter().map(|(c, _, _)| m.concept(*c).name.as_str()).collect();
    assert!(by_name.contains(&"Authorize"));
    assert!(by_name.contains(&"Payment API"));
    assert!(!by_name.contains(&"Lonely Component"));

    let (_, depth, why) = hits.iter().find(|(c, _, _)| m.concept(*c).name == "Authorize").unwrap();
    assert_eq!(*depth, 1);
    assert_eq!(m.concept(why.unwrap()).id, "r2", "the relationship that pulled it in");
}

#[test]
fn stats_summarise_the_model() {
    let m = fixture();
    let g = Graph::build(&m);
    let s = g.stats();
    assert_eq!(s.elements, 9);
    assert_eq!(s.relationships, 7);
    // Lonely Component, and the BusinessProcess that shares a name with the
    // ApplicationComponent but is wired to nothing.
    assert_eq!(s.orphans, 2);
    assert_eq!(s.by_type.get("ApplicationComponent"), Some(&4));
    assert_eq!(s.by_layer.get(&amcli_model::Layer::Application), Some(&7));
}

#[test]
fn components_group_what_is_connected() {
    let m = fixture();
    let g = Graph::build(&m);
    let comps = g.components();
    // The main chain, the Ping/Pong pair, the duplicate-named process, and the
    // lonely component.
    assert_eq!(comps.len(), 4);
    assert_eq!(comps[0].len(), 5, "largest first: the whole authorisation chain");
}

#[test]
fn search_prefers_names_then_documentation_then_properties() {
    let m = fixture();
    let g = Graph::build(&m);

    let hits = g.search("payment", 10);
    assert_eq!(hits[0].field, amcli_graph::MatchField::Name);
    assert!(hits.iter().any(|h| m.concept(h.concept).name == "Payment Record"));

    // A word that appears only in documentation still finds its concept, and
    // the snippet says where the hit came from.
    let doc_hits = g.search("reconciliation", 10);
    assert_eq!(doc_hits.len(), 1);
    assert_eq!(doc_hits[0].field, amcli_graph::MatchField::Documentation);
    assert!(doc_hits[0].snippet.contains("reconciliation"));

    let owner = g.search("team-payments", 10);
    assert_eq!(owner.len(), 1);
    assert_eq!(owner[0].field, amcli_graph::MatchField::Property("owner".into()));

    assert!(g.search("nothing here", 10).is_empty());
}

// ---- selectors ------------------------------------------------------------

#[test]
fn an_ambiguous_name_is_reported_as_ambiguous_not_as_missing() {
    let m = fixture();
    let g = Graph::build(&m);

    // Two concepts are called "Payment API". The old Python tool answered
    // "not found" here, which sent you looking for the wrong problem.
    match Selector::parse("Payment API").resolve_one(&g) {
        Resolution::Ambiguous(c) => {
            assert_eq!(c.len(), 2);
            assert_eq!(names(&m, c), ["Payment API", "Payment API"]);
        }
        other => panic!("expected ambiguity, got {other:?}"),
    }

    // Qualifying by type resolves it, and so does the id.
    assert!(matches!(
        Selector::parse("ApplicationComponent:Payment API").resolve_one(&g),
        Resolution::One(_)
    ));
    assert!(matches!(Selector::parse("id:api").resolve_one(&g), Resolution::One(_)));
}

#[test]
fn a_miss_comes_back_with_something_to_try() {
    let m = fixture();
    let g = Graph::build(&m);
    match Selector::parse("Payment Recrd").resolve_one(&g) {
        Resolution::NotFound { suggestions } => {
            assert!(!suggestions.is_empty(), "a bare miss forces another round trip");
            assert!(names(&m, suggestions).contains(&"Payment Record".to_string()));
        }
        other => panic!("expected a miss, got {other:?}"),
    }
}

#[test]
fn globs_and_filters_select_sets() {
    let m = fixture();
    let g = Graph::build(&m);

    assert_eq!(Selector::parse("*Payment*").matches(&g).len(), 3);

    let apps = Selector::parse("type=ApplicationComponent").matches(&g);
    assert_eq!(apps.len(), 4);

    let named = Selector::parse("type=ApplicationComponent and name~pay").matches(&g);
    assert_eq!(names(&m, named), ["Payment API"]);

    let owned = Selector::parse("prop:owner=team-payments").matches(&g);
    assert_eq!(names(&m, owned), ["Payment API"]);

    let layer = Selector::parse("layer=Business").matches(&g);
    assert_eq!(layer.len(), 2);

    let busy = Selector::parse("deg>1").matches(&g);
    assert!(names(&m, busy).contains(&"Authorize".to_string()));

    let negated = Selector::parse("type=ApplicationComponent and not name~pay").matches(&g);
    assert_eq!(names(&m, negated), ["Lonely Component", "Ping", "Pong"]);

    let folder = Selector::parse("folder^=/Business").matches(&g);
    assert_eq!(folder.len(), 2);

    let re = Selector::parse("name=~^P(ing|ong)$").matches(&g);
    assert_eq!(names(&m, re), ["Ping", "Pong"]);

    let related = Selector::parse("out:Access~record").matches(&g);
    assert_eq!(names(&m, related), ["Authorize"]);
}

#[test]
fn a_bad_filter_explains_itself() {
    let err = amcli_graph::select::Expr::parse("bogus=1").unwrap_err().to_string();
    assert!(err.contains("unknown field"), "{err}");
    assert!(err.contains("layer"), "the message should list what is valid: {err}");

    assert!(amcli_graph::select::Expr::parse("name=~[unclosed").is_err());
}
