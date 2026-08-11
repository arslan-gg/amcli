//! Build tasks.
//!
//! `codegen` turns the vendored Archi assets into Rust tables; `verify` checks
//! the committed output still matches, which is what stops an upstream Archi
//! update from drifting in unnoticed.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

mod types;
use types::{
    ELEMENTS, FEATURE_NAME_OVERRIDES, FolderType, Layer, XSI_NAME_OVERRIDES, default_size,
};

const GENERATED_DIR: &str = "crates/amcli-model/src/generated";

fn main() -> Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "codegen" => codegen(false),
        "verify" => codegen(true),
        _ => {
            eprintln!("usage: cargo xtask <codegen|verify>");
            eprintln!("  codegen  regenerate {GENERATED_DIR} from assets/archi");
            eprintln!("  verify   fail if the committed output is stale");
            std::process::exit(2);
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the repo").to_path_buf()
}

fn codegen(verify_only: bool) -> Result<()> {
    let root = repo_root();
    let assets = root.join("assets/archi");

    let keys = parse_keys(&assets.join("relationships-keys.xml"))?;
    let (concepts, cells) = parse_matrix(&assets.join("relationships.xml"))?;
    let viewpoints = parse_viewpoints(&assets.join("viewpoints.xml"))?;

    cross_check(&concepts)?;

    // Formatting here, rather than leaving it to `cargo fmt`, is what keeps
    // `verify` and `fmt --check` from contradicting each other in CI: the
    // committed output is by construction what both of them expect.
    let files = [
        ("folders.rs", rustfmt(render_folders()?)?),
        ("types.rs", rustfmt(render_types(&concepts, &keys)?)?),
        ("matrix.rs", rustfmt(render_matrix(&concepts, &cells, &keys)?)?),
        ("viewpoints.rs", rustfmt(render_viewpoints(&viewpoints)?)?),
    ];

    let dir = root.join(GENERATED_DIR);
    if !verify_only {
        std::fs::create_dir_all(&dir)?;
    }
    let mut stale = Vec::new();
    for (name, body) in files {
        let path = dir.join(name);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == body {
            continue;
        }
        if verify_only {
            stale.push(name);
        } else {
            std::fs::write(&path, &body).with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }

    if !stale.is_empty() {
        bail!(
            "generated tables are stale: {}\n\
             The vendored assets in assets/archi changed without the generated code being \
             refreshed. Run `cargo xtask codegen` and review the diff — an upstream Archi \
             update should be a visible change, not a silent one.",
            stale.join(", ")
        );
    }
    if verify_only {
        println!("generated tables are up to date");
    }
    Ok(())
}

/// Every element name we claim exists must be a concept the matrix knows about,
/// and vice versa. This is what keeps the hand-written layer table honest.
fn cross_check(concepts: &[String]) -> Result<()> {
    let ours: Vec<&str> = ELEMENTS.iter().flat_map(|(_, names)| names.iter().copied()).collect();

    let mut missing = Vec::new();
    for c in concepts {
        // `Relationship` is a pseudo-concept: the matrix uses it for
        // relationships that target other relationships.
        if c == "Relationship" {
            continue;
        }
        if !ours.contains(&c.as_str()) {
            missing.push(c.clone());
        }
    }
    let mut extra = Vec::new();
    for o in &ours {
        if !concepts.iter().any(|c| c == o) {
            extra.push(o.to_string());
        }
    }
    if !missing.is_empty() || !extra.is_empty() {
        bail!(
            "xtask/src/types.rs disagrees with the vendored relationships.xml\n  \
             in the matrix but not in our table: {missing:?}\n  \
             in our table but not in the matrix: {extra:?}"
        );
    }
    let dupes = {
        let mut sorted = ours.clone();
        sorted.sort_unstable();
        sorted.windows(2).filter(|w| w[0] == w[1]).map(|w| w[0]).collect::<Vec<_>>()
    };
    if !dupes.is_empty() {
        bail!("duplicate element names in xtask/src/types.rs: {dupes:?}");
    }
    Ok(())
}

// ---- parsing the vendored assets -----------------------------------------

/// Letter -> relationship type name, e.g. `a` -> `AccessRelationship`.
fn parse_keys(path: &Path) -> Result<Vec<(char, String)>> {
    let src = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for cap in each_tag(&src, "key") {
        let (Some(ch), Some(rel)) = (attr(&cap, "char"), attr(&cap, "relationship")) else {
            continue;
        };
        let ch = ch.chars().next().context("empty key letter")?;
        out.push((ch, rel));
    }
    if out.len() != 11 {
        bail!("expected 11 relationship keys in {}, found {}", path.display(), out.len());
    }
    Ok(out)
}

/// One `<target>` entry: the source concept's index, the target concept name,
/// and the set of key letters permitted between them.
type MatrixCell = (usize, String, String);

/// Source concepts in file order, plus every cell.
fn parse_matrix(path: &Path) -> Result<(Vec<String>, Vec<MatrixCell>)> {
    let src = std::fs::read_to_string(path)?;
    let mut concepts = Vec::new();
    let mut cells = Vec::new();
    let mut cur: Option<usize> = None;

    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("<source ") {
            if let Some(c) = attr(t, "concept") {
                concepts.push(c);
                cur = Some(concepts.len() - 1);
            }
        } else if t.starts_with("<target ") {
            let (Some(c), Some(rels)) = (attr(t, "concept"), attr(t, "relations")) else {
                continue;
            };
            let Some(idx) = cur else { bail!("<target> outside a <source> in {}", path.display()) };
            cells.push((idx, c, rels));
        }
    }
    if concepts.len() != 62 {
        bail!("expected 62 source concepts in {}, found {}", path.display(), concepts.len());
    }
    Ok((concepts, cells))
}

