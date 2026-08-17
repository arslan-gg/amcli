//! Creating a model from nothing.
//!
//! Every other command needs a model to already exist, and the only way to get
//! the first one was to hand-write the XML — which is the one thing the skill
//! tells an agent never to do, for good reason. So the skeleton is written here,
//! by the tool that knows what shape it has to be.
//!
//! The bytes are parsed back before anything is written to disk. An `init` that
//! produced a file `amcli` itself could not open would be the worst possible
//! failure of this command, and checking costs one parse of nine folders.

use std::path::{Path, PathBuf};

use amcli_model::{FolderType, Model, ids};

use crate::output::{CliError, Code, Output, Row};
use crate::write::Opts;

/// The top-level folders Archi creates, in the order it writes them.
///
/// The order is not cosmetic: it is what a `.archimate` file looks like, and
/// matching it means the first diff after Archi opens and saves the model is
/// empty rather than a reshuffle.
const FOLDERS: [(&str, FolderType); 9] = [
    ("Strategy", FolderType::Strategy),
    ("Business", FolderType::Business),
    ("Application", FolderType::Application),
    ("Technology & Physical", FolderType::Technology),
    ("Motivation", FolderType::Motivation),
    ("Implementation & Migration", FolderType::ImplementationMigration),
    ("Other", FolderType::Other),
    ("Relations", FolderType::Relations),
    ("Views", FolderType::Diagrams),
];

/// `5.0.0` is the ArchiMate 3.2 *model* version, not an Archi version.
const MODEL_VERSION: &str = "5.0.0";

pub fn run(opts: &Opts, name: &str, out: Option<&Path>, force: bool) -> Result<Output, CliError> {
    let path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{}.archimate", slug(name))));

    if path.exists() && !force {
        return Err(CliError::new(
            Code::Conflict,
            "conflict",
            format!("`{}` already exists", path.display()),
        )
        .hint("pass --force to overwrite it, or -o to write somewhere else"));
    }

    let bytes = skeleton(name);

    // Parse before writing, not after: a skeleton this command cannot open is a
    // bug in this command, and it should never reach the disk.
    let model = Model::from_bytes(bytes.clone(), &path).map_err(|e| {
        CliError::new(
            Code::Failed,
            "internal",
            format!("the generated skeleton did not parse: {e}"),
        )
    })?;
    let (folders, checksum) = (model.folders().count(), model.checksum().unwrap_or_default());

    if !opts.dry_run {
        amcli_model::container::write_atomically(&path, &bytes)
            .map_err(|e| CliError::new(Code::Io, "io", e.to_string()))?;
    }

    let row = Row::new()
        .s("path", path.display().to_string())
        .s("name", name.to_string())
        .s("id", model.model_id())
        .n("folders", folders as i64)
        .n("bytes", bytes.len() as i64)
        .b("dry_run", opts.dry_run);
    let out = Output::one(row).meta("checksum", checksum);
    Ok(if opts.dry_run {
        out.note("dry run: nothing was written")
    } else {
        out.note("empty model: add elements with `amcli element add`, then a view with `amcli view auto`")
    })
}

fn skeleton(name: &str) -> Vec<u8> {
    let esc = amcli_xml::escape_attr;
    let mut s = String::with_capacity(1024);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str(&format!(
        "<archimate:model xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xmlns:archimate=\"http://www.archimatetool.com/archimate\" \
         name=\"{}\" id=\"{}\" version=\"{MODEL_VERSION}\">\n",
        esc(name),
        esc(&id(&["model", name])),
    ));
    for (folder, ty) in FOLDERS {
        s.push_str(&format!(
            "  <folder name=\"{}\" id=\"{}\" type=\"{}\"/>\n",
            esc(folder),
            esc(&id(&["folder", folder])),
            ty.as_str(),
        ));
    }
    s.push_str("</archimate:model>\n");
    s.into_bytes()
}

/// Random by default, derived when `--id-seed` is set — the same rule as every
/// other id, so `init` followed by a batch is reproducible end to end.
fn id(parts: &[&str]) -> String {
    if ids::is_seeded() { ids::derived_id(parts, 0) } else { ids::new_id() }
}

/// A filename from a model name, for when `-o` is left off.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() { "model".to_string() } else { trimmed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_sensible_filename() {
        assert_eq!(slug("Monetech Architecture"), "monetech-architecture");
        assert_eq!(slug("Technology & Physical"), "technology-physical");
        assert_eq!(slug("  "), "model");
        assert_eq!(slug("...!!!"), "model");
    }

    /// The skeleton has to be a model amcli can open, and one whose folders the
    /// write path can file new concepts into.
    #[test]
    fn the_skeleton_parses_and_has_a_home_for_every_layer() {
        let m = Model::from_bytes(skeleton("Ann & Bob's \"Model\""), "x.archimate").unwrap();
        assert_eq!(m.name(), "Ann & Bob's \"Model\"", "the name survives escaping");
        assert_eq!(m.folders().count(), 9);
        for (_, ty) in FOLDERS {
            assert!(m.top_folder(ty).is_some(), "no {ty:?} folder");
        }
        assert!(m.views().next().is_none());
        assert!(m.concepts().next().is_none());
    }
}
