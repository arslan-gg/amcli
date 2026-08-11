//! Atomic batches.
//!
//! An agent building a coherent subgraph needs many edits to land together or
//! not at all. Everything here is applied in memory, validated as a whole, and
//! written exactly once — so "rollback" is the absence of a mechanism rather
//! than a mechanism, which is the only kind that cannot itself fail.
//!
//! Two features carry the design. `ref` names a line's result so a later line
//! can point at it before its id exists, which is what makes a batch composable
//! at all. `if_absent` binds the ref to an existing concept instead of failing,
//! which makes a batch re-runnable after a half-finished attempt.

use std::collections::HashMap;
use std::io::Read;

use amcli_graph::{Graph, Resolution, Selector};
use amcli_model::{ConceptId, ElementType, Model, RelType};
use serde::Deserialize;

use crate::output::{CliError, Code, Output, Row};
use crate::write::{Opts, guard_checksum, save};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Op {
    #[serde(rename = "element.add")]
    ElementAdd {
        #[serde(rename = "type")]
        ty: String,
        name: String,
        folder: Option<String>,
        doc: Option<String>,
        #[serde(default)]
        props: HashMap<String, String>,
        #[serde(rename = "ref")]
        reference: Option<String>,
        #[serde(default)]
        if_absent: bool,
    },
    #[serde(rename = "relation.add")]
    RelationAdd {
        #[serde(rename = "type")]
        ty: String,
        source: String,
        target: String,
        access: Option<String>,
        doc: Option<String>,
        #[serde(rename = "ref")]
        reference: Option<String>,
        #[serde(default)]
        if_absent: bool,
    },
    #[serde(rename = "element.rename")]
    ElementRename { target: String, name: String },
    #[serde(rename = "element.doc")]
    ElementDoc { target: String, text: String },
    #[serde(rename = "element.delete")]
    ElementDelete { target: String },
    #[serde(rename = "prop.set")]
    PropSet { target: String, key: String, value: String },
    #[serde(rename = "folder.add")]
    FolderAdd { parent: String, name: String },
}

pub fn run(opts: &Opts, m: &mut Model, file: Option<&str>) -> Result<Output, CliError> {
    guard_checksum(m, opts)?;
    let before = m.checksum().map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?;

    let text = match file {
        None | Some("-") => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?;
            s
        }
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| CliError::new(Code::Io, "io", format!("{p}: {e}")))?,
    };

    let mut refs: HashMap<String, String> = HashMap::new();
    let mut rows: Vec<Row> = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let op: Op = serde_json::from_str(line).map_err(|e| {
            CliError::new(Code::Usage, "usage", format!("line {n}: {e}"))
                .hint("one JSON operation per line; see `amcli apply --help`")
        })?;

        match apply_one(m, &op, &mut refs) {
            Ok(row) => rows.push(row.n("line", n as i64)),
            Err(e) => {
                // The in-memory model may well have changed by now — earlier
                // lines applied — but `save` is only ever called after the
                // whole batch succeeds, so the file on disk is untouched. Say
                // exactly that and nothing more.
                return Err(CliError::new(e.code, e.kind, format!("line {n}: {}", e.message))
                    .hint(format!(
                        "{}; the file was not written, so re-run the whole batch",
                        e.hint.unwrap_or_else(|| "the batch was abandoned".into())
                    ))
                    .rows(e.rows));
            }
        }
    }

    if rows.is_empty() {
        return Ok(Output::empty().note("no operations"));
    }

    if !opts.dry_run {
        save(m)?;
    }
    let after = m.checksum().map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?;
    let applied = rows.len();
    let mut out = Output::rows(rows)
        .meta_n("applied", applied as i64)
        .meta("checksum_before", before)
        .meta("checksum_after", after)
        .meta_b("written", !opts.dry_run);
    if opts.dry_run {
        out = out.note("dry run: nothing was written");
    }
    Ok(out)
}

