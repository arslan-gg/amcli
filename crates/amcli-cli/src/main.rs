//! `amcli` — a CLI over ArchiMate models.

use std::io::Write;
use std::path::{Path, PathBuf};

use amcli_graph::Graph;
use amcli_model::Model;
use clap::{Parser, Subcommand};

mod apply;
mod export;
mod output;
mod read;
mod skill;
mod view;
mod write;

use output::{CliError, Code, Format, Output, Printer};

#[derive(Parser)]
#[command(
    name = "amcli",
    version,
    about = "A CLI over ArchiMate models. No Archi, no JVM, no daemon.",
    after_help = "\
Reads are bare verbs; anything that changes the model is noun-verb, which makes
a write one word longer than a read on purpose.

Flags mean one thing everywhere:
  -t concept type   -r relationship type   -f folder      -D direction
  -n depth          -l limit               -m model       -F format

Subjects are positional, never flags:
  amcli element  add ApplicationComponent \"Refund Service\"
  amcli relation add Serving \"Refund Service\" \"Checkout Service\"

Exit codes, so you can branch without parsing prose:
  0 ok   2 usage   3 not found   4 ambiguous   5 invalid   6 conflict
  7 io   8 unsupported

ArchiMate(R) is a registered trademark of The Open Group. This project is
independent and is not affiliated with or endorsed by them or by Archi."
)]
pub struct Cli {
    /// Model file. Defaults to $AMCLI_MODEL, else the nearest *.archimate
    /// walking up from the working directory.
    #[arg(short = 'm', long, global = true)]
    model: Option<PathBuf>,

    /// Output format: text (tab-separated records), json, jsonl.
    #[arg(short = 'F', long, global = true, default_value = "text")]
    format: String,

    /// Drop the envelope and the notes; emit data only.
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Keep only these fields, or drop them with a leading `-`.
    #[arg(long, global = true, value_delimiter = ',', allow_hyphen_values = true)]
    fields: Option<Vec<String>>,

    /// Print how many results there would be, and nothing else.
    #[arg(long, global = true)]
    count: bool,

    /// Maximum records to return. 0 means no limit.
    #[arg(short = 'l', long, global = true, default_value_t = 50)]
    limit: usize,

    /// Report what a write would do and change nothing.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Refuse the write if the file has changed since this checksum was read.
    #[arg(long, global = true)]
    expect_checksum: Option<String>,

