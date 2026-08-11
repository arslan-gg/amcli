//! Read commands.

use std::path::PathBuf;

use amcli_graph::{Dir, EdgeFilter, Graph, MatchField, Resolution, Selector};
use amcli_model::{ConceptId, Model, RelType};
use amcli_validate::{Fixability, Level};

use crate::output::{CliError, Code, Output, Row};
use crate::write;

pub struct Ctx {
    pub limit: usize,
    pub path: PathBuf,
}

impl Ctx {
    fn cap(&self, n: usize) -> usize {
        if self.limit == 0 { n } else { self.limit.min(n) }
    }
}

/// Resolve a selector to exactly one concept, or fail with something the caller
/// can act on: candidates to choose from, or nearby names to try.
fn one(g: &Graph<'_>, sel: &str) -> Result<ConceptId, CliError> {
    let selector = Selector::parse(sel);
    match selector.resolve_one(g) {
        Resolution::One(c) => Ok(c),
        Resolution::Ambiguous(cs) => {
            let m = g.model();
            Err(CliError::new(
                Code::Ambiguous,
                "ambiguous",
                format!("{} concepts match `{sel}`", cs.len()),
            )
            .hint("re-run with one of the selectors below, or add -t TYPE")
            .rows(
                cs.iter()
                    .map(|c| {
                        let concept = m.concept(*c);
                        let (i, o) = g.degree(*c);
                        Row::new()
                            .s("selector", format!("id:{}", concept.id))
                            .s("type", concept.kind.name())
                            .s("name", concept.name.clone())
                            .s("folder", m.folder_path_of(concept))
                            .n("degree", (i + o) as i64)
                    })
                    .collect(),
            ))
        }
        Resolution::NotFound { suggestions } => {
            let m = g.model();
            let err =
                CliError::new(Code::NotFound, "not_found", format!("nothing matches `{sel}`"));
            if suggestions.is_empty() {
                Err(err.hint("try `amcli search` with part of the name"))
            } else {
                Err(err.hint("did you mean one of these?").rows(
                    suggestions
                        .iter()
                        .map(|c| {
                            let concept = m.concept(*c);
                            Row::new()
                                .s("selector", format!("id:{}", concept.id))
                                .s("type", concept.kind.name())
                                .s("name", concept.name.clone())
                        })
                        .collect(),
                ))
            }
        }
    }
}

fn rel_filter(rel: Option<&str>) -> Result<EdgeFilter, CliError> {
    let Some(r) = rel else { return Ok(EdgeFilter::default()) };
    let mut types = Vec::new();
    for name in r.split(',') {
        match RelType::from_str(name.trim()) {
            Some(t) => types.push(t),
            None => {
                return Err(CliError::new(
                    Code::Usage,
                    "usage",
                    format!("`{name}` is not a relationship type"),
                )
                .hint(format!(
                    "one of: {}",
                    RelType::ALL.iter().map(|r| r.info().short).collect::<Vec<_>>().join(", ")
                )));
            }
        }
    }
    Ok(EdgeFilter::only(types))
}

fn direction(d: &str) -> Result<Dir, CliError> {
    Dir::parse(d).ok_or_else(|| {
        CliError::new(Code::Usage, "usage", format!("`{d}` is not a direction"))
            .hint("one of: out, in, both")
    })
}

fn concept_row(m: &Model, g: &Graph<'_>, c: ConceptId) -> Row {
    let concept = m.concept(c);
    let (i, o) = g.degree(c);
    Row::new()
        .s("id", concept.id.clone())
        .s("type", concept.kind.name())
        .s("name", concept.name.clone())
        .s("folder", m.folder_path_of(concept))
        .n("in", i as i64)
        .n("out", o as i64)
}

/// Documentation is never returned whole in a list: one long blob can cost more
/// than every other result put together.
fn clip(s: &str, full: bool) -> String {
    const MAX: usize = 500;
    if full || s.chars().count() <= MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX).collect();
    out.push('…');
    out
}

