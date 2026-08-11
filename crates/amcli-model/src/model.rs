//! The ArchiMate model as a set of indices over a format-preserving document.
//!
//! Nothing here owns the file's contents. Every concept, folder and view keeps a
//! `NodeId` pointing back into the [`Doc`], which is why unknown attributes and
//! unknown element types cost nothing to support: they are never copied, so they
//! cannot be lost.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use amcli_xml::{Doc, NodeId};
use sha2::{Digest, Sha256};

use crate::container::Container;
use crate::generated::{ElementType, FolderType, Layer, RelType};
use crate::{ModelError, container};

/// Elements and relationships share a namespace, because in the file they share
/// a tag: both are `<element xsi:type="archimate:X">`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ConceptId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FolderId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ViewId(pub u32);

/// What a `<element>` turned out to be.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConceptKind {
    Element(ElementType),
    Relationship(RelType),
    /// A type this build does not know — a newer ArchiMate, an Archi extension,
    /// or a typo. It is indexed, searchable and written back untouched rather
    /// than dropped, so an unfamiliar model is still safe to edit.
    Unknown {
        xsi: String,
        is_relationship: bool,
    },
}

impl ConceptKind {
    pub fn is_relationship(&self) -> bool {
        match self {
            ConceptKind::Element(_) => false,
            ConceptKind::Relationship(_) => true,
            ConceptKind::Unknown { is_relationship, .. } => *is_relationship,
        }
    }

    /// Bare type name, e.g. `ApplicationComponent` or `AccessRelationship`.
    pub fn name(&self) -> &str {
        match self {
            ConceptKind::Element(e) => e.info().short,
            ConceptKind::Relationship(r) => r.info().xsi.trim_start_matches("archimate:"),
            ConceptKind::Unknown { xsi, .. } => xsi.trim_start_matches("archimate:"),
        }
    }

    pub fn layer(&self) -> Option<Layer> {
        match self {
            ConceptKind::Element(e) => Some(e.info().layer),
            _ => None,
        }
    }