struct ViewpointDef {
    id: String,
    name: String,
    concepts: Vec<String>,
}

fn parse_viewpoints(path: &Path) -> Result<Vec<ViewpointDef>> {
    let src = std::fs::read_to_string(path)?;
    let mut out: Vec<ViewpointDef> = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("<viewpoint ") {
            let id = attr(t, "id").context("viewpoint without an id")?;
            out.push(ViewpointDef { id, name: String::new(), concepts: Vec::new() });
        } else if t.starts_with("<name") {
            if let Some(v) = out.last_mut() {
                v.name = between(t, ">", "</name>").unwrap_or_default();
            }
        } else if t.starts_with("<concept>")
            && let Some(v) = out.last_mut()
            && let Some(c) = between(t, "<concept>", "</concept>")
        {
            v.concepts.push(c);
        }
    }
    if out.len() != 25 {
        bail!("expected 25 viewpoints in {}, found {}", path.display(), out.len());
    }
    Ok(out)
}

// ---- rendering ------------------------------------------------------------

const HEADER: &str = "\
// @generated by `cargo xtask codegen` from assets/archi. DO NOT EDIT.
//
// The inputs are vendored from archimatetool/archi (MIT); see
// assets/archi/PROVENANCE.toml for the exact tag and checksums. Run
// `cargo xtask codegen` to regenerate and review the diff.
";

