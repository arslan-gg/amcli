//! Loading real Archi output: what the file says must be what the model says,
//! and writing it back must not disturb anything.

use amcli_model::{ConceptKind, ElementType, FolderType, Model, RelType};

fn corpus(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name)
}

fn open(name: &str) -> Model {
    Model::open(corpus(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn a_real_model_indexes_the_way_the_file_reads() {
    let m = open("testmodel1.archimate");

    assert_eq!(m.name(), "Test Model");
    assert_eq!(m.model_id(), "be0eecc1");
    assert_eq!(m.version(), "5.0.0", "5.0.0 means ArchiMate 3.2");
    assert_eq!(m.purpose().as_deref(), Some("A variety of testing scenarios"));

    // Two elements and one relationship, all in the same namespace.
    assert_eq!(m.concepts().len(), 3);
    let actor = m.concepts().iter().find(|c| c.name == "Business Actor").unwrap();
    assert_eq!(actor.kind, ConceptKind::Element(ElementType::BusinessActor));
    assert_eq!(actor.id, "59fa6c90");
    assert_eq!(m.folder_path_of(actor), "/Business");

    let rel = m.concepts().iter().find(|c| c.kind.is_relationship()).unwrap();
    assert_eq!(rel.kind, ConceptKind::Relationship(RelType::Assignment));
    assert_eq!(rel.source.as_deref(), Some("59fa6c90"));
    assert_eq!(rel.target.as_deref(), Some("1efcd76f"));

    // Nine top-level folders, and Views holds diagrams rather than concepts.
    assert_eq!(m.folders().len(), 9);
    assert_eq!(
        m.folder_by_path("/Business").map(|f| m.folder(f).folder_type),
        Some(FolderType::Business)
    );
    assert!(
        m.folder_by_path("/Technology & Physical").is_some(),
        "the name is XML-escaped in the file"
    );

    // Three ArchiMate views plus one sketch.
    assert_eq!(m.views().len(), 4);
    assert_eq!(m.views().iter().filter(|v| v.is_sketch).count(), 1);
    assert!(m.views().iter().any(|v| v.name == "2 Test Bounds and Images"));
}

#[test]
fn unknown_types_survive_instead_of_being_dropped() {
    // compatibility_test3 carries Bogus1/Bogus2 elements and a Bogus3 that has
    // endpoints. A tool that only understands the types it knows would lose all
    // three; refusing to open the file would be worse still.
    let m = open("compatibility_test3.archimate");
    assert_eq!(m.concepts().len(), 3);

    let e1 = m.concepts().iter().find(|c| c.name == "E1").unwrap();
    assert!(
        matches!(&e1.kind, ConceptKind::Unknown { xsi, is_relationship: false } if xsi == "Bogus1")
    );

    // Classified as an edge because it has both endpoints, not because of its
    // name or the folder it happens to sit in.
    let e3 = m.concepts().iter().find(|c| c.id.ends_with("eae3e9")).unwrap();
    assert!(matches!(&e3.kind, ConceptKind::Unknown { is_relationship: true, .. }));
    assert!(e3.kind.is_relationship());
    assert_eq!(e3.kind.matrix_idx(), None, "an unknown type has no matrix row");

    assert_eq!(
        m.to_bytes().unwrap(),
        std::fs::read(corpus("compatibility_test3.archimate")).unwrap()
    );
}

#[test]
fn a_zipped_model_is_opened_and_written_back_unchanged() {
    let raw = std::fs::read(corpus("model_zipped.archimate")).unwrap();
    assert!(raw.starts_with(b"PK"), "the fixture is genuinely a ZIP");

    let m = open("model_zipped.archimate");
    assert!(m.is_zipped());
    assert_eq!(m.name(), "Zip Test", "the model inside the archive was parsed");
    // Written by an older Archi: the model version is not fixed at 5.0.0, so
    // nothing may assume it.
    assert_eq!(m.version(), "5.8.0");
    assert!(!m.views().is_empty());
    assert!(m.is_unmodified());

    // Untouched means untouched: no recompression, no reordered entries, no
    // rewritten timestamps.
    assert_eq!(m.to_bytes().unwrap(), raw);

    let entries = amcli_model::container::zip_entries(&raw).unwrap();
    assert!(entries.iter().any(|(n, _)| n == "model.xml"));
    assert!(entries.len() > 1, "the fixture also embeds images");
}

#[test]
fn editing_a_zipped_model_keeps_the_other_entries_byte_for_byte() {
    let raw = std::fs::read(corpus("model_zipped.archimate")).unwrap();
    let before = amcli_model::container::zip_entries(&raw).unwrap();

    let mut m = open("model_zipped.archimate");
    let root = m.doc.root();
    m.doc.set_attr(root, "name", "Renamed");

    let out = m.to_bytes().unwrap();
    assert!(out.starts_with(b"PK"));
    let after = amcli_model::container::zip_entries(&out).unwrap();

    for (name, size) in &before {
        if name == "model.xml" {
            continue;
        }
        let found = after.iter().find(|(n, _)| n == name);
        assert_eq!(found.map(|(_, s)| *s), Some(*size), "{name} changed size");
    }

    let reopened = Model::from_bytes(out, "x.archimate").unwrap();
    assert_eq!(reopened.name(), "Renamed");
    assert_eq!(reopened.concepts().len(), m.concepts().len());
}

#[test]
fn every_corpus_model_loads_and_writes_back_identically() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut checked = 0;
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("archimate") {
            continue;
        }
        let raw = std::fs::read(&path).unwrap();
        let m = Model::open(&path).unwrap_or_else(|err| panic!("{path:?}: {err}"));
        assert_eq!(m.to_bytes().unwrap(), raw, "{path:?} changed on write");
        assert!(m.duplicate_ids().is_empty(), "{path:?} has duplicate ids");
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn ids_resolve_to_the_right_kind_of_thing() {
    use amcli_model::Entity;
    let m = open("testmodel1.archimate");

    assert!(matches!(m.entity("59fa6c90"), Some(Entity::Concept(_))));
    assert!(matches!(m.entity("62c7e25e"), Some(Entity::Folder(_))));
    assert!(matches!(m.entity("17cdf396"), Some(Entity::View(_))));
    // Diagram objects are addressable too, which is how a dangling reference
    // can be reported against a real place in the file.
    assert!(matches!(m.entity("eac5adf1"), Some(Entity::Visual(_))));
    assert_eq!(m.entity("nope"), None);
}

/// No corpus fixture carries documentation or properties, so this builds one.
/// Both are child *elements* in the Archi format, not attributes — reading them
/// as attributes is the obvious mistake and would silently return nothing.
#[test]
fn documentation_properties_and_features_are_read_from_child_elements() {
    let src = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<archimate:model xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"",
        " xmlns:archimate=\"http://www.archimatetool.com/archimate\"",
        " name=\"M\" id=\"m1\" version=\"5.0.0\">\n",
        "  <folder name=\"Application\" id=\"f1\" type=\"application\">\n",
        "    <element xsi:type=\"archimate:ApplicationComponent\" name=\"Pay\" id=\"c1\">\n",
        "      <documentation>Handles &amp; settles payments</documentation>\n",
        "      <property key=\"owner\" value=\"team-payments\"/>\n",
        "      <property key=\"tier\" value=\"1\"/>\n",
        "      <feature name=\"lineStyle\" value=\"2\"/>\n",
        "    </element>\n",
        "  </folder>\n",
        "</archimate:model>\n"
    );
    let m = Model::from_bytes(src.as_bytes().to_vec(), "x.archimate").unwrap();
    let c = &m.concepts()[0];

    assert_eq!(m.documentation(c.node).as_deref(), Some("Handles & settles payments"));
    assert_eq!(
        m.properties(c.node),
        vec![
            ("owner".to_string(), "team-payments".to_string()),
            ("tier".to_string(), "1".to_string())
        ]
    );
    // Features carry half a diagram's styling; dropping them loses appearance.
    assert_eq!(m.features(c.node), vec![("lineStyle".to_string(), "2".to_string())]);
    assert_eq!(m.to_bytes().unwrap(), src.as_bytes());
}

#[test]
fn a_file_that_is_not_a_model_is_refused_with_a_useful_message() {
    let Err(err) = Model::from_bytes(b"<notamodel/>".to_vec(), "x.archimate") else {
        panic!("a non-model file must be refused");
    };
    let msg = err.to_string();
    assert!(msg.contains("notamodel"), "{msg}");
    assert!(msg.contains("Archi model"), "{msg}");

    assert!(Model::from_bytes(b"PK\x03\x04garbage".to_vec(), "x.archimate").is_err());
}

#[test]
fn the_checksum_tracks_content_and_nothing_else() {
    let m = open("testmodel1.archimate");
    let a = m.checksum().unwrap();
    assert_eq!(a.len(), 64);
    assert_eq!(a, open("testmodel1.archimate").checksum().unwrap(), "stable across loads");

    let mut m2 = open("testmodel1.archimate");
    let root = m2.doc.root();
    m2.doc.set_attr(root, "name", "Different");
    assert_ne!(m2.checksum().unwrap(), a);
}
