//! Installing the agent skill.
//!
//! The canonical content is embedded in the binary, so the skill can never
//! describe a version of amcli that is not the one installed. `references/
//! commands.md` is generated from the command tree at install time for the same
//! reason: a hand-written command reference goes stale on the first release.
//!
//! Installation targets `~/.agents/skills/`, the documented cross-tool location
//! that Codex reads natively, and symlinks `~/.claude/skills/` at it. That
//! mirrors what `npx skills add` does, so a skill installed either way lands in
//! the same place.

use std::path::{Path, PathBuf};

use clap::{CommandFactory, Subcommand};

use crate::output::{CliError, Code, Output, Row};

/// The skill, compiled in. Editing these files and rebuilding is the only way
/// to change what gets installed.
const FILES: &[(&str, &str)] = &[
    ("SKILL.md", include_str!("../../../skill/SKILL.md")),
    ("references/types.md", include_str!("../../../skill/references/types.md")),
    ("references/batch.md", include_str!("../../../skill/references/batch.md")),
    ("agents/openai.yaml", include_str!("../../../skill/agents/openai.yaml")),
];

#[derive(Subcommand, Clone)]
pub enum SkillCmd {
    /// Write the skill where agents look for it.
    Install {
        /// Install into ./.agents/skills instead of the home directory.
        #[arg(long)]
        project: bool,
        /// Overwrite content that differs.
        #[arg(long)]
        force: bool,
        /// Copy rather than symlink, for filesystems without symlinks.
        #[arg(long)]
        copy: bool,
    },
    /// Remove the skill and the links this command created.
    Uninstall {
        #[arg(long)]
        project: bool,
    },
    /// Print the skill instead of writing it.
    Show,
    /// Where the skill would go.
    Path {
        #[arg(long)]
        project: bool,
    },
}

pub fn run(cmd: &SkillCmd) -> Result<Output, CliError> {
    match cmd {
        SkillCmd::Show => {
            print!("{}", FILES[0].1);
            Ok(Output::empty())
        }
        SkillCmd::Path { project } => {
            let root = target(*project)?;
            Ok(Output::one(
                Row::new()
                    .s("skill", root.display().to_string())
                    .s("claude_link", claude_link(*project)?.display().to_string()),
            ))
        }
        SkillCmd::Install { project, force, copy } => install(*project, *force, *copy),
        SkillCmd::Uninstall { project } => uninstall(*project),
    }
}

fn home() -> Result<PathBuf, CliError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::new(Code::Io, "io", "HOME is not set"))
}

fn target(project: bool) -> Result<PathBuf, CliError> {
    Ok(if project {
        std::env::current_dir()
            .map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?
            .join(".agents/skills/amcli")
    } else {
        home()?.join(".agents/skills/amcli")
    })
}

fn claude_link(project: bool) -> Result<PathBuf, CliError> {
    Ok(if project {
        std::env::current_dir()
            .map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?
            .join(".claude/skills/amcli")
    } else {
        home()?.join(".claude/skills/amcli")
    })
}

