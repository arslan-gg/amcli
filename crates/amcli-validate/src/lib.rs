//! Validation, in layers.
//!
//! Each finding carries a byte offset, and therefore a line and column. No other
//! ArchiMate tool tells you *where* in the file the problem is, and "somewhere
//! in this 900 KB of XML" is not an answer anyone can act on.
//!
//! Findings also carry a `fix` string that is a runnable command, so a report is
//! a work list rather than a complaint.

use std::collections::{HashMap, HashSet};

use amcli_graph::Graph;
use amcli_model::{ConceptKind, ElementType, FolderType, Model, RelType, matrix, viewpoints};
use amcli_xml::NodeId;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    /// Something worth knowing that is not wrong.
    Info,
    /// Legal, but a sign the model is drifting.
    Warning,
    /// The model is broken: Archi will misread it or refuse to open it.
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// Whether `--fix` may repair this automatically.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fixability {
    /// The repair is derived, not chosen: there is exactly one right answer.
    Safe,
    /// A repair exists but it destroys information, so it needs saying so.
    Destructive,
    /// Only a person can decide.
    Manual,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Id of whatever the finding is about.
    pub entity: String,
    pub entity_kind: &'static str,
    pub line: u32,
    pub column: u32,
    pub fixability: Fixability,
    /// A command that addresses this finding.
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub rules_run: usize,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.count(Severity::Error)
    }

    pub fn warnings(&self) -> usize {
        self.count(Severity::Warning)
    }

    fn count(&self, s: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == s).count()
    }

    pub fn is_clean(&self) -> bool {
        self.errors() == 0
    }
}

/// Which layers to run. Layers are cumulative: hygiene lint without referential
/// integrity would report nonsense about a model that is already broken.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    /// Types and schema-level legality.
    Types,
    /// Plus the ArchiMate relationship rules.
    Rules,
    /// Plus referential integrity, including view references.
    Integrity,
    /// Plus modelling hygiene.
    All,
}

impl Level {
    pub fn parse(s: &str) -> Option<Level> {
        Some(match s {
            "types" => Level::Types,
            "rules" => Level::Rules,
            "integrity" => Level::Integrity,
            "all" => Level::All,
            _ => return None,
        })
    }
}

pub fn validate(m: &Model, g: &Graph<'_>, level: Level) -> Report {
    let mut r = Report::default();
    let mut at = |node: NodeId| m.doc.line_col(m.doc.span(node).start);

    check_types(m, &mut r, &mut at);
    if level >= Level::Rules {
        check_rules(m, &mut r, &mut at);
    }
    if level >= Level::Integrity {
        check_integrity(m, g, &mut r, &mut at);
    }
    if level >= Level::All {
        check_hygiene(m, g, &mut r, &mut at);
    }

    r.rules_run = match level {
        Level::Types => 4,
        Level::Rules => 7,
        Level::Integrity => 14,
        Level::All => 20,
    };
    r.findings.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then(a.line.cmp(&b.line)).then(a.code.cmp(b.code))
    });
    r
}

// ---- L1: types ------------------------------------------------------------