pub fn get(g: &Graph<'_>, ctx: &Ctx, sel: &str, full: bool) -> Result<Output, CliError> {
    let m = g.model();
    let c = one(g, sel)?;
    let concept = m.concept(c);

    let rels: Vec<Row> = g
        .neighbors(c, Dir::Both, &EdgeFilter::default())
        .iter()
        .map(|a| {
            let rel = m.concept(a.rel);
            let other = m.concept(a.other);
            Row::new()
                // The relationship's own id: without it there is no way to
                // address a relationship for editing.
                .s("id", rel.id.clone())
                .s("direction", if a.dir == Dir::Out { "out" } else { "in" })
                .s("type", rel.kind.name())
                .s("other_id", other.id.clone())
                .s("other_type", other.kind.name())
                .s("other_name", other.name.clone())
        })
        .collect();

    let views: Vec<Row> = m
        .views()
        .filter(|v| {
            m.doc.descendants(v.node).into_iter().any(|n| {
                m.doc.attr(n, "archimateElement").as_deref() == Some(concept.id.as_str())
                    || m.doc.attr(n, "archimateRelationship").as_deref()
                        == Some(concept.id.as_str())
            })
        })
        .map(|v| Row::new().s("id", v.id.clone()).s("name", v.name.clone()))
        .collect();

    let props: Vec<Row> = m
        .properties(concept.node)
        .into_iter()
        .map(|(k, v)| Row::new().s("key", k).s("value", v))
        .collect();

    let doc = m.documentation(concept.node).unwrap_or_default();
    let row = concept_row(m, g, c)
        .opt("layer", concept.kind.layer().map(|l| l.as_str().to_string()))
        .s("documentation", clip(&doc, full))
        .b("documentation_truncated", !full && doc.chars().count() > 500)
        .list("properties", props)
        .list("views", views)
        .list("relations", rels);

    Ok(Output::one(row).meta("model", ctx.path.display().to_string()))
}

pub fn search(g: &Graph<'_>, ctx: &Ctx, query: &str, ty: Option<&str>) -> Result<Output, CliError> {
    let m = g.model();
    let hits = g.search(query, usize::MAX);
    let filtered: Vec<_> = hits
        .into_iter()
        .filter(|h| ty.is_none_or(|t| m.concept(h.concept).kind.name().eq_ignore_ascii_case(t)))
        .collect();

    let total = filtered.len();
    let shown = ctx.cap(total);
    let rows: Vec<Row> = filtered
        .iter()
        .take(shown)
        .map(|h| {
            concept_row(m, g, h.concept).s("matched", h.field.as_str()).s(
                "snippet",
                if h.field == MatchField::Name { String::new() } else { h.snippet.clone() },
            )
        })
        .collect();

    let mut out =
        Output::rows(rows).meta_n("total", total as i64).meta_b("truncated", shown < total);
    if shown < total {
        out = out.note(format!("{total} total, showing {shown} — narrow with -t, or raise -l"));
    }
    Ok(out)
}

pub fn list(
    g: &Graph<'_>,
    ctx: &Ctx,
    ty: Option<&str>,
    folder: Option<&str>,
) -> Result<Output, CliError> {
    let m = g.model();
    let all: Vec<ConceptId> = m
        .concepts_with_ids()
        .filter(|(_, c)| ty.is_none_or(|t| c.kind.name().eq_ignore_ascii_case(t)))
        .filter(|(_, c)| folder.is_none_or(|f| m.folder_path_of(c).starts_with(f)))
        .map(|(i, _)| i)
        .collect();

    let total = all.len();
    let shown = ctx.cap(total);
    let rows = all.iter().take(shown).map(|c| concept_row(m, g, *c)).collect();
    let mut out =
        Output::rows(rows).meta_n("total", total as i64).meta_b("truncated", shown < total);
    if shown < total {
        out = out.note(format!("{total} total, showing {shown}"));
    }
    Ok(out)
}