/// Layer and folder membership are decisions, not data, so they are generated
/// from the one hand-maintained table in xtask rather than written twice.
fn render_folders() -> Result<String> {
    let all_layers: Vec<Layer> = ELEMENTS.iter().map(|(l, _)| *l).collect();
    let all_folders = [
        FolderType::User,
        FolderType::Strategy,
        FolderType::Business,
        FolderType::Application,
        FolderType::Technology,
        FolderType::Relations,
        FolderType::Other,
        FolderType::Diagrams,
        FolderType::Motivation,
        FolderType::ImplementationMigration,
    ];

    let mut s = String::new();
    writeln!(s, "{HEADER}")?;

    writeln!(s, "/// `FolderType.java` literal values. These numbers are written into")?;
    writeln!(s, "/// model files, so they are part of the format.")?;
    writeln!(s, "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]")?;
    writeln!(s, "#[repr(u8)]")?;
    writeln!(s, "pub enum FolderType {{")?;
    for f in all_folders {
        writeln!(s, "    {} = {},", f.ident(), f as u8)?;
    }
    writeln!(s, "}}\n")?;

    writeln!(s, "#[allow(clippy::should_implement_trait)]")?;
    writeln!(s, "impl FolderType {{")?;
    writeln!(s, "    /// The value written as the folder's `type` attribute. Archi omits")?;
    writeln!(s, "    /// the attribute entirely for user folders, since `user` is the")?;
    writeln!(s, "    /// schema default.")?;
    writeln!(s, "    pub fn as_str(self) -> &'static str {{")?;
    writeln!(s, "        match self {{")?;
    for f in all_folders {
        writeln!(s, "            FolderType::{} => {:?},", f.ident(), f.wire())?;
    }
    writeln!(s, "        }}")?;
    writeln!(s, "    }}\n")?;
    writeln!(s, "    pub fn from_str(s: &str) -> Option<FolderType> {{")?;
    writeln!(s, "        Some(match s {{")?;
    for f in all_folders {
        writeln!(s, "            {:?} => FolderType::{},", f.wire(), f.ident())?;
    }
    writeln!(s, "            _ => return None,")?;
    writeln!(s, "        }})")?;
    writeln!(s, "    }}")?;
    writeln!(s, "}}\n")?;

    writeln!(s, "/// ArchiMate layers. Physical is a layer of its own but shares the")?;
    writeln!(s, "/// Technology folder, which is why the two are distinct here.")?;
    writeln!(s, "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]")?;
    writeln!(s, "pub enum Layer {{")?;
    for l in &all_layers {
        writeln!(s, "    {},", l.ident())?;
    }
    writeln!(s, "}}\n")?;

    writeln!(s, "#[allow(clippy::should_implement_trait)]")?;
    writeln!(s, "impl Layer {{")?;
    writeln!(s, "    pub fn as_str(self) -> &'static str {{")?;
    writeln!(s, "        match self {{")?;
    for l in &all_layers {
        writeln!(s, "            Layer::{} => {:?},", l.ident(), l.wire())?;
    }
    writeln!(s, "        }}")?;
    writeln!(s, "    }}\n")?;
    writeln!(s, "    /// Mirrors `ArchimateModel.getDefaultFolderForObject`.")?;
    writeln!(s, "    pub fn folder(self) -> FolderType {{")?;
    writeln!(s, "        match self {{")?;
    for l in &all_layers {
        writeln!(s, "            Layer::{} => FolderType::{},", l.ident(), l.folder().ident())?;
    }
    writeln!(s, "        }}")?;
    writeln!(s, "    }}\n")?;
    writeln!(s, "    pub fn from_str(s: &str) -> Option<Layer> {{")?;
    writeln!(s, "        Some(match s.to_ascii_lowercase().as_str() {{")?;
    for l in &all_layers {
        writeln!(s, "            {:?} => Layer::{},", l.ident().to_ascii_lowercase(), l.ident())?;
    }
    writeln!(s, "            _ => return None,")?;
    writeln!(s, "        }})")?;
    writeln!(s, "    }}")?;
    writeln!(s, "}}")?;

    Ok(s)
}