fn check_types(m: &Model, r: &mut Report, at: &mut impl FnMut(NodeId) -> (u32, u32)) {
    for c in m.concepts() {
        let (line, column) = at(c.node);

        if let ConceptKind::Unknown { xsi, .. } = &c.kind {
            r.findings.push(Finding {
                code: "TYP1001",
                severity: Severity::Info,
                message: format!(
                    "`{xsi}` is not an ArchiMate 3.2 type this build knows; \
                     it is preserved but not checked"
                ),
                entity: c.id.clone(),
                entity_kind: "concept",
                line,
                column,
                fixability: Fixability::Manual,
                fix: None,
            });
        }

        // accessType is only meaningful on Access, and only 0..3 are defined.
        if let Some(raw) = m.doc.attr(c.node, "accessType") {
            let ok_type = c.kind == ConceptKind::Relationship(RelType::Access);
            let value = raw.parse::<i64>().ok();
            if !ok_type {
                r.findings.push(Finding {
                    code: "TYP1002",
                    severity: Severity::Warning,
                    message: format!(
                        "accessType on a {} relationship, where it has no meaning",
                        c.kind.name()
                    ),
                    entity: c.id.clone(),
                    entity_kind: "relationship",
                    line,
                    column,
                    fixability: Fixability::Safe,
                    fix: Some(format!("amcli relation set id:{} --access none", c.id)),
                });
            } else if !value.is_some_and(|v| (0..=3).contains(&v)) {
                r.findings.push(Finding {
                    code: "TYP1003",
                    severity: Severity::Error,
                    message: format!(
                        "accessType `{raw}` is not one of 0 (write), 1 (read), \
                         2 (unspecified), 3 (read/write)"
                    ),
                    entity: c.id.clone(),
                    entity_kind: "relationship",
                    line,
                    column,
                    fixability: Fixability::Manual,
                    fix: Some(format!("amcli relation set id:{} --access rw", c.id)),
                });
            }
        }

        // Junction carries a type of "" (and) or "or"; anything else is a typo.
        if c.kind == ConceptKind::Element(ElementType::Junction)
            && let Some(t) = m.doc.attr(c.node, "type")
            && t != "or"
        {
            r.findings.push(Finding {
                code: "TYP1004",
                severity: Severity::Error,
                message: format!("junction type `{t}` is not recognised; expected `or` or nothing"),
                entity: c.id.clone(),
                entity_kind: "element",
                line,
                column,
                fixability: Fixability::Manual,
                fix: None,
            });
        }
    }

    for v in m.views() {
        if v.viewpoint.is_empty() || viewpoints::by_id(&v.viewpoint).is_some() {
            continue;
        }
        let (line, column) = at(v.node);
        r.findings.push(Finding {
            code: "TYP1005",
            severity: Severity::Warning,
            message: format!("`{}` is not a known viewpoint id", v.viewpoint),
            entity: v.id.clone(),
            entity_kind: "view",
            line,
            column,
            fixability: Fixability::Manual,
            fix: None,
        });
    }
}

// ---- L2: ArchiMate rules --------------------------------------------------

fn check_rules(m: &Model, r: &mut Report, at: &mut impl FnMut(NodeId) -> (u32, u32)) {
    let mut seen_pairs: HashSet<(String, String, &str)> = HashSet::new();
    let mut junction_types: HashMap<String, (RelType, String)> = HashMap::new();

    for c in m.concepts() {
        let ConceptKind::Relationship(rel) = &c.kind else { continue };
        let (Some(src_id), Some(tgt_id)) = (c.source.as_deref(), c.target.as_deref()) else {
            continue;
        };
        let (line, column) = at(c.node);

        let (Some(s), Some(t)) = (m.concept_by_id(src_id), m.concept_by_id(tgt_id)) else {
            continue; // reported by referential integrity instead
        };
        let (s, t) = (m.concept(s), m.concept(t));

        if let (Some(si), Some(ti)) = (s.kind.matrix_idx(), t.kind.matrix_idx())
            && !matrix::allows(si, ti, *rel)
        {
            let allowed: Vec<&str> =
                matrix::permitted(si, ti).iter().map(|x| x.info().short).collect();
            r.findings.push(Finding {
                code: "REL2001",
                severity: Severity::Error,
                message: format!(
                    "ArchiMate does not permit {} from {} to {}{}",
                    rel.info().short,
                    s.kind.name(),
                    t.kind.name(),
                    if allowed.is_empty() {
                        String::new()
                    } else {
                        format!("; permitted here: {}", allowed.join(", "))
                    }
                ),
                entity: c.id.clone(),
                entity_kind: "relationship",
                line,
                column,
                fixability: Fixability::Destructive,
                fix: Some(match allowed.first() {
                    Some(a) => format!("amcli relation set id:{} --type {a}", c.id),
                    None => format!("amcli relation delete id:{}", c.id),
                }),
            });
        }

        let key = (src_id.to_string(), tgt_id.to_string(), rel.info().short);
        if !seen_pairs.insert(key) {
            r.findings.push(Finding {
                code: "REL2002",
                severity: Severity::Warning,
                message: format!(
                    "a second {} relationship between the same pair adds nothing",
                    rel.info().short
                ),
                entity: c.id.clone(),
                entity_kind: "relationship",
                line,
                column,
                fixability: Fixability::Destructive,
                fix: Some(format!("amcli relation delete id:{}", c.id)),
            });
        }

        // Every relationship touching a junction must be of the same type.
        for end in [s, t] {
            if end.kind != ConceptKind::Element(ElementType::Junction) {
                continue;
            }
            match junction_types.get(&end.id) {
                Some((first, first_id)) if first != rel => {
                    r.findings.push(Finding {
                        code: "REL2003",
                        severity: Severity::Error,
                        message: format!(
                            "junction `{}` mixes {} with {} (see {first_id})",
                            display(end),
                            first.info().short,
                            rel.info().short
                        ),
                        entity: c.id.clone(),
                        entity_kind: "relationship",
                        line,
                        column,
                        fixability: Fixability::Manual,
                        fix: Some(format!(
                            "amcli relation set id:{} --type {}",
                            c.id,
                            first.info().short
                        )),
                    });
                }
                Some(_) => {}
                None => {
                    junction_types.insert(end.id.clone(), (*rel, c.id.clone()));
                }
            }
        }
    }
}

