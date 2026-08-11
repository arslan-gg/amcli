//! Mutations.
//!
//! Everything that changes a model goes through here, and everything here is
//! in-memory. Persisting is a separate, explicit step, which is what lets a
//! batch of edits either all land or leave the file byte-identical.
//!
//! Attribute order for newly created nodes matches what Archi writes — verified
//! against its own test fixtures. Getting it wrong would not corrupt anything,
//! but it would make every diff noisier than it needs to be, and a noisy diff is
//! how a review stops catching real changes.

use amcli_xml::{NodeBuilder, NodeId};

use crate::model::{Concept, ConceptId, ConceptKind, FolderId, ViewId};
use crate::{ElementType, FolderType, Model, ModelError, RelType, ids, matrix};

/// Why an edit was refused.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("no concept with id `{0}`")]
    NoSuchConcept(String),
    #[error("no folder at `{0}`")]
    NoSuchFolder(String),
    #[error(
        "ArchiMate does not permit {rel} from {source_type} to {target_type}{}",
        permitted_hint(.permitted)
    )]
    InvalidRelationship {
        rel: &'static str,
        source_type: String,
        target_type: String,
        permitted: Vec<&'static str>,
    },
    // Note: not named `source`, which thiserror reserves for the error cause.
    #[error("a {rel} relationship from `{from}` to `{to}` already exists (id `{existing}`)")]
    DuplicateRelationship { rel: &'static str, from: String, to: String, existing: String },
    #[error("every relationship at a junction must be the same type; `{0}` already has {1}")]
    MixedJunction(String, &'static str),
    #[error("accessType must be 0 (write), 1 (read), 2 (unspecified) or 3 (read/write), not {0}")]
    BadAccessType(i64),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Xml(#[from] amcli_xml::MixedContent),
}

fn permitted_hint(p: &[&'static str]) -> String {
    if p.is_empty() {
        " — no relationship type is permitted between these two".to_string()
    } else {
        format!(" — permitted here: {}", p.join(", "))
    }
}

/// What a delete would take with it. Returned before anything is touched so a
/// caller can look before it leaps, and returned again afterwards as a record.
#[derive(Clone, Debug, Default)]
pub struct Cascade {
    /// Concepts removed, the requested one first.
    pub concepts: Vec<String>,
    /// Relationships removed because an endpoint went.
    pub relationships: Vec<String>,
    /// Diagram objects removed because the concept they showed went.
    pub diagram_objects: Vec<String>,
    /// Connections removed because their relationship or an endpoint went.
    pub connections: Vec<String>,
    /// Views whose contents changed.
    pub views: Vec<String>,
    /// Junctions left with fewer than two connections. Flagged, never
    /// auto-deleted: a junction is a modelling decision, not debris.
    pub degenerate_junctions: Vec<String>,
}

impl Cascade {
    pub fn is_empty(&self) -> bool {
        self.relationships.is_empty()
            && self.diagram_objects.is_empty()
            && self.connections.is_empty()
    }

    pub fn total(&self) -> usize {
        self.concepts.len()
            + self.relationships.len()
            + self.diagram_objects.len()
            + self.connections.len()
    }
}

impl Model {
    // ---- creating -------------------------------------------------------

    /// Add an element. With no folder given it lands in the one Archi would
    /// have chosen for its type, which keeps the folder taxonomy from rotting
    /// the way it does when everything is dumped in one place.
    pub fn add_element(
        &mut self,
        ty: ElementType,
        name: &str,
        folder: Option<FolderId>,
        documentation: Option<&str>,
    ) -> Result<ConceptId, EditError> {
        let folder = match folder {
            Some(f) => f,
            None => self
                .top_folder(ty.info().home)
                .ok_or_else(|| EditError::NoSuchFolder(ty.info().home.as_str().to_string()))?,
        };
        let id = ids::new_id();
        let folder_node = self.folder(folder).node;
        // Attribute order matches what Archi writes: xsi:type, name, id. Not a
        // correctness issue, but the wrong order makes every diff noisier, and
        // a noisy diff is how review stops catching real changes.
        let node = self.doc.append_child(
            folder_node,
            NodeBuilder::new("element")
                .attr("xsi:type", ty.info().xsi)
                .attr("name", name)
                .attr("id", &*id),
        )?;
        if let Some(doc) = documentation.filter(|d| !d.is_empty()) {
            self.set_documentation_node(node, doc)?;
        }
        self.reindex();
        Ok(self.concept_by_id(&id).expect("just added"))
    }

    /// Add a relationship, refusing anything ArchiMate does not permit.
    ///
    /// Three checks, matching what Archi enforces: the matrix, no duplicate
    /// direct relationship of the same type between the same ordered pair, and
    /// the rule that every relationship touching a junction shares its type.
    pub fn add_relation(
        &mut self,
        ty: RelType,
        source: ConceptId,
        target: ConceptId,
        access_type: Option<i64>,
        documentation: Option<&str>,
    ) -> Result<ConceptId, EditError> {
        self.check_relationship(ty, source, target)?;
        if let Some(a) = access_type
            && !(0..=3).contains(&a)
        {
            return Err(EditError::BadAccessType(a));
        }

        let folder = self
            .top_folder(FolderType::Relations)
            .ok_or_else(|| EditError::NoSuchFolder("relations".to_string()))?;
        let id = ids::new_id();
        let (src_id, tgt_id) = (self.concept(source).id.clone(), self.concept(target).id.clone());

        let mut b = NodeBuilder::new("element")
            .attr("xsi:type", ty.info().xsi)
            .attr("id", &*id)
            .attr("source", &*src_id)
            .attr("target", &*tgt_id);
        // Archi omits accessType when it equals the schema default of 0
        // (write); writing it explicitly would break byte identity against a
        // file Archi produced.
        if let Some(a) = access_type.filter(|a| *a != 0) {
            b = b.attr("accessType", a.to_string());
        }

        let folder_node = self.folder(folder).node;
        let node = self.doc.append_child(folder_node, b)?;
        if let Some(doc) = documentation.filter(|d| !d.is_empty()) {
            self.set_documentation_node(node, doc)?;
        }
        self.reindex();
        Ok(self.concept_by_id(&id).expect("just added"))
    }

    /// The full legality check, without performing the edit.
    pub fn check_relationship(
        &self,
        ty: RelType,
        source: ConceptId,
        target: ConceptId,
    ) -> Result<(), EditError> {
        let (s, t) = (self.concept(source), self.concept(target));

        // An unknown type has no matrix row, so the table cannot judge it. That
        // is a reason to allow the edit and let validation report it, not to
        // block work on a model this build does not fully understand.
        if let (Some(si), Some(ti)) = (s.kind.matrix_idx(), t.kind.matrix_idx())
            && !matrix::allows(si, ti, ty)
        {
            return Err(EditError::InvalidRelationship {
                rel: ty.info().short,
                source_type: s.kind.name().to_string(),
                target_type: t.kind.name().to_string(),
                permitted: matrix::permitted(si, ti).iter().map(|r| r.info().short).collect(),
            });
        }

        if let Some(existing) = self.concepts().find(|c| {
            c.kind == ConceptKind::Relationship(ty)
                && c.source.as_deref() == Some(s.id.as_str())
                && c.target.as_deref() == Some(t.id.as_str())
        }) {
            return Err(EditError::DuplicateRelationship {
                rel: ty.info().short,
                from: display_name(s),
                to: display_name(t),
                existing: existing.id.clone(),
            });
        }

        for end in [s, t] {
            if end.kind == ConceptKind::Element(ElementType::Junction)
                && let Some(other) = self.junction_rel_type(&end.id)
                && other != ty
            {
                return Err(EditError::MixedJunction(display_name(end), other.info().short));
            }
        }
        Ok(())
    }

    /// The relationship type already in use at a junction, if any.
    fn junction_rel_type(&self, junction_id: &str) -> Option<RelType> {
        self.concepts().find_map(|c| {
            let touches = c.source.as_deref() == Some(junction_id)
                || c.target.as_deref() == Some(junction_id);
            match (&c.kind, touches) {
                (ConceptKind::Relationship(r), true) => Some(*r),
                _ => None,
            }
        })
    }

    pub fn add_folder(&mut self, parent: FolderId, name: &str) -> Result<FolderId, EditError> {
        let id = ids::new_id();
        let parent_node = self.folder(parent).node;
        // Folders come before elements in a folder's children, as Archi writes
        // them; inserting at the end would still load, but the diff would move
        // things around on the next Archi save.
        let at = self
            .doc
            .children(parent_node)
            .take_while(|c| self.doc.local_name(*c) == "folder")
            .count();
        self.doc.insert_child(
            parent_node,
            at,
            NodeBuilder::new("folder").attr("name", name).attr("id", &*id),
        )?;
        self.reindex();
        Ok(self.folder_id_by_id(&id).expect("just added"))
    }

    // ---- changing -------------------------------------------------------

    pub fn rename(&mut self, c: ConceptId, name: &str) {
        let node = self.concept(c).node;
        self.doc.set_attr(node, "name", name);
        self.reindex();
    }

    pub fn set_documentation(&mut self, c: ConceptId, text: &str) -> Result<(), EditError> {
        let node = self.concept(c).node;
        self.set_documentation_node(node, text)
    }

    fn set_documentation_node(&mut self, node: NodeId, text: &str) -> Result<(), EditError> {
        match self.doc.child_named(node, "documentation") {
            Some(d) if text.is_empty() => self.doc.remove_subtree(d),
            Some(d) => self.doc.set_text(d, text)?,
            None if text.is_empty() => {}
            None => {
                // Documentation is the first child, ahead of properties, which
                // is the order Archi writes.
                self.doc.insert_child(node, 0, NodeBuilder::new("documentation").text(text))?;
            }
        }
        Ok(())
    }

    pub fn set_property(&mut self, c: ConceptId, key: &str, value: &str) -> Result<(), EditError> {
        let node = self.concept(c).node;
        let existing = self
            .doc
            .children(node)
            .filter(|n| self.doc.local_name(*n) == "property")
            .find(|n| self.doc.attr(*n, "key").as_deref() == Some(key));
        match existing {
            Some(p) => self.doc.set_attr(p, "value", value),
            None => {
                let at = self.doc.children(node).count();
                self.doc.insert_child(
                    node,
                    at,
                    NodeBuilder::new("property").attr("key", key).attr("value", value),
                )?;
            }
        }
        Ok(())
    }

    pub fn remove_property(&mut self, c: ConceptId, key: &str) {
        let node = self.concept(c).node;
        let found: Vec<NodeId> = self
            .doc
            .children(node)
            .filter(|n| {
                self.doc.local_name(*n) == "property"
                    && self.doc.attr(*n, "key").as_deref() == Some(key)
            })
            .collect();
        for p in found {
            self.doc.remove_subtree(p);
        }
        self.reindex();
    }

    /// Re-file a concept. The node itself moves, keeping its own bytes, so
    /// unknown attributes and unknown children survive the trip — rebuilding it
    /// from the fields we understand would quietly drop them.
    pub fn move_to_folder(&mut self, c: ConceptId, folder: FolderId) -> Result<(), EditError> {
        let node = self.concept(c).node;
        let target = self.folder(folder).node;
        if self.doc.parent(node) == Some(target) {
            return Ok(());
        }
        let at = self.doc.children(target).count();
        self.doc.move_child(node, target, at);
        self.reindex();
        Ok(())
    }

    // ---- deleting -------------------------------------------------------

    /// Everything a delete would remove, computed without changing anything.
    ///
    /// The visual half is what the old Python tool skipped, and skipping it is
    /// what left models Archi refused to open: a diagram object whose
    /// `archimateElement` no longer resolves is a load error, not a cosmetic
    /// problem.
    pub fn delete_plan(&self, c: ConceptId) -> Cascade {
        let mut plan = Cascade::default();
        let root = self.concept(c);
        plan.concepts.push(root.id.clone());

        // Relationships fall transitively: a relationship may itself be the
        // endpoint of an association.
        let mut doomed_concepts: Vec<String> = vec![root.id.clone()];
        let mut i = 0;
        while i < doomed_concepts.len() {
            let victim = doomed_concepts[i].clone();
            i += 1;
            for rel in self.concepts().filter(|r| r.kind.is_relationship()) {
                if plan.relationships.contains(&rel.id) {
                    continue;
                }
                if rel.source.as_deref() == Some(victim.as_str())
                    || rel.target.as_deref() == Some(victim.as_str())
                {
                    plan.relationships.push(rel.id.clone());
                    doomed_concepts.push(rel.id.clone());
                }
            }
        }

        let gone: std::collections::HashSet<&str> =
            doomed_concepts.iter().map(String::as_str).collect();

        // Now the visuals, view by view, iterating until nothing new falls: a
        // removed diagram object takes its connections, which may in turn be
        // the last thing holding another object's connection.
        for view in self.views() {
            let mut touched = false;
            let mut dead_visuals: std::collections::HashSet<String> = Default::default();

            for n in self.doc.descendants(view.node) {
                let local = self.doc.local_name(n);
                let id = self.doc.attr(n, "id").unwrap_or_default();
                let refers =
                    |attr: &str| self.doc.attr(n, attr).is_some_and(|v| gone.contains(v.as_str()));
                let dies = match local {
                    "child" => refers("archimateElement"),
                    "sourceConnection" => refers("archimateRelationship"),
                    _ => false,
                };
                if dies {
                    dead_visuals.insert(id);
                }
            }

            // Connections whose endpoint object is going, and anything nested
            // inside a doomed object.
            loop {
                let before = dead_visuals.len();
                for n in self.doc.descendants(view.node) {
                    let id = self.doc.attr(n, "id").unwrap_or_default();
                    if id.is_empty() || dead_visuals.contains(&id) {
                        continue;
                    }
                    let local = self.doc.local_name(n);
                    let doomed_parent = self
                        .doc
                        .parent(n)
                        .and_then(|p| self.doc.attr(p, "id"))
                        .is_some_and(|p| dead_visuals.contains(&p));
                    let endpoint_gone = local == "sourceConnection"
                        && ["source", "target"].iter().any(|a| {
                            self.doc.attr(n, a).is_some_and(|v| dead_visuals.contains(&v))
                        });
                    if doomed_parent || endpoint_gone {
                        dead_visuals.insert(id);
                    }
                }
                if dead_visuals.len() == before {
                    break;
                }
            }

            for n in self.doc.descendants(view.node) {
                let Some(id) = self.doc.attr(n, "id") else { continue };
                if !dead_visuals.contains(&id) {
                    continue;
                }
                touched = true;
                match self.doc.local_name(n) {
                    "child" => plan.diagram_objects.push(id),
                    "sourceConnection" => plan.connections.push(id),
                    _ => {}
                }
            }
            if touched {
                plan.views.push(view.id.clone());
            }
        }

        // A junction left with fewer than two connections no longer joins
        // anything, but removing it is a modelling decision.
        for concept in self.concepts() {
            if concept.kind != ConceptKind::Element(ElementType::Junction)
                || gone.contains(concept.id.as_str())
            {
                continue;
            }
            let left = self
                .concepts()
                .filter(|r| {
                    r.kind.is_relationship()
                        && !plan.relationships.contains(&r.id)
                        && (r.source.as_deref() == Some(concept.id.as_str())
                            || r.target.as_deref() == Some(concept.id.as_str()))
                })
                .count();
            if left < 2 {
                plan.degenerate_junctions.push(display_name(concept));
            }
        }

        plan
    }

    /// Delete a concept and everything the plan says goes with it.
    pub fn delete_concept(&mut self, c: ConceptId) -> Result<Cascade, EditError> {
        let plan = self.delete_plan(c);

        let mut nodes: Vec<NodeId> = Vec::new();
        for id in plan.concepts.iter().chain(plan.relationships.iter()) {
            if let Some(cid) = self.concept_by_id(id) {
                nodes.push(self.concept(cid).node);
            }
        }
        let visual_ids: std::collections::HashSet<&str> = plan
            .diagram_objects
            .iter()
            .chain(plan.connections.iter())
            .map(String::as_str)
            .collect();
        for view in self.views() {
            for n in self.doc.descendants(view.node) {
                if self.doc.attr(n, "id").is_some_and(|id| visual_ids.contains(id.as_str())) {
                    nodes.push(n);
                }
            }
        }
        for n in nodes {
            self.doc.remove_subtree(n);
        }

        // `targetConnections` is a derived mirror of the connections that point
        // at an object. Recomputing it, rather than patching it, removes an
        // entire class of corruption that Archi tolerates in memory but chokes
        // on at load.
        for view_id in &plan.views {
            if let Some(v) = self.view_by_id(view_id) {
                self.recompute_target_connections(v);
            }
        }

        self.reindex();
        Ok(plan)
    }

    /// Rebuild every `targetConnections` in a view from the connections that
    /// actually exist.
    pub fn recompute_target_connections(&mut self, view: ViewId) {
        let node = self.view(view).node;
        let mut incoming: std::collections::HashMap<String, Vec<String>> = Default::default();
        for n in self.doc.descendants(node) {
            if self.doc.local_name(n) != "sourceConnection" {
                continue;
            }
            let (Some(id), Some(target)) = (self.doc.attr(n, "id"), self.doc.attr(n, "target"))
            else {
                continue;
            };
            incoming.entry(target).or_default().push(id);
        }

        let objects: Vec<NodeId> = self
            .doc
            .descendants(node)
            .into_iter()
            .filter(|n| self.doc.local_name(*n) == "child")
            .collect();
        for obj in objects {
            let Some(id) = self.doc.attr(obj, "id") else { continue };
            match incoming.get(&id) {
                Some(list) => self.doc.set_attr(obj, "targetConnections", &list.join(" ")),
                // EMF omits an empty IDREFS attribute entirely.
                None => self.doc.remove_attr(obj, "targetConnections"),
            }
        }
    }
}

fn display_name(c: &Concept) -> String {
    if c.name.is_empty() { c.id.clone() } else { c.name.clone() }
}

// ---- views ---------------------------------------------------------------

impl Model {
    /// Create an empty view in the Views folder.
    pub fn add_view(&mut self, name: &str, viewpoint: Option<&str>) -> Result<ViewId, EditError> {
        let folder = self
            .top_folder(FolderType::Diagrams)
            .ok_or_else(|| EditError::NoSuchFolder("diagrams".to_string()))?;
        let id = ids::new_id();
        let mut b = NodeBuilder::new("element")
            .attr("xsi:type", "archimate:ArchimateDiagramModel")
            .attr("name", name)
            .attr("id", &*id);
        // An empty viewpoint means "no viewpoint", and EMF omits it.
        if let Some(v) = viewpoint.filter(|v| !v.is_empty()) {
            b = b.attr("viewpoint", v);
        }
        let folder_node = self.folder(folder).node;
        self.doc.append_child(folder_node, b)?;
        self.reindex();
        Ok(self.view_by_id(&id).expect("just added"))
    }

    /// Put a concept on a view at the given bounds, returning the new diagram
    /// object's id.
    ///
    /// A concept may legitimately appear on a view more than once, so this does
    /// not deduplicate; callers that want at-most-once should check first.
    pub fn add_view_object(
        &mut self,
        view: ViewId,
        concept: ConceptId,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Result<String, EditError> {
        let id = ids::new_id();
        let concept_id = self.concept(concept).id.clone();
        let view_node = self.view(view).node;
        let obj = self.doc.append_child(
            view_node,
            NodeBuilder::new("child")
                .attr("xsi:type", "archimate:DiagramObject")
                .attr("id", &*id)
                .attr("archimateElement", &*concept_id),
        )?;
        // `<bounds>` is a child element, not attributes on the object.
        self.doc.append_child(
            obj,
            NodeBuilder::new("bounds")
                .attr("x", x.to_string())
                .attr("y", y.to_string())
                .attr("width", w.to_string())
                .attr("height", h.to_string()),
        )?;
        self.reindex();
        Ok(id)
    }

    /// Draw a relationship between two objects already on the view.
    ///
    /// The connection is a child of its *source* object, which is how Archi
    /// stores it, and `targetConnections` on the target is recomputed rather
    /// than appended to.
    pub fn add_view_connection(
        &mut self,
        view: ViewId,
        relationship: ConceptId,
        source_object: &str,
        target_object: &str,
    ) -> Result<String, EditError> {
        let id = ids::new_id();
        let rel_id = self.concept(relationship).id.clone();
        let view_node = self.view(view).node;
        let src = self
            .doc
            .descendants(view_node)
            .into_iter()
            .find(|n| {
                self.doc.local_name(*n) == "child"
                    && self.doc.attr(*n, "id").as_deref() == Some(source_object)
            })
            .ok_or_else(|| EditError::NoSuchConcept(source_object.to_string()))?;

        self.doc.append_child(
            src,
            NodeBuilder::new("sourceConnection")
                .attr("xsi:type", "archimate:Connection")
                .attr("id", &*id)
                .attr("source", source_object)
                .attr("target", target_object)
                .attr("archimateRelationship", &*rel_id),
        )?;
        self.recompute_target_connections(view);
        self.reindex();
        Ok(id)
    }

    /// Move an object already on a view.
    pub fn set_view_object_bounds(
        &mut self,
        view: ViewId,
        object_id: &str,
        x: i32,
        y: i32,
    ) -> Result<(), EditError> {
        let view_node = self.view(view).node;
        let Some(obj) = self.doc.descendants(view_node).into_iter().find(|n| {
            self.doc.local_name(*n) == "child"
                && self.doc.attr(*n, "id").as_deref() == Some(object_id)
        }) else {
            return Err(EditError::NoSuchConcept(object_id.to_string()));
        };
        if let Some(b) = self.doc.child_named(obj, "bounds") {
            self.doc.set_attr(b, "x", &x.to_string());
            self.doc.set_attr(b, "y", &y.to_string());
        }
        Ok(())
    }

    /// Every diagram object on a view, as (object id, concept id).
    pub fn view_objects(&self, view: ViewId) -> Vec<(String, Option<String>)> {
        let node = self.view(view).node;
        self.doc
            .descendants(node)
            .into_iter()
            .filter(|n| self.doc.local_name(*n) == "child")
            .filter_map(|n| Some((self.doc.attr(n, "id")?, self.doc.attr(n, "archimateElement"))))
            .collect()
    }
}