fn install(project: bool, force: bool, copy: bool) -> Result<Output, CliError> {
    let root = target(project)?;
    let io = |e: std::io::Error, p: &Path| {
        CliError::new(Code::Io, "io", format!("{}: {e}", p.display()))
    };

    // Refuse to clobber content someone may have edited, unless told.
    if root.exists() && !force {
        let differs = FILES.iter().any(|(name, body)| {
            std::fs::read_to_string(root.join(name)).map(|c| c != *body).unwrap_or(true)
        });
        if differs {
            return Err(CliError::new(
                Code::Conflict,
                "conflict",
                format!("{} already holds different content", root.display()),
            )
            .hint("pass --force to overwrite"));
        }
    }

    let mut written = Vec::new();
    for (name, body) in FILES {
        let path = root.join(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(e, dir))?;
        }
        std::fs::write(&path, body).map_err(|e| io(e, &path))?;
        written.push(name.to_string());
    }

    // Generated, not written by hand, so it cannot drift from the binary.
    let commands = root.join("references/commands.md");
    std::fs::write(&commands, command_reference()).map_err(|e| io(e, &commands))?;
    written.push("references/commands.md".to_string());

    // One link, for Claude Code. Codex reads ~/.agents/skills natively, so a
    // second copy under ~/.codex would only be another thing to keep in sync.
    let link = claude_link(project)?;
    let mut linked = false;
    if let Some(parent) = link.parent()
        && (parent.exists() || project)
    {
        std::fs::create_dir_all(parent).map_err(|e| io(e, parent))?;
        if link.exists() || link.symlink_metadata().is_ok() {
            let _ = std::fs::remove_file(&link);
            let _ = std::fs::remove_dir_all(&link);
        }
        linked = if copy {
            copy_tree(&root, &link).is_ok()
        } else {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&root, &link).is_ok()
            }
            #[cfg(not(unix))]
            {
                copy_tree(&root, &link).is_ok()
            }
        };
    }

    let mut out = Output::one(
        Row::new()
            .s("skill", root.display().to_string())
            .n("files", written.len() as i64)
            .b("claude_link", linked),
    )
    .note(format!("installed {} files into {}", written.len(), root.display()));

    if linked {
        out = out.note(format!("linked {}", link.display()));
    } else {
        out = out.note(format!(
            "no Claude Code directory at {}; Codex and other tools read {} directly",
            link.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            root.display()
        ));
    }
    Ok(out.note("start a new agent session to pick it up"))
}

fn uninstall(project: bool) -> Result<Output, CliError> {
    let root = target(project)?;
    let link = claude_link(project)?;
    let mut removed = Vec::new();

    // Only the link is removed, never whatever it pointed at if it was not ours.
    if link.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&link);
        removed.push(link.display().to_string());
    }
    if root.exists() {
        std::fs::remove_dir_all(&root)
            .map_err(|e| CliError::new(Code::Io, "io", format!("{}: {e}", root.display())))?;
        removed.push(root.display().to_string());
    }
    if removed.is_empty() {
        return Ok(Output::empty().note("nothing to remove"));
    }
    Ok(Output::rows(removed.into_iter().map(|p| Row::new().s("removed", p)).collect()))
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// Render the whole command tree out of clap, so the reference is exactly what
/// the binary does.
fn command_reference() -> String {
    let mut out = String::from(
        "# amcli commands\n\n\
         Generated from the binary by `amcli skill install`. Do not edit — it is\n\
         overwritten on every install, which is what keeps it from going stale.\n\n",
    );
    let cmd = crate::Cli::command();
    out.push_str("## Global flags\n\n```\n");
    for a in cmd.get_arguments() {
        let long = a.get_long().map(|l| format!("--{l}")).unwrap_or_default();
        let short = a.get_short().map(|s| format!("-{s}, ")).unwrap_or_default();
        let help = a.get_help().map(|h| h.to_string()).unwrap_or_default();
        out.push_str(&format!("  {short}{long:<24} {}\n", help.replace('\n', " ")));
    }
    out.push_str("```\n\n## Commands\n\n");
    for sub in cmd.get_subcommands() {
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        out.push_str(&format!("### `{}`\n\n{}\n\n", sub.get_name(), about.replace('\n', " ")));
        let nested: Vec<_> = sub.get_subcommands().collect();
        if !nested.is_empty() {
            out.push_str("```\n");
            for n in nested {
                let a = n.get_about().map(|a| a.to_string()).unwrap_or_default();
                out.push_str(&format!(
                    "  amcli {} {:<16} {}\n",
                    sub.get_name(),
                    n.get_name(),
                    a.replace('\n', " ")
                ));
            }
            out.push_str("```\n\n");
        }
    }
    out
}