fn render_types(concepts: &[String], keys: &[(char, String)]) -> Result<String> {
    let idx_of = |name: &str| {
        concepts.iter().position(|c| c == name).unwrap_or_else(|| panic!("unknown concept {name}"))
    };

    let mut s = String::new();
    writeln!(s, "{HEADER}")?;
    writeln!(s, "use super::folders::{{FolderType, Layer}};\n")?;

    // --- elements
    writeln!(s, "/// Every ArchiMate 3.2 element type. Junction is an element in")?;
    writeln!(s, "/// the model even though it behaves as a relationship connector.")?;
    writeln!(s, "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]")?;
    writeln!(s, "#[repr(u8)]")?;
    writeln!(s, "pub enum ElementType {{")?;
    for (_, names) in ELEMENTS {
        for n in *names {
            writeln!(s, "    {n},")?;
        }
    }
    writeln!(s, "}}\n")?;

    writeln!(s, "/// Static facts about one element type.")?;
    writeln!(s, "#[derive(Clone, Copy, Debug)]")?;
    writeln!(s, "pub struct TypeInfo {{")?;
    writeln!(s, "    /// Bare type name, e.g. `ApplicationComponent`.")?;
    writeln!(s, "    pub short: &'static str,")?;
    writeln!(s, "    /// As written in the file, e.g. `archimate:ApplicationComponent`.")?;
    writeln!(s, "    pub xsi: &'static str,")?;
    writeln!(s, "    pub layer: Layer,")?;
    writeln!(s, "    /// The folder Archi files this type under by default.")?;
    writeln!(s, "    pub home: FolderType,")?;
    writeln!(s, "    /// Row/column in the relationship matrix.")?;
    writeln!(s, "    pub matrix_idx: u8,")?;
    writeln!(s, "    /// Figure size to use when bounds carry `-1`.")?;
    writeln!(s, "    pub default_wh: (i32, i32),")?;
    writeln!(s, "}}\n")?;

    let count: usize = ELEMENTS.iter().map(|(_, n)| n.len()).sum();
    writeln!(s, "pub static ELEMENT_INFO: [TypeInfo; {count}] = [")?;
    for (layer, names) in ELEMENTS {
        for n in *names {
            let (w, h) = default_size(n);
            writeln!(
                s,
                "    TypeInfo {{ short: {n:?}, xsi: \"archimate:{n}\", layer: Layer::{}, \
                 home: FolderType::{}, matrix_idx: {}, default_wh: ({w}, {h}) }},",
                layer.ident(),
                layer.folder().ident(),
                idx_of(n),
            )?;
        }
    }
    writeln!(s, "];\n")?;

    writeln!(s, "// `from_str` returning Option is friendlier than the FromStr trait,")?;
    writeln!(s, "// which would force an error type nobody wants to match on.")?;
    writeln!(s, "#[allow(clippy::should_implement_trait)]")?;
    writeln!(s, "impl ElementType {{")?;
    writeln!(s, "    pub const ALL: [ElementType; {count}] = [")?;
    for (_, names) in ELEMENTS {
        for n in *names {
            writeln!(s, "        ElementType::{n},")?;
        }
    }
    writeln!(s, "    ];\n")?;
    writeln!(s, "    #[inline]")?;
    writeln!(s, "    pub fn info(self) -> &'static TypeInfo {{")?;
    writeln!(s, "        &ELEMENT_INFO[self as usize]")?;
    writeln!(s, "    }}\n")?;
    writeln!(s, "    /// Accepts `ApplicationComponent` or `archimate:ApplicationComponent`.")?;
    writeln!(s, "    pub fn from_str(s: &str) -> Option<ElementType> {{")?;
    writeln!(s, "        let s = s.strip_prefix(\"archimate:\").unwrap_or(s);")?;
    writeln!(s, "        Some(match s {{")?;
    for (_, names) in ELEMENTS {
        for n in *names {
            writeln!(s, "            {n:?} => ElementType::{n},")?;
        }
    }
    writeln!(s, "            _ => return None,")?;
    writeln!(s, "        }})")?;
    writeln!(s, "    }}")?;
    writeln!(s, "}}\n")?;

    // --- relationships
    let rels: Vec<(char, &str, String)> = keys
        .iter()
        .map(|(ch, full)| (*ch, full.as_str(), full.trim_end_matches("Relationship").to_string()))
        .collect();

    writeln!(s, "/// The 11 ArchiMate relationship types.")?;
    writeln!(s, "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]")?;
    writeln!(s, "#[repr(u8)]")?;
    writeln!(s, "pub enum RelType {{")?;
    for (_, _, short) in &rels {
        writeln!(s, "    {short},")?;
    }
    writeln!(s, "}}\n")?;

    writeln!(s, "#[derive(Clone, Copy, Debug)]")?;
    writeln!(s, "pub struct RelInfo {{")?;
    writeln!(s, "    /// Bare name, e.g. `Access`.")?;
    writeln!(s, "    pub short: &'static str,")?;
    writeln!(s, "    /// As written in the file, e.g. `archimate:AccessRelationship`.")?;
    writeln!(s, "    pub xsi: &'static str,")?;
    writeln!(s, "    /// Key letter used by the relationship matrix.")?;
    writeln!(s, "    pub key: char,")?;
    writeln!(s, "}}\n")?;

    writeln!(s, "pub static REL_INFO: [RelInfo; {}] = [", rels.len())?;
    for (ch, full, short) in &rels {
        writeln!(s, "    RelInfo {{ short: {short:?}, xsi: \"archimate:{full}\", key: {ch:?} }},")?;
    }
    writeln!(s, "];\n")?;

    writeln!(s, "#[allow(clippy::should_implement_trait)]")?;
    writeln!(s, "impl RelType {{")?;
    writeln!(s, "    pub const ALL: [RelType; {}] = [", rels.len())?;
    for (_, _, short) in &rels {
        writeln!(s, "        RelType::{short},")?;
    }
    writeln!(s, "    ];\n")?;
    writeln!(s, "    #[inline]")?;
    writeln!(s, "    pub fn info(self) -> &'static RelInfo {{")?;
    writeln!(s, "        &REL_INFO[self as usize]")?;
    writeln!(s, "    }}\n")?;
    writeln!(s, "    /// Accepts `Access`, `AccessRelationship` or the `archimate:` form.")?;
    writeln!(s, "    pub fn from_str(s: &str) -> Option<RelType> {{")?;
    writeln!(s, "        let s = s.strip_prefix(\"archimate:\").unwrap_or(s);")?;
    writeln!(s, "        let s = s.strip_suffix(\"Relationship\").unwrap_or(s);")?;
    writeln!(s, "        Some(match s {{")?;
    for (_, _, short) in &rels {
        writeln!(s, "            {short:?} => RelType::{short},")?;
    }
    writeln!(s, "            _ => return None,")?;
    writeln!(s, "        }})")?;
    writeln!(s, "    }}")?;
    writeln!(s, "}}\n")?;

    // --- name overrides
    writeln!(s, "/// EMF serialises these classes under a different name. Getting one")?;
    writeln!(s, "/// wrong yields a file Archi mis-reads, so they live in one place.")?;
    writeln!(s, "pub static XSI_NAME_OVERRIDES: [(&str, &str); {}] = [", XSI_NAME_OVERRIDES.len())?;
    for (from, to) in XSI_NAME_OVERRIDES {
        writeln!(s, "    ({from:?}, {to:?}),")?;
    }
    writeln!(s, "];\n")?;

    writeln!(s, "/// EMF feature renames: Java field name to XML name.")?;
    writeln!(
        s,
        "pub static FEATURE_NAME_OVERRIDES: [(&str, &str); {}] = [",
        FEATURE_NAME_OVERRIDES.len()
    )?;
    for (from, to) in FEATURE_NAME_OVERRIDES {
        writeln!(s, "    ({from:?}, {to:?}),")?;
    }
    writeln!(s, "];")?;

    Ok(s)
}

