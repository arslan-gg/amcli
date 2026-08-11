//! The generated tables decide what edits are legal, so they get asserted
//! against facts read straight out of the ArchiMate 3.2 matrix rather than
//! against what the generator happened to produce.

use amcli_model::generated::{matrix, types, viewpoints};
use amcli_model::{ElementType, FolderType, Layer, RelType};

fn idx(e: ElementType) -> u8 {
    e.info().matrix_idx
}

fn allows(src: ElementType, dst: ElementType, rel: RelType) -> bool {
    matrix::allows(idx(src), idx(dst), rel)
}

#[test]
fn the_inventory_is_archimate_3_2() {
    assert_eq!(ElementType::ALL.len(), 61, "60 element types plus Junction");
    assert_eq!(RelType::ALL.len(), 11);
    assert_eq!(matrix::CONCEPTS, 62, "61 concepts plus the Relationship pseudo-concept");
}

#[test]
fn layer_membership_matches_the_specification() {
    let count = |l: Layer| ElementType::ALL.iter().filter(|e| e.info().layer == l).count();
    assert_eq!(count(Layer::Strategy), 4);
    assert_eq!(count(Layer::Business), 13);
    assert_eq!(count(Layer::Application), 9);
    assert_eq!(count(Layer::Technology), 13);
    assert_eq!(count(Layer::Physical), 4);
    assert_eq!(count(Layer::Motivation), 10);
    assert_eq!(count(Layer::ImplementationMigration), 5);
    assert_eq!(count(Layer::Other), 3);
}

/// Mirrors `ArchimateModel.getDefaultFolderForObject`. The two cases worth
/// pinning are the ones a reasonable person would get wrong.
#[test]
fn folder_assignment_matches_archi() {
    assert_eq!(Layer::Physical.folder(), FolderType::Technology, "Physical shares Technology");
    assert_eq!(ElementType::Junction.info().home, FolderType::Other);
    assert_eq!(ElementType::Location.info().home, FolderType::Other);
    assert_eq!(ElementType::Grouping.info().home, FolderType::Other);
    assert_eq!(ElementType::Material.info().home, FolderType::Technology);
    assert_eq!(ElementType::ApplicationComponent.info().home, FolderType::Application);

    // The numeric values are written into files, so they are part of the format.
    assert_eq!(FolderType::User as u8, 0);
    assert_eq!(FolderType::Relations as u8, 5);
    assert_eq!(FolderType::Diagrams as u8, 7);
    assert_eq!(FolderType::ImplementationMigration as u8, 9);
    assert_eq!(FolderType::Diagrams.as_str(), "diagrams");
    assert_eq!(
        FolderType::from_str("implementation_migration"),
        Some(FolderType::ImplementationMigration)
    );
}

#[test]
fn every_element_indexes_its_own_row_in_the_matrix() {
    for e in ElementType::ALL {
        let info = e.info();
        assert_eq!(
            matrix::CONCEPT_NAMES[info.matrix_idx as usize],
            info.short,
            "{} points at the wrong matrix row",
            info.short
        );
        assert_eq!(info.xsi, format!("archimate:{}", info.short));
    }
    assert_eq!(matrix::CONCEPT_NAMES[matrix::RELATIONSHIP_PSEUDO_IDX as usize], "Relationship");
}

#[test]
fn type_names_round_trip() {
    for e in ElementType::ALL {
        assert_eq!(ElementType::from_str(e.info().short), Some(e));
        assert_eq!(ElementType::from_str(e.info().xsi), Some(e));
    }
    for r in RelType::ALL {
        assert_eq!(RelType::from_str(r.info().short), Some(r));
        assert_eq!(RelType::from_str(r.info().xsi), Some(r));
        // The bare and suffixed spellings must both resolve; agents write either.
        assert_eq!(RelType::from_str(&format!("{}Relationship", r.info().short)), Some(r));
    }
    assert_eq!(ElementType::from_str("NotAThing"), None);
    assert_eq!(RelType::from_str("NotAThing"), None);
}