    /// Index into the relationship matrix, when the type is one it covers.
    pub fn matrix_idx(&self) -> Option<u8> {
        match self {
            ConceptKind::Element(e) => Some(e.info().matrix_idx),
            // A relationship can be the target of an Association; the matrix
            // models that with a single pseudo-concept.
            ConceptKind::Relationship(_) => Some(crate::generated::matrix::RELATIONSHIP_PSEUDO_IDX),
            ConceptKind::Unknown { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Concept {
    /// False once deleted. The slot itself survives so that a `ConceptId`
    /// handed to a caller stays valid across later edits — renumbering on every
    /// mutation would silently repoint a handle at a different concept.
    pub alive: bool,
    pub node: NodeId,
    pub id: String,
    pub name: String,
    pub kind: ConceptKind,
    pub folder: FolderId,
    /// Endpoint ids, present exactly when this is a relationship.
    pub source: Option<String>,
    pub target: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub alive: bool,
    pub node: NodeId,
    pub id: String,
    pub name: String,
    /// Absent on nested user folders, where `user` is the schema default.
    pub folder_type: FolderType,
    pub parent: Option<FolderId>,
    /// `/Business/Processes`, computed on load and never stored in the file.
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct View {
    pub alive: bool,
    pub node: NodeId,
    pub id: String,
    pub name: String,
    pub folder: FolderId,
    /// `goal_realization` and friends; empty means no viewpoint.
    pub viewpoint: String,
    /// Sketch views carry no ArchiMate semantics but must survive editing.
    pub is_sketch: bool,
}

/// What an id resolves to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Entity {
    Concept(ConceptId),
    Folder(FolderId),
    View(ViewId),
    /// A diagram object or connection inside a view.
    Visual(NodeId),
}

pub struct Model {
    pub doc: Doc,
    container: Container,
    path: PathBuf,
    concepts: Vec<Concept>,
    folders: Vec<Folder>,
    views: Vec<View>,
    by_id: HashMap<String, Entity>,
    /// Which slot each XML node already owns. This is what makes ConceptId,
    /// FolderId and ViewId stable handles: the arena never renumbers, so a
    /// re-walk after an edit updates slots in place rather than reassigning
    /// them.
    slot_of_node: HashMap<NodeId, Entity>,
    /// Ids seen more than once. A duplicate is a validation finding, not a
    /// reason to refuse to open the file — you cannot fix what you cannot load.
    duplicate_ids: Vec<String>,
}

impl Model {
    pub fn open(path: impl AsRef<Path>) -> Result<Model, ModelError> {
        let path = path.as_ref();
        let (container, xml) = Container::open(path)?;
        Model::from_parts(container, xml, path.to_path_buf())
    }

    pub fn from_bytes(bytes: Vec<u8>, path: impl Into<PathBuf>) -> Result<Model, ModelError> {
        let path = path.into();
        let (container, xml) = Container::from_bytes(bytes, &path)?;
        Model::from_parts(container, xml, path)
    }

    fn from_parts(container: Container, xml: Vec<u8>, path: PathBuf) -> Result<Model, ModelError> {
        let doc =
            Doc::parse(xml).map_err(|source| ModelError::Xml { path: path.clone(), source })?;
        let root = doc.root();
        if doc.local_name(root) != "model" {
            return Err(ModelError::NotAModel { path, root: doc.name(root).to_string() });
        }

        let mut m = Model {
            doc,
            container,
            path,
            concepts: Vec::new(),
            folders: Vec::new(),
            views: Vec::new(),
            by_id: HashMap::new(),
            slot_of_node: HashMap::new(),
            duplicate_ids: Vec::new(),
        };
        m.index();
        Ok(m)
    }

    /// Rebuild every index from the document.
    ///
    /// Called after each structural edit. Doing it wholesale rather than
    /// incrementally is O(n) per edit, which on a 3,000-concept model is
    /// microseconds — and it makes an index that disagrees with the document
    /// structurally impossible, which is worth far more than the cycles.
    pub(crate) fn reindex(&mut self) {
        for c in &mut self.concepts {
            c.alive = false;
        }
        for f in &mut self.folders {
            f.alive = false;
        }
        for v in &mut self.views {
            v.alive = false;
        }
        self.by_id.clear();
        self.duplicate_ids.clear();
        self.index();
    }

    /// Look up a folder by its id attribute.
    pub fn folder_id_by_id(&self, id: &str) -> Option<FolderId> {
        match self.by_id.get(id) {
            Some(Entity::Folder(f)) => Some(*f),
            _ => None,
        }
    }

    /// Walk the folder tree once, classifying everything as it goes.
    fn index(&mut self) {
        let root = self.doc.root();
        let top: Vec<NodeId> =
            self.doc.children(root).filter(|c| self.doc.local_name(*c) == "folder").collect();
        for node in top {
            self.index_folder(node, None, "");
        }
    }

    fn index_folder(&mut self, node: NodeId, parent: Option<FolderId>, parent_path: &str) {
        let id = self.doc.attr(node, "id").unwrap_or_default();
        let name = self.doc.attr(node, "name").unwrap_or_default();
        // Only top-level folders carry `type`; EMF omits the `user` default.
        let folder_type = self
            .doc
            .attr(node, "type")
            .and_then(|t| FolderType::from_str(&t))
            .or_else(|| parent.map(|p| self.folders[p.0 as usize].folder_type))
            .unwrap_or(FolderType::User);

        let path = format!("{parent_path}/{name}");
        let folder = Folder {
            alive: true,
            node,
            id: id.clone(),
            name,
            folder_type,
            parent,
            path: path.clone(),
        };
        let fid = match self.slot_of_node.get(&node) {
            Some(Entity::Folder(f)) => {
                let f = *f;
                self.folders[f.0 as usize] = folder;
                f
            }
            _ => {
                let f = FolderId(self.folders.len() as u32);
                self.folders.push(folder);
                self.slot_of_node.insert(node, Entity::Folder(f));
                f
            }
        };
        self.register(id, Entity::Folder(fid));

        let children: Vec<NodeId> = self.doc.children(node).collect();
        for c in children {
            match self.doc.local_name(c) {
                "folder" => self.index_folder(c, Some(fid), &path),
                "element" => self.index_element(c, fid),
                _ => {}
            }
        }
    }

    fn index_element(&mut self, node: NodeId, folder: FolderId) {
        let xsi = self.doc.attr(node, "xsi:type").unwrap_or_default();
        let bare = xsi.trim_start_matches("archimate:").to_string();
        let id = self.doc.attr(node, "id").unwrap_or_default();
        let name = self.doc.attr(node, "name").unwrap_or_default();

        // Views live in the same `<element>` slot as concepts, told apart only
        // by their type.
        if bare == "ArchimateDiagramModel" || bare == "SketchModel" {
            let view = View {
                alive: true,
                node,
                id: id.clone(),
                name,
                folder,
                viewpoint: self.doc.attr(node, "viewpoint").unwrap_or_default(),
                is_sketch: bare == "SketchModel",
            };
            let vid = match self.slot_of_node.get(&node) {
                Some(Entity::View(v)) => {
                    let v = *v;
                    self.views[v.0 as usize] = view;
                    v
                }
                _ => {
                    let v = ViewId(self.views.len() as u32);
                    self.views.push(view);
                    self.slot_of_node.insert(node, Entity::View(v));
                    v
                }
            };
            self.register(id, Entity::View(vid));
            self.index_visuals(node);
            return;
        }

        let source = self.doc.attr(node, "source");
        let target = self.doc.attr(node, "target");
        let kind = match (ElementType::from_str(&bare), RelType::from_str(&bare)) {
            (Some(e), _) => ConceptKind::Element(e),
            (None, Some(r)) => ConceptKind::Relationship(r),
            // For a type we do not recognise, having both endpoints is what
            // makes something an edge — far more reliable than guessing from
            // the name or trusting which folder it happens to sit in.
            (None, None) => ConceptKind::Unknown {
                xsi: bare,
                is_relationship: source.is_some() && target.is_some(),
            },
        };

        let concept =
            Concept { alive: true, node, id: id.clone(), name, kind, folder, source, target };
        let cid = match self.slot_of_node.get(&node) {
            Some(Entity::Concept(c)) => {
                let c = *c;
                self.concepts[c.0 as usize] = concept;
                c
            }
            _ => {
                let c = ConceptId(self.concepts.len() as u32);
                self.concepts.push(concept);
                self.slot_of_node.insert(node, Entity::Concept(c));
                c
            }
        };
        self.register(id, Entity::Concept(cid));
    }

    /// Diagram objects and connections are addressable by id too, which is how
    /// a dangling `archimateElement` gets reported against a real location.
    fn index_visuals(&mut self, view_node: NodeId) {
        for n in self.doc.descendants(view_node) {
            if n == view_node {
                continue;
            }
            if matches!(self.doc.local_name(n), "child" | "sourceConnection")
                && let Some(id) = self.doc.attr(n, "id")
            {
                self.register(id, Entity::Visual(n));
            }
        }
    }

    fn register(&mut self, id: String, e: Entity) {
        if id.is_empty() {
            return;
        }
        if self.by_id.contains_key(&id) {
            self.duplicate_ids.push(id);
            return;
        }
        self.by_id.insert(id, e);
    }

    // ---- accessors --------------------------------------------------------

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_zipped(&self) -> bool {
        self.container.is_zip()
    }

    pub fn name(&self) -> String {
        self.doc.attr(self.doc.root(), "name").unwrap_or_default()
    }

    pub fn model_id(&self) -> String {
        self.doc.attr(self.doc.root(), "id").unwrap_or_default()
    }

    /// The `version` attribute, which is the *model* version (5.0.0 means
    /// ArchiMate 3.2), not the Archi version.
    pub fn version(&self) -> String {
        self.doc.attr(self.doc.root(), "version").unwrap_or_default()
    }

    pub fn purpose(&self) -> Option<String> {
        self.doc.child_named(self.doc.root(), "purpose").map(|n| self.doc.text(n))
    }

    /// Live concepts. Deleted slots are skipped, so this can never hand back a
    /// tombstone by accident.
    pub fn concepts(&self) -> impl Iterator<Item = &Concept> + '_ {
        self.concepts.iter().filter(|c| c.alive)
    }

    /// Live concepts with their stable handles.
    pub fn concepts_with_ids(&self) -> impl Iterator<Item = (ConceptId, &Concept)> + '_ {
        self.concepts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.alive)
            .map(|(i, c)| (ConceptId(i as u32), c))
    }

    /// Number of slots, including tombstones. Use it to size arrays indexed by
    /// `ConceptId`, never as a concept count.
    pub fn concept_slots(&self) -> usize {
        self.concepts.len()
    }

    pub fn folders(&self) -> impl Iterator<Item = &Folder> + '_ {
        self.folders.iter().filter(|f| f.alive)
    }