fn apply_one(m: &mut Model, op: &Op, refs: &mut HashMap<String, String>) -> Result<Row, CliError> {
    match op {
        Op::ElementAdd { ty, name, folder, doc, props, reference, if_absent } => {
            let t = ElementType::from_str(ty).ok_or_else(|| {
                CliError::new(Code::Usage, "usage", format!("`{ty}` is not an element type"))
            })?;

            let existing = if *if_absent { find_by_type_and_name(m, ty, name) } else { None };
            let (c, created) = match existing {
                Some(c) => (c, false),
                None => {
                    let f = folder.as_deref().map(|p| folder_of(m, p)).transpose()?;
                    let c = m
                        .add_element(t, name, f, doc.as_deref())
                        .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
                    (c, true)
                }
            };
            for (k, v) in props {
                m.set_property(c, k, v)
                    .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            }
            let id = m.concept(c).id.clone();
            if let Some(r) = reference {
                refs.insert(r.clone(), id.clone());
            }
            Ok(Row::new().s("op", "element.add").s("id", id).b("created", created))
        }

        Op::RelationAdd { ty, source, target, access, doc, reference, if_absent } => {
            let t = RelType::from_str(ty).ok_or_else(|| {
                CliError::new(Code::Usage, "usage", format!("`{ty}` is not a relationship type"))
            })?;
            let s = resolve(m, source, refs)?;
            let g = resolve(m, target, refs)?;
            let a = access.as_deref().map(access_value).transpose()?;

            if *if_absent && m.check_relationship(t, s, g).is_err() {
                // Already there, or not permitted. Only the first is a reason
                // to skip quietly, so the check is repeated to tell them apart.
                if let Err(amcli_model::EditError::DuplicateRelationship { existing, .. }) =
                    m.check_relationship(t, s, g)
                {
                    if let Some(r) = reference {
                        refs.insert(r.clone(), existing.clone());
                    }
                    return Ok(Row::new()
                        .s("op", "relation.add")
                        .s("id", existing)
                        .b("created", false));
                }
            }

            let c = m
                .add_relation(t, s, g, a, doc.as_deref())
                .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            let id = m.concept(c).id.clone();
            if let Some(r) = reference {
                refs.insert(r.clone(), id.clone());
            }
            Ok(Row::new().s("op", "relation.add").s("id", id).b("created", true))
        }

        Op::ElementRename { target, name } => {
            let c = resolve(m, target, refs)?;
            m.rename(c, name);
            Ok(Row::new().s("op", "element.rename").s("id", m.concept(c).id.clone()))
        }
        Op::ElementDoc { target, text } => {
            let c = resolve(m, target, refs)?;
            m.set_documentation(c, text)
                .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            Ok(Row::new().s("op", "element.doc").s("id", m.concept(c).id.clone()))
        }
        Op::ElementDelete { target } => {
            let c = resolve(m, target, refs)?;
            let id = m.concept(c).id.clone();
            let done = m
                .delete_concept(c)
                .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            Ok(Row::new().s("op", "element.delete").s("id", id).n("removed", done.total() as i64))
        }
        Op::PropSet { target, key, value } => {
            let c = resolve(m, target, refs)?;
            m.set_property(c, key, value)
                .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            Ok(Row::new()
                .s("op", "prop.set")
                .s("id", m.concept(c).id.clone())
                .s("key", key.clone()))
        }
        Op::FolderAdd { parent, name } => {
            let p = folder_of(m, parent)?;
            let f = m
                .add_folder(p, name)
                .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
            Ok(Row::new().s("op", "folder.add").s("path", m.folder(f).path.clone()))
        }
    }
}

fn find_by_type_and_name(m: &Model, ty: &str, name: &str) -> Option<ConceptId> {
    m.concepts_with_ids()
        .find(|(_, c)| c.name == name && c.kind.name().eq_ignore_ascii_case(ty))
        .map(|(i, _)| i)
}

/// `ref:name` refers to something an earlier line produced; anything else is an
/// ordinary selector. Refs resolve forwards only, so a typo is an error at the
/// line that used it rather than a mystery later on.
fn resolve(m: &Model, sel: &str, refs: &HashMap<String, String>) -> Result<ConceptId, CliError> {
    if let Some(name) = sel.strip_prefix("ref:") {
        let id = refs.get(name).ok_or_else(|| {
            CliError::new(Code::NotFound, "not_found", format!("no earlier line named `{name}`"))
                .hint("a ref must be defined by a previous line")
        })?;
        return m.concept_by_id(id).ok_or_else(|| {
            CliError::new(
                Code::NotFound,
                "not_found",
                format!("ref `{name}` points at a deleted concept"),
            )
        });
    }
    let g = Graph::build(m);
    match Selector::parse(sel).resolve_one(&g) {
        Resolution::One(c) => Ok(c),
        Resolution::Ambiguous(cs) => Err(CliError::new(
            Code::Ambiguous,
            "ambiguous",
            format!("{} concepts match `{sel}`", cs.len()),
        )
        .rows(
            cs.iter()
                .map(|c| Row::new().s("selector", format!("id:{}", m.concept(*c).id)))
                .collect(),
        )),
        Resolution::NotFound { .. } => {
            Err(CliError::new(Code::NotFound, "not_found", format!("nothing matches `{sel}`")))
        }
    }
}

fn folder_of(m: &Model, path: &str) -> Result<amcli_model::FolderId, CliError> {
    m.folder_by_path(path)
        .ok_or_else(|| CliError::new(Code::NotFound, "not_found", format!("no folder at `{path}`")))
}

fn access_value(s: &str) -> Result<i64, CliError> {
    Ok(match s {
        "write" | "w" => 0,
        "read" | "r" => 1,
        "unspecified" | "none" => 2,
        "rw" | "readwrite" => 3,
        _ => {
            return Err(CliError::new(
                Code::Usage,
                "usage",
                format!("`{s}` is not an access type"),
            ));
        }
    })
}