pub fn query(g: &Graph<'_>, ctx: &Ctx, expr: &str) -> Result<Output, CliError> {
    // A filter that fails to parse should say so rather than being treated as a
    // name that happens to match nothing.
    if let Err(e) = amcli_graph::select::Expr::parse(expr) {
        return Err(CliError::new(Code::Usage, "usage", e.to_string())
            .hint("fields: id name type layer folder doc deg view prop:KEY in:Rel out:Rel"));
    }
    let matches = Selector::parse(expr).matches(g);
    let total = matches.len();
    let shown = ctx.cap(total);
    let rows = matches.iter().take(shown).map(|c| concept_row(g.model(), g, *c)).collect();
    let mut out =
        Output::rows(rows).meta_n("total", total as i64).meta_b("truncated", shown < total);
    if shown < total {
        out = out.note(format!("{total} total, showing {shown}"));
    }
    Ok(out)
}

pub fn neighbors(
    g: &Graph<'_>,
    ctx: &Ctx,
    sel: &str,
    dir: &str,
    rel: Option<&str>,
) -> Result<Output, CliError> {
    let m = g.model();
    let c = one(g, sel)?;
    let arcs = g.neighbors(c, direction(dir)?, &rel_filter(rel)?);
    let total = arcs.len();
    let rows = arcs
        .iter()
        .take(ctx.cap(total))
        .map(|a| {
            let r = m.concept(a.rel);
            concept_row(m, g, a.other)
                .s("via", r.kind.name())
                .s("via_id", r.id.clone())
                .s("direction", if a.dir == Dir::Out { "out" } else { "in" })
        })
        .collect();
    Ok(Output::rows(rows).meta_n("total", total as i64))
}

pub fn trace(
    g: &Graph<'_>,
    ctx: &Ctx,
    sel: &str,
    dir: &str,
    depth: u32,
    rel: Option<&str>,
    ty: Option<&str>,
) -> Result<Output, CliError> {
    let m = g.model();
    let root = one(g, sel)?;
    let max_nodes = if ctx.limit == 0 { 100_000 } else { ctx.limit.max(50) * 10 };
    let sub = g.k_hop(&[root], depth, direction(dir)?, &rel_filter(rel)?, max_nodes);

    // The type filter projects the result; the walk itself crossed every type,
    // which is what keeps a multi-hop query from coming back empty.
    let nodes: Vec<Row> = sub
        .nodes
        .iter()
        .filter(|(c, _)| ty.is_none_or(|t| m.concept(*c).kind.name().eq_ignore_ascii_case(t)))
        .map(|(c, d)| {
            let mut r = Row::new().s("kind", "node");
            r.0.extend(concept_row(m, g, *c).0);
            r.n("depth", *d as i64)
        })
        .collect();

    let edges: Vec<Row> = sub
        .edges
        .iter()
        .filter_map(|e| {
            let r = m.concept(*e);
            let (s, t) = g.ends(*e)?;
            Some(
                Row::new()
                    .s("kind", "edge")
                    // Edges are keyed by id, never by name: two concepts can
                    // share a name, and then a name-keyed edge is simply wrong.
                    .s("id", r.id.clone())
                    .s("type", r.kind.name())
                    .s("name", r.name.clone())
                    .s("source", m.concept(s).id.clone())
                    .s("target", m.concept(t).id.clone()),
            )
        })
        .collect();

    // Flat records, not a nested object: nesting would collapse to a count in
    // text output, and the nodes and edges are exactly what the caller came for.
    // A leading `kind` column keeps the two record shapes greppable.
    let mut rows: Vec<Row> = Vec::with_capacity(nodes.len() + edges.len());
    rows.extend(nodes);
    rows.extend(edges);

    let mut out = Output::rows(rows)
        .meta("root", m.concept(root).id.clone())
        .meta_n("depth", depth as i64)
        .meta("direction", dir.to_string())
        .meta_n("nodes", sub.nodes.len() as i64)
        .meta_n("edges", sub.edges.len() as i64)
        .meta_b("truncated", sub.truncated);
    if sub.truncated {
        out = out.note("the walk hit its node limit; raise -l or lower -n");
    }
    Ok(out)
}

