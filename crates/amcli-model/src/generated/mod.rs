//! Tables derived from the vendored Archi assets.
//!
//! Everything in the sibling modules is produced by `cargo xtask codegen` and
//! must not be hand-edited. `cargo xtask verify` fails the build if the
//! committed output no longer matches `assets/archi`, so an upstream Archi
//! update always arrives as a reviewable diff.

pub mod folders;
pub mod matrix;
pub mod types;
pub mod viewpoints;

pub use folders::{FolderType, Layer};
pub use types::{ElementType, RelType, TypeInfo};