fn render_matrix(
    concepts: &[String],
    cells: &[MatrixCell],
    keys: &[(char, String)],
) -> Result<String> {
    let n = concepts.len();
    let bit = |ch: char| -> Result<u16> {
        let i = keys
            .iter()
            .position(|(c, _)| *c == ch)
            .with_context(|| format!("unknown relationship key letter {ch:?}"))?;
        Ok(1u16 << i)
    };

    let mut matrix = vec![0u16; n * n];
    for (src, target, letters) in cells {
        let Some(dst) = concepts.iter().position(|c| c == target) else {
            bail!("target concept {target:?} is not a source concept");
        };
        let mut mask = 0u16;
        for ch in letters.chars() {
            mask |= bit(ch)?;
        }
        matrix[src * n + dst] = mask;
    }

    let pseudo = concepts.iter().position(|c| c == "Relationship").context("no pseudo-concept")?;

    let mut s = String::new();
    writeln!(s, "{HEADER}")?;
    writeln!(s, "use super::types::RelType;\n")?;
    writeln!(s, "/// Number of concepts on each axis of the relationship matrix.")?;
    writeln!(s, "pub const CONCEPTS: usize = {n};\n")?;
    writeln!(s, "/// Matrix axis order. Index {pseudo} is the pseudo-concept `Relationship`,")?;
    writeln!(s, "/// used for relationships that target other relationships.")?;
    writeln!(s, "pub const RELATIONSHIP_PSEUDO_IDX: u8 = {pseudo};\n")?;
    writeln!(s, "pub static CONCEPT_NAMES: [&str; {n}] = [")?;
    for c in concepts {
        writeln!(s, "    {c:?},")?;
    }
    writeln!(s, "];\n")?;

    writeln!(s, "/// One bit per [`RelType`], indexed `source * CONCEPTS + target`.")?;
    writeln!(s, "/// This is Archi's derivation-CLOSED table: a set bit means the")?;
    writeln!(s, "/// relationship is permitted, whether directly or by derivation.")?;
    writeln!(s, "static MATRIX: [u16; CONCEPTS * CONCEPTS] = [")?;
    for row in 0..n {
        write!(s, "   ")?;
        for col in 0..n {
            write!(s, " 0x{:04x},", matrix[row * n + col])?;
        }
        writeln!(s, " // {}", concepts[row])?;
    }
    writeln!(s, "];\n")?;

    writeln!(s, "/// Whether the matrix permits `rel` from `source` to `target`.")?;
    writeln!(s, "///")?;
    writeln!(s, "/// This is the table lookup only. Archi layers two further rules on")?;
    writeln!(s, "/// top: no duplicate direct relationship of the same type between the")?;
    writeln!(s, "/// same ordered pair, and every relationship touching a junction must")?;
    writeln!(s, "/// be of the same type, checked transitively through it.")?;
    writeln!(s, "#[inline]")?;
    writeln!(s, "pub fn allows(source: u8, target: u8, rel: RelType) -> bool {{")?;
    writeln!(s, "    let (s, t) = (source as usize, target as usize);")?;
    writeln!(s, "    if s >= CONCEPTS || t >= CONCEPTS {{")?;
    writeln!(s, "        return false;")?;
    writeln!(s, "    }}")?;
    writeln!(s, "    MATRIX[s * CONCEPTS + t] & (1 << rel as u16) != 0")?;
    writeln!(s, "}}\n")?;

    writeln!(s, "/// Every relationship type the matrix permits between this pair.")?;
    writeln!(s, "pub fn permitted(source: u8, target: u8) -> Vec<RelType> {{")?;
    writeln!(s, "    RelType::ALL.into_iter().filter(|r| allows(source, target, *r)).collect()")?;
    writeln!(s, "}}")?;

    Ok(s)
}