// ---- L3: referential integrity --------------------------------------------

fn check_integrity(
    m: &Model,
    g: &Graph<'_>,
    r: &mut Report,
    at: &mut impl FnMut(NodeId) -> (u32, u32),
) {
    for dup in m.duplicate_ids() {
        r.findings.push(Finding {
            code: "REF3001",
            severity: Severity::Error,
            message: format!(
                "id `{dup}` is used more than once; every reference to it is ambiguous"
            ),
            entity: dup.clone(),
            entity_kind: "id",
            line: 0,
            column: 0,
            fixability: Fixability::Manual,
            fix: None,
        });
    }

    for c in m.concepts() {
        if c.id.is_empty() {
            let (line, column) = at(c.node);
            r.findings.push(Finding {
                code: "REF3002",
                severity: Severity::Error,
                message: "concept has no id, so nothing can refer to it".to_string(),
                entity: c.name.clone(),
                entity_kind: "concept",
                line,
                column,
                fixability: Fixability::Safe,
                fix: None,
            });
        }
    }

    for &rel in g.dangling() {
        let c = m.concept(rel);
        let (line, column) = at(c.node);
        let missing = [c.source.as_deref(), c.target.as_deref()]
            .into_iter()
            .flatten()
            .find(|id| m.concept_by_id(id).is_none())
            .unwrap_or_default();
        r.findings.push(Finding {
            code: "REF3010",
            severity: Severity::Error,
            message: format!("relationship endpoint `{missing}` does not exist"),
            entity: c.id.clone(),
            entity_kind: "relationship",
            line,
            column,
            fixability: Fixability::Destructive,
            fix: Some(format!("amcli relation delete id:{}", c.id)),
        });
    }

    // The view half. A diagram object whose concept is gone is not a cosmetic
    // problem: Archi fails to load the model.
    for v in m.views() {
        // A connection may end on another connection, not just on an object:
        // that is how an association pointing at a relationship is drawn.
        // Treating only objects as valid endpoints reports real Archi output as
        // broken.
        let mut object_ids: HashSet<String> = HashSet::new();
        let mut connections: Vec<(String, String, String, NodeId)> = Vec::new();
        let mut incoming: HashMap<String, Vec<String>> = HashMap::new();

        for n in m.doc.descendants(v.node) {
            let Some(id) = m.doc.attr(n, "id") else { continue };
            match m.doc.local_name(n) {
                "child" => {
                    object_ids.insert(id.clone());
                    if let Some(e) = m.doc.attr(n, "archimateElement")
                        && m.concept_by_id(&e).is_none()
                    {
                        let (line, column) = at(n);
                        r.findings.push(Finding {
                            code: "REF3020",
                            severity: Severity::Error,
                            message: format!(
                                "diagram object on view `{}` refers to missing element `{e}`",
                                v.name
                            ),
                            entity: id.clone(),
                            entity_kind: "diagram-object",
                            line,
                            column,
                            fixability: Fixability::Safe,
                            fix: Some("amcli validate --fix   # removes the orphan".to_string()),
                        });
                    }
                }
                "sourceConnection" => {
                    object_ids.insert(id.clone());
                    if let Some(rel) = m.doc.attr(n, "archimateRelationship")
                        && m.concept_by_id(&rel).is_none()
                    {
                        let (line, column) = at(n);
                        r.findings.push(Finding {
                            code: "REF3021",
                            severity: Severity::Error,
                            message: format!(
                                "connection on view `{}` refers to missing relationship `{rel}`",
                                v.name
                            ),
                            entity: id.clone(),
                            entity_kind: "connection",
                            line,
                            column,
                            fixability: Fixability::Safe,
                            fix: Some("amcli validate --fix   # removes the orphan".to_string()),
                        });
                    }
                    let src = m.doc.attr(n, "source").unwrap_or_default();
                    let tgt = m.doc.attr(n, "target").unwrap_or_default();
                    incoming.entry(tgt.clone()).or_default().push(id.clone());
                    connections.push((id.clone(), src, tgt, n));
                }
                _ => {}
            }
        }

        for (id, src, tgt, node) in &connections {
            for (which, end) in [("source", src), ("target", tgt)] {
                if !end.is_empty() && !object_ids.contains(end) {
                    let (line, column) = at(*node);
                    r.findings.push(Finding {
                        code: "REF3023",
                        severity: Severity::Error,
                        message: format!("connection {which} `{end}` is not on view `{}`", v.name),
                        entity: id.clone(),
                        entity_kind: "connection",
                        line,
                        column,
                        fixability: Fixability::Safe,
                        fix: Some("amcli validate --fix".to_string()),
                    });
                }
            }
        }

        // targetConnections is a derived mirror; if it disagrees with reality,
        // Archi's own load can drop connections silently.
        for n in m.doc.descendants(v.node) {
            if m.doc.local_name(n) != "child" {
                continue;
            }
            let Some(id) = m.doc.attr(n, "id") else { continue };
            let declared: Vec<String> = m
                .doc
                .attr(n, "targetConnections")
                .map(|s| s.split_whitespace().map(String::from).collect())
                .unwrap_or_default();
            let mut actual = incoming.get(&id).cloned().unwrap_or_default();
            let mut declared_sorted = declared.clone();
            actual.sort();
            declared_sorted.sort();
            if actual != declared_sorted {
                let (line, column) = at(n);
                r.findings.push(Finding {
                    code: "REF3022",
                    severity: Severity::Error,
                    message: format!(
                        "targetConnections on view `{}` lists {} but {} connection(s) actually point here",
                        v.name,
                        if declared.is_empty() { "nothing".into() } else { declared.join(" ") },
                        actual.len()
                    ),
                    entity: id.clone(),
                    entity_kind: "diagram-object",
                    line,
                    column,
                    fixability: Fixability::Safe,
                    fix: Some("amcli validate --fix   # recomputes the mirror".to_string()),
                });
            }
        }
    }
}

