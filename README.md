# amcli

**A CLI over ArchiMate models. No Archi, no JVM, no daemon — one static binary that
reads and writes `.archimate` files directly.**

Built so an AI agent can study and change an architecture model the way a person
uses Archi, minus the GUI: search it, walk the graph, edit it safely, validate it
against the real ArchiMate rules, and render a view.

> Status: early. `amcli-xml` — the layer everything else stands on — is done and
> tested. The CLI itself is not built yet. See [the milestones](#milestones).

## Why this exists

Archi's own command line (ACLI) is a batch pipeline: load, save, import, CSV,
HTML report, Open Exchange, coArchi. It has no command to search a model, walk
its graph, edit one element, or validate anything, and no image export. The only
way to do those things headlessly today is jArchi scripting, which needs a full
Archi install (Eclipse RCP, ~150–200 MB) plus a JRE, starts in seconds, and makes
you write a fresh JavaScript file for every question you want to ask.

And among open-source libraries, nothing permissively licensed does full-fidelity
**read *and* write** of the Archi format at all.

## What makes it different

**Edits produce a clean diff.** Untouched nodes are written back as the exact
bytes they were parsed from — comments, whitespace, attribute order, the quoting
of the XML declaration, everything. Changing one element's name changes one line
in `git diff`, not the whole file.

**Writes cannot corrupt the model.** Every write is atomic (temp file, fsync,
rename), backed up by default, and checked against the ArchiMate relationship
matrix first. Deleting a concept also cleans up every diagram object and
connection that referenced it, so the model still opens in Archi afterwards.

**It answers questions in one call.** `search`, `trace`, `path`, `impact` — with
a stable output contract and exit codes an agent can branch on without parsing
prose.

## Milestones

- **M0 — read.** Parse (including the zipped variant), search, list, query,
  trace, path, stats. Selector language, output contract, the skill.
- **M1 — write.** Element/relation/folder/property CRUD, atomic batches,
  validation on every write, cascading deletes that keep views consistent.
- **M2 — formats and pictures.** grafico and Open Exchange, view authoring with
  auto-layout, SVG and PNG rendering.

## Development

```bash
cargo test
```

The corpus in `tests/corpus/` is real Archi output. The identity test asserts
that parsing and writing every file in it is a byte-for-byte no-op; the
property tests apply random edit sequences and assert the result still re-parses,
still means the same thing, and left every untouched subtree alone.

## Licence and trademarks

Apache-2.0. See [NOTICE](NOTICE) for vendored Archi assets and their MIT licence.

ArchiMate® is a registered trademark of The Open Group. Archi® is a trademark of
Phillip Beauvoir. This project is independent and is not affiliated with,
endorsed by, or certified by either. It reads and writes their file formats for
interoperability.