fn render_viewpoints(defs: &[ViewpointDef]) -> Result<String> {
    let mut s = String::new();
    writeln!(s, "{HEADER}")?;
    writeln!(s, "use super::types::ElementType;\n")?;
    writeln!(s, "/// A viewpoint restricts which concepts may appear on a view.")?;
    writeln!(s, "///")?;
    writeln!(s, "/// An EMPTY `elements` list means every element is allowed, not that")?;
    writeln!(s, "/// none is — `layered` is the obvious case. The check is evaluated")?;
    writeln!(s, "/// independently for elements and for relationships.")?;
    writeln!(s, "#[derive(Clone, Copy, Debug)]")?;
    writeln!(s, "pub struct Viewpoint {{")?;
    writeln!(s, "    pub id: &'static str,")?;
    writeln!(s, "    pub name: &'static str,")?;
    writeln!(s, "    pub elements: &'static [ElementType],")?;
    writeln!(s, "}}\n")?;

    writeln!(s, "pub static VIEWPOINTS: [Viewpoint; {}] = [", defs.len())?;
    for d in defs {
        let mut elements: Vec<String> = Vec::new();
        for c in &d.concepts {
            if let Some(macro_name) = c.strip_prefix('$').and_then(|c| c.strip_suffix('$')) {
                let layer = expand_macro(macro_name)?;
                let (_, names) = ELEMENTS
                    .iter()
                    .find(|(l, _)| *l == layer)
                    .with_context(|| format!("no elements for layer {layer:?}"))?;
                elements.extend(names.iter().map(|n| n.to_string()));
                // Physical elements are their own macro but Technology's macro
                // does not cover them, so neither expansion may borrow the other.
            } else {
                elements.push(c.clone());
            }
        }
        elements.sort();
        elements.dedup();
        for e in &elements {
            if !ELEMENTS.iter().any(|(_, names)| names.contains(&e.as_str())) {
                bail!("viewpoint {:?} lists unknown concept {e:?}", d.id);
            }
        }
        let list =
            elements.iter().map(|e| format!("ElementType::{e}")).collect::<Vec<_>>().join(", ");
        writeln!(
            s,
            "    Viewpoint {{ id: {:?}, name: {:?}, elements: &[{list}] }},",
            d.id, d.name
        )?;
    }
    writeln!(s, "];\n")?;

    writeln!(s, "pub fn by_id(id: &str) -> Option<&'static Viewpoint> {{")?;
    writeln!(s, "    VIEWPOINTS.iter().find(|v| v.id == id)")?;
    writeln!(s, "}}\n")?;

    writeln!(s, "/// True when the viewpoint permits this element type. An unknown")?;
    writeln!(s, "/// viewpoint id permits everything rather than rejecting a model we")?;
    writeln!(s, "/// simply do not recognise.")?;
    writeln!(s, "pub fn allows(viewpoint: &str, element: ElementType) -> bool {{")?;
    writeln!(s, "    match by_id(viewpoint) {{")?;
    writeln!(s, "        None => true,")?;
    writeln!(s, "        Some(v) => v.elements.is_empty() || v.elements.contains(&element),")?;
    writeln!(s, "    }}")?;
    writeln!(s, "}}")?;

    Ok(s)
}

