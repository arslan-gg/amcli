//! The one hand-maintained table in the project: which layer each ArchiMate
//! concept belongs to, and therefore which folder Archi files it under.
//!
//! The concept *names* are not invented here — they come from the vendored
//! `relationships.xml`, and codegen fails if this table and that file disagree.
//! What lives here is the part `relationships.xml` does not encode: layer
//! membership.
//!
//! Folder assignment mirrors `ArchimateModel.getDefaultFolderForObject`
//! (Archi 5.9.0), including its two non-obvious cases: Technology and Physical
//! share the `technology` folder, and Junction, Location and Grouping all land
//! in `other`.

/// `FolderType.java` literal values. The numbers are written into files, so they
/// are part of the format, not an internal detail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FolderType {
    User = 0,
    Strategy = 1,
    Business = 2,
    Application = 3,
    Technology = 4,
    Relations = 5,
    Other = 6,
    Diagrams = 7,
    Motivation = 8,
    ImplementationMigration = 9,
}

impl FolderType {
    pub fn ident(self) -> &'static str {
        match self {
            FolderType::User => "User",
            FolderType::Strategy => "Strategy",
            FolderType::Business => "Business",
            FolderType::Application => "Application",
            FolderType::Technology => "Technology",
            FolderType::Relations => "Relations",
            FolderType::Other => "Other",
            FolderType::Diagrams => "Diagrams",
            FolderType::Motivation => "Motivation",
            FolderType::ImplementationMigration => "ImplementationMigration",
        }
    }

    /// The value written as the `type` attribute.
    pub fn wire(self) -> &'static str {
        match self {
            FolderType::User => "user",
            FolderType::Strategy => "strategy",
            FolderType::Business => "business",
            FolderType::Application => "application",
            FolderType::Technology => "technology",
            FolderType::Relations => "relations",
            FolderType::Other => "other",
            FolderType::Diagrams => "diagrams",
            FolderType::Motivation => "motivation",
            FolderType::ImplementationMigration => "implementation_migration",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Strategy,
    Business,
    Application,
    Technology,
    Physical,
    Motivation,
    ImplementationMigration,
    Other,
}

impl Layer {
    pub fn ident(self) -> &'static str {
        match self {
            Layer::Strategy => "Strategy",
            Layer::Business => "Business",
            Layer::Application => "Application",
            Layer::Technology => "Technology",
            Layer::Physical => "Physical",
            Layer::Motivation => "Motivation",
            Layer::ImplementationMigration => "ImplementationMigration",
            Layer::Other => "Other",
        }
    }

    pub fn wire(self) -> &'static str {
        match self {
            Layer::Strategy => "Strategy",
            Layer::Business => "Business",
            Layer::Application => "Application",
            Layer::Technology => "Technology",
            Layer::Physical => "Physical",
            Layer::Motivation => "Motivation",
            Layer::ImplementationMigration => "Implementation & Migration",
            Layer::Other => "Other",
        }
    }

    /// Per `getDefaultFolderForObject`: Physical shares Technology's folder, and
    /// the Other layer covers Junction, Location and Grouping.
    pub fn folder(self) -> FolderType {
        match self {
            Layer::Strategy => FolderType::Strategy,
            Layer::Business => FolderType::Business,
            Layer::Application => FolderType::Application,
            Layer::Technology | Layer::Physical => FolderType::Technology,
            Layer::Motivation => FolderType::Motivation,
            Layer::ImplementationMigration => FolderType::ImplementationMigration,
            Layer::Other => FolderType::Other,
        }
    }
}

/// All 61 ArchiMate 3.2 element types, grouped by layer.
pub const ELEMENTS: &[(Layer, &[&str])] = &[
    (Layer::Strategy, &["Resource", "Capability", "CourseOfAction", "ValueStream"]),
    (
        Layer::Business,
        &[
            "BusinessActor",
            "BusinessRole",
            "BusinessCollaboration",
            "BusinessInterface",
            "BusinessProcess",
            "BusinessFunction",
            "BusinessInteraction",
            "BusinessEvent",
            "BusinessService",
            "BusinessObject",
            "Contract",
            "Representation",
            "Product",
        ],
    ),
    (
        Layer::Application,
        &[
            "ApplicationComponent",
            "ApplicationCollaboration",
            "ApplicationInterface",
            "ApplicationFunction",
            "ApplicationInteraction",
            "ApplicationProcess",
            "ApplicationEvent",
            "ApplicationService",
            "DataObject",
        ],
    ),
    (
        Layer::Technology,
        &[
            "Node",
            "Device",
            "SystemSoftware",
            "TechnologyCollaboration",
            "TechnologyInterface",
            "Path",
            "CommunicationNetwork",
            "TechnologyFunction",
            "TechnologyProcess",
            "TechnologyInteraction",
            "TechnologyEvent",
            "TechnologyService",
            "Artifact",
        ],
    ),
    (Layer::Physical, &["Equipment", "Facility", "DistributionNetwork", "Material"]),
    (
        Layer::Motivation,
        &[
            "Stakeholder",
            "Driver",
            "Assessment",
            "Goal",
            "Outcome",
            "Principle",
            "Requirement",
            "Constraint",
            "Meaning",
            "Value",
        ],
    ),
    (
        Layer::ImplementationMigration,
        &["WorkPackage", "Deliverable", "ImplementationEvent", "Plateau", "Gap"],
    ),
    (Layer::Other, &["Location", "Grouping", "Junction"]),
];

/// Default figure size when `<bounds>` carries `width="-1" height="-1"`.
///
/// Verified against Archi 5.9.0: `IPreferenceConstants.DEFAULT_ARCHIMATE_FIGURE_
/// {WIDTH,HEIGHT}` is 120x55, and `getDefaultSizeForFigureType` no longer varies
/// by figure type — every override delegates to super. Junction is the one
/// element that overrides `getDefaultSize`.
pub fn default_size(name: &str) -> (i32, i32) {
    match name {
        "Junction" => (15, 15),
        _ => (120, 55),
    }
}

/// The five classes EMF serialises under a different name, plus the root. Getting
/// any of these wrong produces a file Archi silently mis-reads, so they live in
/// exactly one place.
pub const XSI_NAME_OVERRIDES: &[(&str, &str)] = &[
    ("ArchimateModel", "model"),
    ("DiagramModelGroup", "Group"),
    ("DiagramModelNote", "Note"),
    ("DiagramModelArchimateObject", "DiagramObject"),
    ("DiagramModelArchimateConnection", "Connection"),
];

/// EMF feature renames: the Java field name on the left, the XML element or
/// attribute name on the right.
pub const FEATURE_NAME_OVERRIDES: &[(&str, &str)] = &[
    ("folders", "folder"),
    ("elements", "element"),
    ("properties", "property"),
    ("features", "feature"),
    ("entries", "entry"),
    ("profiles", "profile"),
    ("children", "child"),
    ("sourceConnections", "sourceConnection"),
    ("bendpoints", "bendpoint"),
    ("referencedModel", "model"),
];