/// Read directly out of `assets/archi/relationships.xml`.
#[test]
fn the_matrix_says_what_archimate_says() {
    use ElementType::*;
    use RelType::*;

    // ApplicationComponent -> BusinessProcess = "fortv"
    for r in [Flow, Association, Realization, Triggering, Serving] {
        assert!(allows(ApplicationComponent, BusinessProcess, r), "{r:?} should be allowed");
    }
    for r in [Composition, Aggregation, Assignment, Access, Influence, Specialization] {
        assert!(!allows(ApplicationComponent, BusinessProcess, r), "{r:?} should be forbidden");
    }

    // DataObject -> ApplicationComponent = "o": association and nothing else.
    // This is the shape of violation the old Python tool let through.
    assert!(allows(DataObject, ApplicationComponent, Association));
    assert!(!allows(DataObject, ApplicationComponent, Serving));
    assert!(!allows(DataObject, ApplicationComponent, Access));

    // ApplicationFunction -> DataObject = "ao"
    assert!(allows(ApplicationFunction, DataObject, Access));
    assert!(allows(ApplicationFunction, DataObject, Association));
    assert!(!allows(ApplicationFunction, DataObject, Realization));

    // DataObject -> DataObject = "cgos". Composition between data objects IS
    // permitted by the standard, whatever local modelling guidelines may say.
    assert!(allows(DataObject, DataObject, Composition));
    assert!(allows(DataObject, DataObject, Aggregation));
    assert!(!allows(DataObject, DataObject, Access));

    assert!(allows(BusinessActor, BusinessRole, Assignment));
    assert!(allows(Node, SystemSoftware, Assignment));
}

#[test]
fn permitted_lists_exactly_the_allowed_relationships() {
    use ElementType::*;
    use RelType::*;
    assert_eq!(
        matrix::permitted(idx(ApplicationFunction), idx(DataObject)),
        vec![Access, Association]
    );
    assert_eq!(matrix::permitted(idx(DataObject), idx(ApplicationComponent)), vec![Association]);
}

#[test]
fn a_junction_accepts_every_relationship_type() {
    // The type-level check is permissive; the constraint that all relationships
    // at one junction share a type is a rule on top of the table, not in it.
    for r in RelType::ALL {
        assert!(allows(ElementType::Junction, ElementType::Junction, r), "{r:?}");
    }
}

#[test]
fn out_of_range_indices_are_refused_not_panicked_on() {
    assert!(!matrix::allows(200, 0, RelType::Association));
    assert!(!matrix::allows(0, 200, RelType::Association));
}

#[test]
fn default_figure_sizes_match_archi() {
    assert_eq!(ElementType::ApplicationComponent.info().default_wh, (120, 55));
    assert_eq!(ElementType::BusinessActor.info().default_wh, (120, 55));
    assert_eq!(ElementType::Junction.info().default_wh, (15, 15));
}

#[test]
fn viewpoints_are_complete_and_an_empty_list_means_everything() {
    assert_eq!(viewpoints::VIEWPOINTS.len(), 25);

    let layered = viewpoints::by_id("layered").expect("the layered viewpoint exists");
    assert!(layered.elements.is_empty(), "layered restricts nothing");
    assert!(viewpoints::allows("layered", ElementType::ApplicationComponent));

    let org = viewpoints::by_id("organization").expect("the organization viewpoint exists");
    assert!(org.elements.contains(&ElementType::BusinessActor));
    assert!(!org.elements.contains(&ElementType::ApplicationComponent));
    assert!(viewpoints::allows("organization", ElementType::BusinessActor));
    assert!(!viewpoints::allows("organization", ElementType::ApplicationComponent));

    // A macro must have expanded to a whole layer somewhere.
    let coop = viewpoints::by_id("business_process_cooperation").unwrap();
    assert!(coop.elements.contains(&ElementType::ApplicationComponent), "$ApplicationElements$");
    assert!(coop.elements.contains(&ElementType::ApplicationService));

    // An unknown viewpoint permits everything rather than rejecting the model.
    assert!(viewpoints::allows("not_a_viewpoint", ElementType::DataObject));
}

#[test]
fn the_five_serialisation_name_overrides_are_present() {
    let m = |k: &str| types::XSI_NAME_OVERRIDES.iter().find(|(f, _)| *f == k).map(|(_, t)| *t);
    assert_eq!(m("DiagramModelArchimateObject"), Some("DiagramObject"));
    assert_eq!(m("DiagramModelArchimateConnection"), Some("Connection"));
    assert_eq!(m("DiagramModelGroup"), Some("Group"));
    assert_eq!(m("DiagramModelNote"), Some("Note"));
    assert_eq!(m("ArchimateModel"), Some("model"));
    assert_eq!(types::XSI_NAME_OVERRIDES.len(), 5);

    let f = |k: &str| types::FEATURE_NAME_OVERRIDES.iter().find(|(a, _)| *a == k).map(|(_, b)| *b);
    assert_eq!(f("sourceConnections"), Some("sourceConnection"));
    assert_eq!(f("children"), Some("child"));
}
