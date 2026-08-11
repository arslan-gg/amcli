//! View commands: listing, authoring and rendering.

use amcli_graph::{Dir, EdgeFilter, Graph, Resolution, Selector};
use amcli_model::{ConceptId, ConceptKind, Model, ViewId, viewpoints};
use amcli_render::Options;
use amcli_view::geometry::Rect;
use amcli_view::geometry::bendpoint_for;
use amcli_view::layout::{Algorithm, Item, free_slot, place};
use clap::Subcommand;

use crate::output::{CliError, Code, Output, Row};
use crate::write::Opts;

#[derive(Subcommand, Clone)]
pub enum ViewCmd {
    /// List views.
    List,
    /// Create an empty view.
    Create {
        name: String,
        /// One of the 25 ArchiMate viewpoint ids, e.g. layered.
        #[arg(long)]
        viewpoint: Option<String>,
    },
    /// Put a concept on a view.
    Add {
        view: String,
        selector: String,
        #[arg(long)]
        x: Option<i32>,
        #[arg(long)]
        y: Option<i32>,
    },
    /// Build a view from a concept and its neighbourhood, laid out and wired up.
    Auto {
        name: String,
        /// The concept to start from.
        #[arg(long)]
        from: String,
        #[arg(short = 'n', long, default_value_t = 2)]
        depth: u32,
        #[arg(short = 'D', long, default_value = "both")]
        direction: String,
        /// sugiyama (the default) | grid
        #[arg(long, default_value = "layered")]
        layout: String,
        #[arg(long)]
        viewpoint: Option<String>,
    },
    /// Re-place the objects on a view.
    Layout {
        view: String,
        #[arg(long, default_value = "layered")]
        algorithm: String,
        /// Move everything, not just objects that have never been placed.
        #[arg(long)]
        relayout_all: bool,
    },
    /// Draw a view.
    Render {
        view: String,
        /// svg | json. This is `--as`, not the global `-F`: one controls what
        /// is drawn, the other how amcli reports. The field is named
        /// `draw_as` because a second `format` would be merged into the
        /// global one by clap.
        #[arg(long = "as", default_value = "svg")]
        draw_as: String,
        /// Write here instead of to stdout.
        #[arg(short = 'o', long)]
        out: Option<String>,
        #[arg(long, default_value_t = 10)]
        margin: i32,
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
    },
}

pub fn run(opts: &Opts, m: &mut Model, cmd: &ViewCmd) -> Result<Output, CliError> {
    match cmd {
        ViewCmd::List => list(m),
        ViewCmd::Create { name, viewpoint } => create(opts, m, name, viewpoint.as_deref()),
        ViewCmd::Add { view, selector, x, y } => add(opts, m, view, selector, *x, *y),
        ViewCmd::Auto { name, from, depth, direction, layout, viewpoint } => {
            auto(opts, m, name, from, *depth, direction, layout, viewpoint.as_deref())
        }
        ViewCmd::Layout { view, algorithm, relayout_all } => {
            relayout(opts, m, view, algorithm, *relayout_all)
        }
        ViewCmd::Render { view, draw_as, out, margin, scale } => {
            render(m, view, draw_as, out.as_deref(), *margin, *scale)
        }
    }
}

fn find_view(m: &Model, sel: &str) -> Result<ViewId, CliError> {
    if let Some(id) = sel.strip_prefix("id:").and_then(|i| m.view_by_id(i)) {
        return Ok(id);
    }
    let matches: Vec<(ViewId, String)> = m
        .views_with_ids()
        .filter(|(_, v)| v.name == sel || v.id == sel)
        .map(|(i, v)| (i, v.name.clone()))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].0),
        0 => Err(CliError::new(Code::NotFound, "not_found", format!("no view called `{sel}`"))
            .hint("run `amcli view list`")
            .rows(
                m.views()
                    .map(|v| Row::new().s("id", v.id.clone()).s("name", v.name.clone()))
                    .collect(),
            )),
        _ => Err(CliError::new(
            Code::Ambiguous,
            "ambiguous",
            format!("{} views called `{sel}`", matches.len()),
        )
        .hint("use id:…")
        .rows(
            matches
                .iter()
                .map(|(i, n)| {
                    Row::new().s("selector", format!("id:{}", m.view(*i).id)).s("name", n.clone())
                })
                .collect(),
        )),
    }
}

fn list(m: &Model) -> Result<Output, CliError> {
    let rows: Vec<Row> = m
        .views()
        .map(|v| {
            Row::new()
                .s("id", v.id.clone())
                .s("name", v.name.clone())
                .s("kind", if v.is_sketch { "sketch" } else { "archimate" })
                .s("viewpoint", v.viewpoint.clone())
        })
        .collect();
    let total = rows.len();
    Ok(Output::rows(rows).meta_n("total", total as i64))
}