    /// Skip the confirmation on a cascading delete.
    #[arg(short = 'y', long, global = true)]
    yes: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// A concept with its inbound and outbound relationships.
    Get {
        /// id:… , a name, Type:Name, or a glob.
        selector: String,
        /// Show documentation in full instead of truncating it.
        #[arg(long)]
        full: bool,
    },
    /// Substring search over names, then documentation, then property values.
    Search {
        query: String,
        #[arg(short = 't', long)]
        r#type: Option<String>,
    },
    /// Enumerate concepts.
    List {
        #[arg(short = 't', long)]
        r#type: Option<String>,
        /// Folder path prefix, e.g. /Application.
        #[arg(short = 'f', long)]
        folder: Option<String>,
    },
    /// Filter expression, e.g. 'type=ApplicationComponent and name~pay'.
    Query { expr: String },
    /// One hop out from a concept.
    Neighbors {
        selector: String,
        #[arg(short = 'D', long, default_value = "both")]
        direction: String,
        #[arg(short = 'r', long)]
        rel: Option<String>,
        /// Keep only concepts of this type.
        #[arg(short = 't', long)]
        r#type: Option<String>,
    },
    /// The neighbourhood within N hops, as an induced subgraph.
    Trace {
        selector: String,
        #[arg(short = 'D', long, default_value = "both")]
        direction: String,
        #[arg(short = 'n', long, default_value_t = 2)]
        depth: u32,
        #[arg(short = 'r', long)]
        rel: Option<String>,
        /// Show only concepts of this type. The walk still crosses every type,
        /// so a multi-hop query stays useful.
        #[arg(short = 't', long)]
        r#type: Option<String>,
    },
    /// How two concepts are connected.
    Path {
        from: String,
        to: String,
        #[arg(short = 'D', long, default_value = "out")]
        direction: String,
        /// Every simple path, not just the shortest.
        #[arg(long)]
        all: bool,
        #[arg(short = 'n', long, default_value_t = 6)]
        depth: u32,
    },
    /// What is reachable, and the relationship that pulled each thing in.
    Impact {
        selector: String,
        #[arg(short = 'D', long, default_value = "in")]
        direction: String,
        #[arg(short = 'n', long)]
        depth: Option<u32>,
        /// Report only concepts of this type. The walk still crosses every
        /// type, so asking for components two hops away still finds them.
        #[arg(short = 't', long)]
        r#type: Option<String>,
    },
    /// Composition and aggregation upwards.
    Ancestors { selector: String },
    /// Composition and aggregation downwards.
    Descendants { selector: String },
    /// Dependency cycles.
    Cycles {
        #[arg(short = 'r', long)]
        rel: Option<String>,
    },
    /// Counts by type, layer and folder, plus orphans.
    Stats,
    /// Views: list, create, populate, lay out and draw.
    #[command(subcommand)]
    View(view::ViewCmd),
    /// Check the model.
    Validate {
        /// How far to check. Each level includes the ones before it:
        /// types, rules, integrity, all.
        #[arg(long, default_value = "all")]
        level: String,
        /// Apply the repairs that are derived rather than chosen.
        #[arg(long)]
        fix: bool,
        /// Treat warnings as failure.
        #[arg(long)]
        strict: bool,
    },
    /// Create, change and delete elements.
    #[command(subcommand)]
    Element(write::ElementCmd),
    /// Create, change and delete relationships.
    #[command(subcommand)]
    Relation(write::RelationCmd),
    /// Folders.
    #[command(subcommand)]
    Folder(write::FolderCmd),
    /// Properties on a concept.
    #[command(subcommand)]
    Prop(write::PropCmd),
    /// Apply a batch of edits atomically: all of them land, or none do.
    Apply {
        /// A JSONL file, or `-` for stdin.
        #[arg(default_value = "-")]
        file: String,
    },
    /// Export the whole model.
    Export {
        /// csv | json | mermaid | dot. Named `to` rather than `format`
        /// because clap merges a same-named subcommand field into the global
        /// -F, which controls how amcli reports rather than what it writes.
        to: String,
        #[arg(short = 'o', long)]
        out: Option<String>,
    },
    /// Install the agent skill.
    #[command(subcommand)]
    Skill(skill::SkillCmd),
    /// Model-level facts.
    Info,
}

/// Parse, but turn "no such subcommand" into a version-skew hint.
///
/// The skill is installed from the default branch by `npx skills add` while
/// the binary comes from the newest release, so the skill can legitimately be
/// *newer* than the binary and document a command it does not have. clap
/// answers that with "unrecognized subcommand" and exits before any of our
/// code runs, which reads like a broken skill rather than an old binary.
///
/// This is deliberately the only place the two versions are reconciled: a
/// `metadata:` field would be inert (the skills CLI reads only `name` and
/// `description`), and asking an agent to compare version strings by eye is
/// the kind of step that fails quietly.
fn parse_or_hint() -> Cli {
    use clap::error::ErrorKind;
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let skew =
                matches!(e.kind(), ErrorKind::InvalidSubcommand | ErrorKind::UnknownArgument);
            let _ = e.print();
            if skew {
                eprintln!(
                    "\nIf a skill or documentation said this exists, this amcli is older \
                     than that document.\nUpgrade it:  sh ~/.agents/skills/amcli/scripts/install.sh"
                );
            }
            std::process::exit(if e.use_stderr() { Code::Usage as i32 } else { 0 });
        }
    }
}

fn main() {
    let cli = parse_or_hint();
    let Some(format) = Format::parse(&cli.format) else {
        eprintln!("error: unknown format `{}`; expected text, json or jsonl", cli.format);
        std::process::exit(Code::Usage as i32);
    };
    let printer =
        Printer { format, quiet: cli.quiet, fields: cli.fields.clone(), count_only: cli.count };

    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();

    match run(&cli) {
        Ok(out) => {
            let verdict = out.exit;
            printer.print(out, &mut stdout, &mut stderr);
            let _ = stdout.flush();
            if let Some(code) = verdict {
                std::process::exit(code as i32);
            }
        }
        Err(e) => {
            let code = e.code;
            printer.print_error(&e, &mut stdout, &mut stderr);
            let _ = stdout.flush();
            std::process::exit(code as i32);
        }
    }
}