fn expand_macro(name: &str) -> Result<Layer> {
    Ok(match name {
        "StrategyElements" => Layer::Strategy,
        "BusinessElements" => Layer::Business,
        "ApplicationElements" => Layer::Application,
        "TechnologyElements" => Layer::Technology,
        "PhysicalElements" => Layer::Physical,
        "MotivationElements" => Layer::Motivation,
        "ImplementationMigrationElements" => Layer::ImplementationMigration,
        // Failing loudly matters: a macro we silently ignored would quietly
        // narrow a viewpoint and make the conformance check wrong.
        other => bail!("unknown viewpoint macro ${other}$ — teach xtask about it"),
    })
}

/// Run generated source through rustfmt, using the repo's own rustfmt.toml.
///
/// If rustfmt is unavailable the unformatted text is used rather than failing —
/// codegen still produces correct code, and `cargo fmt --check` will say so.
fn rustfmt(src: String) -> Result<String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut child = match Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: rustfmt unavailable ({e}); writing unformatted output");
            return Ok(src);
        }
    };
    child.stdin.take().context("rustfmt stdin")?.write_all(src.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("rustfmt rejected generated code: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8(out.stdout)?)
}

// ---- tiny XML helpers -----------------------------------------------------
//
// These read three small, stable, vendored files whose exact shape is pinned by
// PROVENANCE.toml, so a line-oriented scan is enough. Anything more general
// belongs in amcli-xml.

fn each_tag<'a>(src: &'a str, tag: &'a str) -> impl Iterator<Item = String> + 'a {
    let open = format!("<{tag} ");
    src.lines().map(str::trim).filter(move |l| l.starts_with(&open)).map(String::from)
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let start = tag.find(&pat)? + pat.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)? + start;
    Some(s[start..end].to_string())
}