fn check_viewpoint(vp: Option<&str>) -> Result<(), CliError> {
    let Some(v) = vp.filter(|v| !v.is_empty()) else { return Ok(()) };
    if viewpoints::by_id(v).is_some() {
        return Ok(());
    }
    Err(CliError::new(Code::Usage, "usage", format!("`{v}` is not a viewpoint id")).hint(format!(
        "one of: {}",
        viewpoints::VIEWPOINTS.iter().map(|v| v.id).collect::<Vec<_>>().join(", ")
    )))
}

fn create(opts: &Opts, m: &mut Model, name: &str, vp: Option<&str>) -> Result<Output, CliError> {
    check_viewpoint(vp)?;
    let v =
        m.add_view(name, vp).map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
    let row = Row::new()
        .s("id", m.view(v).id.clone())
        .s("name", name.to_string())
        .b("dry_run", opts.dry_run);
    finish(opts, m, row)
}

fn finish(opts: &Opts, m: &Model, row: Row) -> Result<Output, CliError> {
    if !opts.dry_run {
        crate::write::save(m)?;
    }
    let out = Output::one(row);
    Ok(if opts.dry_run { out.note("dry run: nothing was written") } else { out })
}

fn resolve(m: &Model, sel: &str) -> Result<ConceptId, CliError> {
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

/// Warn rather than refuse when a concept is outside the view's viewpoint.
///
/// Archi ghosts non-conforming elements instead of blocking, and an agent
/// mid-task should not be stopped by a modelling convention.
fn viewpoint_note(m: &Model, view: ViewId, c: ConceptId) -> Option<String> {
    let vp = &m.view(view).viewpoint;
    if vp.is_empty() {
        return None;
    }
    let ConceptKind::Element(e) = &m.concept(c).kind else { return None };
    (!viewpoints::allows(vp, *e))
        .then(|| format!("viewpoint `{vp}` does not cover {}; added anyway", e.info().short))
}

fn add(
    opts: &Opts,
    m: &mut Model,
    view: &str,
    sel: &str,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<Output, CliError> {
    let v = find_view(m, view)?;
    let c = resolve(m, sel)?;
    let note = viewpoint_note(m, v, c);

    let (w, h) = match &m.concept(c).kind {
        ConceptKind::Element(e) => e.info().default_wh,
        _ => (120, 55),
    };
    // Placed clear of everything already there, so adding one object never
    // disturbs the rest of the diagram.
    let taken: Vec<Rect> = amcli_view::compile(m, v).nodes.iter().map(|n| n.abs).collect();
    let slot = match (x, y) {
        (Some(x), Some(y)) => Rect { x, y, w, h },
        _ => free_slot(&taken, w, h),
    };

    let id = m
        .add_view_object(v, c, slot.x, slot.y, slot.w, slot.h)
        .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;

    let row = Row::new()
        .s("object", id)
        .s("concept", m.concept(c).id.clone())
        .n("x", slot.x as i64)
        .n("y", slot.y as i64)
        .b("dry_run", opts.dry_run);
    let out = finish(opts, m, row)?;
    Ok(match note {
        Some(n) => out.note(n),
        None => out,
    })
}

#[allow(clippy::too_many_arguments)] // one parameter per CLI flag; grouping them would only hide the surface
fn auto(
    opts: &Opts,
    m: &mut Model,
    name: &str,
    from: &str,
    depth: u32,
    dir: &str,
    algorithm: &str,
    vp: Option<&str>,
) -> Result<Output, CliError> {
    check_viewpoint(vp)?;
    let algo = Algorithm::parse(algorithm).ok_or_else(|| {
        CliError::new(Code::Usage, "usage", format!("`{algorithm}` is not a layout"))
            .hint("one of: sugiyama, grid")
    })?;
    let dir = Dir::parse(dir).ok_or_else(|| {
        CliError::new(Code::Usage, "usage", format!("`{dir}` is not a direction"))
    })?;

    // Gather first, mutate second: the graph borrows the model.
    let (items, edges, concepts, rels) = {
        let g = Graph::build(m);
        let root = match Selector::parse(from).resolve_one(&g) {
            Resolution::One(c) => c,
            _ => {
                return Err(CliError::new(
                    Code::NotFound,
                    "not_found",
                    format!("nothing matches `{from}`"),
                ));
            }
        };
        let sub = g.k_hop(&[root], depth, dir, &EdgeFilter::default(), 500);
        let concepts: Vec<ConceptId> = sub.nodes.iter().map(|(c, _)| *c).collect();

        let items: Vec<Item> = concepts
            .iter()
            .map(|c| {
                let concept = m.concept(*c);
                let (w, h) = match &concept.kind {
                    ConceptKind::Element(e) => e.info().default_wh,
                    _ => (120, 55),
                };
                Item { id: concept.id.clone(), name: concept.name.clone(), w, h }
            })
            .collect();

        let index = |c: ConceptId| concepts.iter().position(|x| *x == c);
        let mut edges = Vec::new();
        let mut rels = Vec::new();
        for e in &sub.edges {
            if let Some((s, t)) = g.ends(*e)
                && let (Some(a), Some(b)) = (index(s), index(t))
            {
                edges.push((a, b));
                rels.push((*e, a, b));
            }
        }
        (items, edges, concepts, rels)
    };

    if concepts.is_empty() {
        return Err(CliError::new(Code::NotFound, "not_found", "nothing to put on the view"));
    }

    let placed = place(&items, &edges, algo);
    let v =
        m.add_view(name, vp).map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;

    let mut object_ids = Vec::with_capacity(concepts.len());
    for (c, r) in concepts.iter().zip(placed.rects.iter()) {
        let id = m
            .add_view_object(v, *c, r.x, r.y, r.w, r.h)
            .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
        object_ids.push(id);
    }

    let mut drawn = 0;
    let mut routed = 0;
    for (edge_index, (rel, a, b)) in rels.into_iter().enumerate() {
        // Waypoints the layout produced are stored as bendpoints, so the
        // routing lives in the file and Archi draws the same line we do.
        let bends: Vec<(i32, i32, i32, i32)> = placed
            .routes
            .get(&edge_index)
            .map(|pts| {
                pts.iter()
                    .map(|p| {
                        let bp = bendpoint_for(placed.rects[a], placed.rects[b], *p);
                        (bp.start_x, bp.start_y, bp.end_x, bp.end_y)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !bends.is_empty() {
            routed += 1;
        }
        if m.add_view_connection(v, rel, &object_ids[a], &object_ids[b], &bends).is_ok() {
            drawn += 1;
        }
    }

    let row = Row::new()
        .s("id", m.view(v).id.clone())
        .s("name", name.to_string())
        .n("objects", object_ids.len() as i64)
        .n("connections", drawn)
        .n("routed", routed)
        .b("dry_run", opts.dry_run);
    finish(opts, m, row)
}

fn relayout(
    opts: &Opts,
    m: &mut Model,
    view: &str,
    algorithm: &str,
    all: bool,
) -> Result<Output, CliError> {
    let v = find_view(m, view)?;
    let algo = Algorithm::parse(algorithm).ok_or_else(|| {
        CliError::new(Code::Usage, "usage", format!("`{algorithm}` is not a layout"))
    })?;

    // Only objects that have never been placed move, unless told otherwise.
    // Reflowing everything by default is how one added element turns into a
    // four-hundred-line diff.
    let scene = amcli_view::compile(m, v);
    let movable: Vec<(String, Rect)> = scene
        .nodes
        .iter()
        .filter(|n| all || (n.abs.x == 0 && n.abs.y == 0))
        .map(|n| (n.id.clone(), n.abs))
        .collect();

    if movable.is_empty() {
        return Ok(Output::empty().note("nothing to move; pass --relayout-all to reflow the view"));
    }

    let items: Vec<Item> = movable
        .iter()
        .enumerate()
        .map(|(i, (id, r))| {
            let node = &scene.nodes[i];
            Item { id: id.clone(), name: node.label.clone(), w: r.w, h: r.h }
        })
        .collect();
    let placed = place(&items, &[], algo);

    for ((id, _), r) in movable.iter().zip(placed.rects.iter()) {
        m.set_view_object_bounds(v, id, r.x, r.y)
            .map_err(|e| CliError::new(Code::Invalid, "invalid", e.to_string()))?;
    }

    let row = Row::new()
        .s("view", m.view(v).id.clone())
        .n("moved", movable.len() as i64)
        .b("dry_run", opts.dry_run);
    finish(opts, m, row)
}

fn render(
    m: &Model,
    view: &str,
    format: &str,
    out_path: Option<&str>,
    margin: i32,
    scale: f64,
) -> Result<Output, CliError> {
    let v = find_view(m, view)?;
    let scene = amcli_view::compile(m, v);

    let body = match format {
        "svg" => amcli_render::svg(&scene, &Options { margin, scale, ..Default::default() }),
        "json" => amcli_render::scene_json(&scene),
        other => {
            return Err(CliError::new(
                Code::Unsupported,
                "unsupported",
                format!("`{other}` is not a render format"),
            )
            .hint(
                "svg or json. For PNG: render to SVG and convert, e.g. \
                 `amcli view render V -o v.svg && rsvg-convert -o v.png v.svg`",
            ));
        }
    };

    match out_path {
        Some(p) => {
            std::fs::write(p, &body)
                .map_err(|e| CliError::new(Code::Io, "io", format!("{p}: {e}")))?;
            let mut o = Output::one(
                Row::new()
                    .s("path", p.to_string())
                    .n("bytes", body.len() as i64)
                    .n("nodes", scene.nodes.len() as i64)
                    .n("edges", scene.edges.len() as i64),
            );
            for w in &scene.warnings {
                o = o.note(w.clone());
            }
            Ok(o)
        }
        None => {
            // The drawing itself is the output, so it goes to stdout raw.
            print!("{body}");
            let mut o = Output::empty();
            for w in &scene.warnings {
                o = o.note(w.clone());
            }
            Ok(o)
        }
    }
}