fn run(cli: &Cli) -> Result<Output, CliError> {
    // The skill is about this machine, not about any model, so it runs before
    // amcli goes looking for one.
    if let Command::Skill(c) = &cli.command {
        return skill::run(c);
    }

    let path = find_model(cli.model.as_deref())?;
    let mut model = Model::open(&path).map_err(|e| {
        CliError::new(Code::Io, "io", e.to_string())
            .hint("check the path, or pass -m to point at the model")
    })?;

    let ctx = read::Ctx { limit: cli.limit, path: path.clone() };

    // Reads borrow the model; writes need it mutably, so they are dispatched
    // separately rather than threading a borrow through both.
    match &cli.command {
        Command::Element(_) | Command::Relation(_) | Command::Folder(_) | Command::Prop(_) => {
            return write::run(cli_write_opts(cli), &mut model, &cli.command_write());
        }
        Command::View(c) => return view::run(&write_opts(cli), &mut model, c),
        Command::Apply { file } => {
            return apply::run(&write_opts(cli), &mut model, Some(file.as_str()));
        }
        Command::Validate { level, fix, strict } => {
            return read::validate(&mut model, level, *fix, *strict, &write_opts(cli));
        }
        _ => {}
    }

    let graph = Graph::build(&model);
    match &cli.command {
        Command::Get { selector, full } => read::get(&graph, &ctx, selector, *full),
        Command::Search { query, r#type } => read::search(&graph, &ctx, query, r#type.as_deref()),
        Command::List { r#type, folder } => {
            read::list(&graph, &ctx, r#type.as_deref(), folder.as_deref())
        }
        Command::Query { expr } => read::query(&graph, &ctx, expr),
        Command::Neighbors { selector, direction, rel, r#type } => {
            read::neighbors(&graph, &ctx, selector, direction, rel.as_deref(), r#type.as_deref())
        }
        Command::Trace { selector, direction, depth, rel, r#type } => read::trace(
            &graph,
            &ctx,
            selector,
            direction,
            *depth,
            rel.as_deref(),
            r#type.as_deref(),
        ),
        Command::Path { from, to, direction, all, depth } => {
            read::path(&graph, &ctx, from, to, direction, *all, *depth)
        }
        Command::Impact { selector, direction, depth, r#type } => {
            read::impact(&graph, &ctx, selector, direction, *depth, r#type.as_deref())
        }
        Command::Ancestors { selector } => read::containment(&graph, &ctx, selector, true),
        Command::Descendants { selector } => read::containment(&graph, &ctx, selector, false),
        Command::Cycles { rel } => read::cycles(&graph, &ctx, rel.as_deref()),
        Command::Stats => read::stats(&graph, &ctx),
        Command::Info => read::info(&graph, &ctx),
        Command::Export { to, out } => export::run(&graph, to, out.as_deref()),
        Command::Element(_)
        | Command::Relation(_)
        | Command::Folder(_)
        | Command::Prop(_)
        | Command::View(_)
        | Command::Apply { .. }
        | Command::Skill(_)
        | Command::Validate { .. } => unreachable!("dispatched above"),
    }
}

impl Cli {
    fn command_write(&self) -> write::WriteCmd {
        match &self.command {
            Command::Element(c) => write::WriteCmd::Element(c.clone()),
            Command::Relation(c) => write::WriteCmd::Relation(c.clone()),
            Command::Folder(c) => write::WriteCmd::Folder(c.clone()),
            Command::Prop(c) => write::WriteCmd::Prop(c.clone()),
            _ => unreachable!("only write commands reach here"),
        }
    }
}

fn write_opts(cli: &Cli) -> write::Opts {
    write::Opts { dry_run: cli.dry_run, yes: cli.yes, expect_checksum: cli.expect_checksum.clone() }
}

fn cli_write_opts(cli: &Cli) -> write::Opts {
    write_opts(cli)
}

/// Explicit flag, then the environment, then the nearest model walking up. An
/// ambiguous directory is reported rather than guessed at.
fn find_model(explicit: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("AMCLI_MODEL") {
        return Ok(PathBuf::from(p));
    }

    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let mut found: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("archimate") {
                    found.push(p);
                }
            }
        }
        found.sort();
        match found.len() {
            1 => return Ok(found.remove(0)),
            0 => {}
            _ => {
                return Err(CliError::new(
                    Code::Ambiguous,
                    "ambiguous",
                    format!("{} models in {}", found.len(), dir.display()),
                )
                .hint("pass -m to choose one")
                .rows(
                    found
                        .iter()
                        .map(|p| output::Row::new().s("path", p.display().to_string()))
                        .collect(),
                ));
            }
        }
        if !dir.pop() {
            break;
        }
    }
    Err(CliError::new(Code::NotFound, "not_found", "no *.archimate file found")
        .hint("pass -m PATH, set AMCLI_MODEL, or run from a directory containing a model"))
}