pub fn path(
    g: &Graph<'_>,
    ctx: &Ctx,
    from: &str,
    to: &str,
    dir: &str,
    all: bool,
    depth: u32,
) -> Result<Output, CliError> {
    let m = g.model();
    let a = one(g, from)?;
    let b = one(g, to)?;
    let d = direction(dir)?;
    let f = EdgeFilter::default();

    let (paths, truncated) = if all {
        g.all_paths(a, b, depth, ctx.cap(20).max(1), d, &f)
    } else {
        (g.shortest_path(a, b, d, &f).into_iter().collect(), false)
    };

    if paths.is_empty() {
        return Ok(Output::empty()
            .meta_n("total", 0)
            .note(format!("no path from `{from}` to `{to}` going {dir}"))
            .note("try -D both, or `amcli impact` to see what is reachable"));
    }

    let rows = paths
        .iter()
        .map(|p| {
            let hops: Vec<Row> = p
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let concept = m.concept(*n);
                    let via = i.checked_sub(1).and_then(|j| p.edges.get(j));
                    Row::new()
                        .s("id", concept.id.clone())
                        .s("type", concept.kind.name())
                        .s("name", concept.name.clone())
                        .opt("via", via.map(|e| m.concept(*e).kind.name().to_string()))
                        .opt("via_id", via.map(|e| m.concept(*e).id.clone()))
                })
                .collect();
            Row::new().n("hops", p.len() as i64).list("path", hops)
        })
        .collect();

    Ok(Output::rows(rows).meta_n("total", paths.len() as i64).meta_b("truncated", truncated))
}

pub fn impact(
    g: &Graph<'_>,
    ctx: &Ctx,
    sel: &str,
    dir: &str,
    depth: Option<u32>,
) -> Result<Output, CliError> {
    let m = g.model();
    let c = one(g, sel)?;
    let max = if ctx.limit == 0 { 100_000 } else { ctx.limit.max(50) * 10 };
    let (hits, truncated) = g.impact(&[c], direction(dir)?, depth, &EdgeFilter::default(), max);

    let total = hits.len();
    let rows = hits
        .iter()
        .take(ctx.cap(total))
        .map(|(c, d, why)| {
            concept_row(m, g, *c).n("depth", *d as i64).opt(
                "via",
                why.map(|r| format!("{} {}", m.concept(r).kind.name(), m.concept(r).id)),
            )
        })
        .collect();

    Ok(Output::rows(rows).meta_n("total", total as i64).meta_b("truncated", truncated))
}

pub fn containment(g: &Graph<'_>, ctx: &Ctx, sel: &str, up: bool) -> Result<Output, CliError> {
    let m = g.model();
    let c = one(g, sel)?;
    let found = if up {
        g.ancestors(c, &Graph::CONTAINMENT)
    } else {
        g.descendants(c, &Graph::CONTAINMENT)
    };
    let total = found.len();
    let rows = found.iter().take(ctx.cap(total)).map(|c| concept_row(m, g, *c)).collect();
    Ok(Output::rows(rows).meta_n("total", total as i64))
}

pub fn cycles(g: &Graph<'_>, ctx: &Ctx, rel: Option<&str>) -> Result<Output, CliError> {
    let m = g.model();
    let found = g.cycles(&rel_filter(rel)?);
    let total = found.len();
    let rows = found
        .iter()
        .take(ctx.cap(total))
        .map(|comp| {
            Row::new().n("size", comp.len() as i64).list(
                "members",
                comp.iter()
                    .map(|c| {
                        let concept = m.concept(*c);
                        Row::new()
                            .s("id", concept.id.clone())
                            .s("type", concept.kind.name())
                            .s("name", concept.name.clone())
                    })
                    .collect(),
            )
        })
        .collect();
    let mut out = Output::rows(rows).meta_n("total", total as i64);
    if total == 0 {
        out = out.note("no cycles");
    }
    Ok(out)
}