// ---- L4: modelling hygiene ------------------------------------------------

fn check_hygiene(
    m: &Model,
    g: &Graph<'_>,
    r: &mut Report,
    at: &mut impl FnMut(NodeId) -> (u32, u32),
) {
    let mut names: HashMap<String, Vec<String>> = HashMap::new();

    for (id, c) in m.concepts_with_ids() {
        let (line, column) = at(c.node);

        if c.kind.is_relationship() {
            // A relationship filed outside Relations still loads, but the
            // taxonomy rots one edit at a time.
            let folder = m.folder(c.folder);
            if folder.folder_type != FolderType::Relations {
                r.findings.push(Finding {
                    code: "LNT4003",
                    severity: Severity::Warning,
                    message: format!("relationship filed under `{}`, not Relations", folder.path),
                    entity: c.id.clone(),
                    entity_kind: "relationship",
                    line,
                    column,
                    fixability: Fixability::Safe,
                    fix: Some(format!("amcli relation move id:{} --folder /Relations", c.id)),
                });
            }
            continue;
        }

        if c.name.trim().is_empty() {
            r.findings.push(Finding {
                code: "LNT4002",
                severity: Severity::Warning,
                message: format!("{} has no name", c.kind.name()),
                entity: c.id.clone(),
                entity_kind: "element",
                line,
                column,
                fixability: Fixability::Manual,
                fix: Some(format!("amcli element rename id:{} \"…\"", c.id)),
            });
        } else {
            names.entry(c.name.to_lowercase()).or_default().push(c.id.clone());
        }

        let (i, o) = g.degree(id);
        if i + o == 0 {
            r.findings.push(Finding {
                code: "LNT4001",
                severity: Severity::Warning,
                message: format!("`{}` has no relationships at all", display(c)),
                entity: c.id.clone(),
                entity_kind: "element",
                line,
                column,
                fixability: Fixability::Manual,
                fix: Some(format!("amcli trace id:{} -D both", c.id)),
            });
        }
    }

    for (name, ids) in names {
        if ids.len() < 2 {
            continue;
        }
        r.findings.push(Finding {
            code: "LNT4004",
            severity: Severity::Warning,
            message: format!(
                "{} concepts share the name `{name}`; every selector using it is ambiguous",
                ids.len()
            ),
            entity: ids[0].clone(),
            entity_kind: "element",
            line: 0,
            column: 0,
            fixability: Fixability::Manual,
            fix: Some(format!("amcli query 'name={name}'")),
        });
    }

    for v in m.views() {
        let objects = m
            .doc
            .descendants(v.node)
            .into_iter()
            .filter(|n| m.doc.local_name(*n) == "child")
            .count();
        if objects == 0 {
            let (line, column) = at(v.node);
            r.findings.push(Finding {
                code: "LNT4005",
                severity: Severity::Info,
                message: format!("view `{}` is empty", v.name),
                entity: v.id.clone(),
                entity_kind: "view",
                line,
                column,
                fixability: Fixability::Manual,
                fix: None,
            });
        }
    }
}