    pub fn folders_with_ids(&self) -> impl Iterator<Item = (FolderId, &Folder)> + '_ {
        self.folders
            .iter()
            .enumerate()
            .filter(|(_, f)| f.alive)
            .map(|(i, f)| (FolderId(i as u32), f))
    }

    pub fn views(&self) -> impl Iterator<Item = &View> + '_ {
        self.views.iter().filter(|v| v.alive)
    }

    pub fn views_with_ids(&self) -> impl Iterator<Item = (ViewId, &View)> + '_ {
        self.views.iter().enumerate().filter(|(_, v)| v.alive).map(|(i, v)| (ViewId(i as u32), v))
    }

    pub fn duplicate_ids(&self) -> &[String] {
        &self.duplicate_ids
    }

    pub fn concept(&self, id: ConceptId) -> &Concept {
        &self.concepts[id.0 as usize]
    }

    pub fn folder(&self, id: FolderId) -> &Folder {
        &self.folders[id.0 as usize]
    }

    pub fn view(&self, id: ViewId) -> &View {
        &self.views[id.0 as usize]
    }

    pub fn entity(&self, id: &str) -> Option<Entity> {
        self.by_id.get(id).copied()
    }

    pub fn concept_by_id(&self, id: &str) -> Option<ConceptId> {
        match self.by_id.get(id) {
            Some(Entity::Concept(c)) => Some(*c),
            _ => None,
        }
    }

    pub fn view_by_id(&self, id: &str) -> Option<ViewId> {
        match self.by_id.get(id) {
            Some(Entity::View(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn folder_by_path(&self, path: &str) -> Option<FolderId> {
        let want = path.trim_end_matches('/');
        self.folders_with_ids().find(|(_, f)| f.path.eq_ignore_ascii_case(want)).map(|(i, _)| i)
    }

    /// The top-level folder of a given type, which is where new concepts go.
    pub fn top_folder(&self, t: FolderType) -> Option<FolderId> {
        self.folders_with_ids()
            .find(|(_, f)| f.parent.is_none() && f.folder_type == t)
            .map(|(i, _)| i)
    }

    // ---- per-concept detail, read straight from the document --------------

    pub fn documentation(&self, node: NodeId) -> Option<String> {
        self.doc.child_named(node, "documentation").map(|n| self.doc.text(n))
    }

    pub fn properties(&self, node: NodeId) -> Vec<(String, String)> {
        self.doc
            .children(node)
            .filter(|c| self.doc.local_name(*c) == "property")
            .map(|c| {
                (
                    self.doc.attr(c, "key").unwrap_or_default(),
                    self.doc.attr(c, "value").unwrap_or_default(),
                )
            })
            .collect()
    }

    /// Archi 4.6+ stores extended styling here; losing it loses half a diagram's
    /// appearance, so it is surfaced rather than skipped.
    pub fn features(&self, node: NodeId) -> Vec<(String, String)> {
        self.doc
            .children(node)
            .filter(|c| self.doc.local_name(*c) == "feature")
            .map(|c| {
                (
                    self.doc.attr(c, "name").unwrap_or_default(),
                    self.doc.attr(c, "value").unwrap_or_default(),
                )
            })
            .collect()
    }

    pub fn folder_path_of(&self, c: &Concept) -> &str {
        &self.folders[c.folder.0 as usize].path
    }

    // ---- writing ----------------------------------------------------------

    /// Serialise, re-wrapping into a ZIP if that is what we opened.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ModelError> {
        if self.doc.is_unmodified()
            && let Some(original) = self.container.original()
        {
            return Ok(original.to_vec());
        }
        self.container.wrap(self.doc.to_bytes())
    }

    /// A content hash of the file as it would be written. Read commands report
    /// it and writes can require it, which is what protects an agent that reads
    /// on one turn and writes three turns later.
    pub fn checksum(&self) -> Result<String, ModelError> {
        Ok(format!("{:x}", Sha256::digest(&self.to_bytes()?)))
    }

    pub fn save(&self) -> Result<(), ModelError> {
        container::write_atomically(&self.path, &self.to_bytes()?)
    }

    pub fn save_as(&self, path: &Path) -> Result<(), ModelError> {
        container::write_atomically(path, &self.to_bytes()?)
    }

    /// True when nothing has been modified since the file was read.
    pub fn is_unmodified(&self) -> bool {
        self.doc.is_unmodified()
    }
}