pub fn stats(g: &Graph<'_>, ctx: &Ctx) -> Result<Output, CliError> {
    let s = g.stats();
    let mut by_type: Vec<_> = s.by_type.iter().collect();
    by_type.sort_by_key(|(name, count)| (std::cmp::Reverse(**count), (*name).clone()));
    let mut by_layer: Vec<_> = s.by_layer.iter().collect();
    by_layer.sort_by_key(|(l, _)| l.as_str());

    // One labelled record per line. A single wide row would print as a column
    // of bare numbers, which is unreadable and un-greppable.
    let mut rows = vec![
        Row::new().s("kind", "total").s("key", "elements").n("count", s.elements as i64),
        Row::new().s("kind", "total").s("key", "relationships").n("count", s.relationships as i64),
        Row::new().s("kind", "total").s("key", "views").n("count", s.views as i64),
        Row::new().s("kind", "total").s("key", "folders").n("count", s.folders as i64),
        Row::new().s("kind", "total").s("key", "orphans").n("count", s.orphans as i64),
    ];
    rows.extend(
        by_layer.iter().map(|(l, c)| {
            Row::new().s("kind", "layer").s("key", l.as_str()).n("count", **c as i64)
        }),
    );
    rows.extend(
        by_type.iter().map(|(t, c)| {
            Row::new().s("kind", "type").s("key", (*t).clone()).n("count", **c as i64)
        }),
    );

    Ok(Output::rows(rows)
        .meta("model", g.model().name())
        .meta("model_path", ctx.path.display().to_string()))
}

pub fn info(g: &Graph<'_>, ctx: &Ctx) -> Result<Output, CliError> {
    let m = g.model();
    let s = g.stats();
    let row = Row::new()
        .s("path", ctx.path.display().to_string())
        .s("name", m.name())
        .s("id", m.model_id())
        // 5.0.0 is the ArchiMate 3.2 model version, not the Archi version.
        .s("version", m.version())
        .b("zipped", m.is_zipped())
        .opt("purpose", m.purpose())
        .n("elements", s.elements as i64)
        .n("relationships", s.relationships as i64)
        .n("views", s.views as i64)
        .s("checksum", m.checksum().unwrap_or_default());
    Ok(Output::one(row))
}

pub fn validate(
    m: &mut Model,
    level: &str,
    fix: bool,
    strict: bool,
    opts: &write::Opts,
) -> Result<Output, CliError> {
    let lvl = Level::parse(level).ok_or_else(|| {
        CliError::new(Code::Usage, "usage", format!("`{level}` is not a level"))
            .hint("one of: types, rules, integrity, all")
    })?;

    let mut repaired = None;
    if fix {
        write::guard_checksum(m, opts)?;
        let done = amcli_validate::fix_safe(m);
        if !opts.dry_run && done.total() > 0 {
            write::save(m)?;
        }
        repaired = Some(done);
    }

    let g = Graph::build(m);
    let report = amcli_validate::validate(m, &g, lvl);

    let rows: Vec<Row> = report
        .findings
        .iter()
        .map(|f| {
            Row::new()
                .s("code", f.code)
                .s("severity", f.severity.as_str())
                .s("entity", f.entity.clone())
                .s("kind", f.entity_kind)
                .n("line", f.line as i64)
                .n("column", f.column as i64)
                .s("message", f.message.clone())
                .s(
                    "fixable",
                    match f.fixability {
                        Fixability::Safe => "safe",
                        Fixability::Destructive => "destructive",
                        Fixability::Manual => "manual",
                    },
                )
                .opt("fix", f.fix.clone())
        })
        .collect();

    let mut out = Output::rows(rows)
        .meta_n("errors", report.errors() as i64)
        .meta_n("warnings", report.warnings() as i64)
        .meta_n("rules_run", report.rules_run as i64);

    if let Some(r) = repaired {
        out = out.meta_n("repaired", r.total() as i64).note(format!(
            "repaired {} orphaned object(s), {} orphaned connection(s), {} view mirror(s)",
            r.orphan_objects.len(),
            r.orphan_connections.len(),
            r.recomputed_views.len()
        ));
    }

    let failed = report.errors() > 0 || (strict && report.warnings() > 0);
    if failed {
        // The findings ARE the output. Reporting this as an error would print a
        // one-line complaint and discard the work list.
        return Ok(out
            .note(format!(
                "{} error(s), {} warning(s) — each finding carries a runnable fix",
                report.errors(),
                report.warnings()
            ))
            .exit(Code::Invalid));
    }
    Ok(out.note(format!(
        "clean: {} rule(s) run, {} warning(s)",
        report.rules_run,
        report.warnings()
    )))
}