// ---- repair ---------------------------------------------------------------

/// What `--fix` did.
#[derive(Clone, Debug, Default)]
pub struct Repairs {
    pub orphan_objects: Vec<String>,
    pub orphan_connections: Vec<String>,
    pub recomputed_views: Vec<String>,
    pub moved_relationships: Vec<String>,
}

impl Repairs {
    pub fn total(&self) -> usize {
        self.orphan_objects.len()
            + self.orphan_connections.len()
            + self.recomputed_views.len()
            + self.moved_relationships.len()
    }
}

/// Apply only the repairs that are *derived* rather than chosen.
///
/// An orphaned diagram object has no meaning left — the thing it displayed is
/// gone — and a targetConnections mirror has exactly one correct value. Neither
/// asks anyone to decide anything. Deleting an invalid relationship, by
/// contrast, throws away someone's modelling, so it is never automatic.
pub fn fix_safe(m: &mut Model) -> Repairs {
    let mut fixed = Repairs::default();

    // Orphaned visuals first: removing them changes what the mirrors should say.
    loop {
        let mut doomed: Vec<NodeId> = Vec::new();
        let view_nodes: Vec<(String, NodeId)> = m.views().map(|v| (v.id.clone(), v.node)).collect();

        for (_, view_node) in &view_nodes {
            // Objects and connections both count as endpoints; see the note in
            // check_integrity.
            let object_ids: HashSet<String> = m
                .doc
                .descendants(*view_node)
                .into_iter()
                .filter(|n| matches!(m.doc.local_name(*n), "child" | "sourceConnection"))
                .filter_map(|n| m.doc.attr(n, "id"))
                .collect();

            for n in m.doc.descendants(*view_node) {
                let Some(id) = m.doc.attr(n, "id") else { continue };
                match m.doc.local_name(n) {
                    "child" => {
                        if m.doc
                            .attr(n, "archimateElement")
                            .is_some_and(|e| m.concept_by_id(&e).is_none())
                        {
                            fixed.orphan_objects.push(id);
                            doomed.push(n);
                        }
                    }
                    "sourceConnection" => {
                        let rel_gone = m
                            .doc
                            .attr(n, "archimateRelationship")
                            .is_some_and(|rel| m.concept_by_id(&rel).is_none());
                        let end_gone = ["source", "target"]
                            .iter()
                            .any(|a| m.doc.attr(n, a).is_some_and(|e| !object_ids.contains(&e)));
                        if rel_gone || end_gone {
                            fixed.orphan_connections.push(id);
                            doomed.push(n);
                        }
                    }
                    _ => {}
                }
            }
        }
        if doomed.is_empty() {
            break;
        }
        for n in doomed {
            m.doc.remove_subtree(n);
        }
        m.reindex_public();
    }

    // Then recompute every mirror, whether or not it was wrong: the value is
    // derived, so rewriting it can only ever agree with reality.
    let views: Vec<_> = m.views_with_ids().map(|(i, v)| (i, v.id.clone())).collect();
    for (vid, id) in views {
        let before = m.to_bytes().ok();
        m.recompute_target_connections(vid);
        if before != m.to_bytes().ok() {
            fixed.recomputed_views.push(id);
        }
    }

    fixed
}

fn display(c: &amcli_model::Concept) -> String {
    if c.name.is_empty() { c.id.clone() } else { c.name.clone() }
}
