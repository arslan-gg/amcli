# amcli

**A CLI over ArchiMate models. No Archi, no JVM, no daemon — one static binary
that reads and writes `.archimate` files directly.**

Built so an AI agent can study and change an architecture model the way a person
uses Archi, minus the GUI: search it, walk the graph, edit it safely, validate it
against the real ArchiMate rules, and draw a view.

```console
$ amcli stats
total   elements        947
total   relationships   1745
layer   Application     812

$ amcli search payment -t ApplicationComponent -l 3
4e9a7c21  ApplicationComponent  Payment API      /Application/Payments  7  12  name
1b90ccde  ApplicationComponent  Payment Gateway  /Application/Payments  2   4  name

$ amcli impact id:4e9a7c21 -D in          # what breaks if this changes?
$ amcli relation add Serving "Payment API" "Checkout"
$ amcli view auto "Payments" --from "Payment API" -n 2
$ amcli view render "Payments" -o payments.svg
```

## Why this exists

Archi's own command line (ACLI) is a batch pipeline: load, save, import, CSV,
HTML report, Open Exchange, coArchi. It has no command to search a model, walk
its graph, edit one element, or validate anything, and no image export. The only
headless route to those today is jArchi scripting, which needs a full Archi
install (Eclipse RCP, ~150–200 MB) plus a JRE, starts in seconds, and makes you
write a fresh JavaScript file for every question.

Among open-source libraries, nothing permissively licensed does full-fidelity
**read *and* write** of the Archi format at all.

## What makes it different

**Edits produce a clean diff.** Untouched nodes are written back as the exact
bytes they were parsed from — comments, whitespace, attribute order, the quoting
of the XML declaration, everything. Renaming one element changes one line.

**Writes cannot corrupt the model.** Every write goes through Archi's own 62×62
relationship matrix and is refused if the standard forbids it — with the
permitted alternatives named. Deleting a concept also removes every diagram
object and connection that referenced it and recomputes the view's
`targetConnections`, so the model still opens in Archi afterwards. Saves are
atomic; `--expect-checksum` refuses a write over a file that moved since you read
it.

**It answers questions in one call.** `search`, `trace`, `path`, `impact`,
`cycles` — tab-separated records on stdout, context on stderr, and exit codes an
agent can branch on without parsing prose. `3` is not found, `4` is ambiguous,
and both come back with something to retry.

**Batches are atomic.** `amcli apply` takes JSONL, resolves forward references
between lines, and writes once at the end. If any line fails the file is
byte-identical.

## Install

Not published yet. To build from source:

```bash
cargo build --release          # target/release/amcli
amcli skill install            # teach your agent to use it
```

`skill install` writes the [Agent Skills](https://agentskills.io) package into
`~/.agents/skills/amcli/` — the documented cross-tool location Codex reads
natively — and symlinks `~/.claude/skills/amcli` at it, mirroring what
`npx skills add` does.

## Rendering

`amcli view render` draws the geometry the model stores: every figure on the
bounds Archi recorded, every connection on the polyline Archi computes. It does
**not** promise pixel identity with Archi, which is not achievable in principle —
Archi's default view font is the platform system font, so its own export differs
between macOS and Windows.

`export mermaid` and `export dot` re-lay-out, so they are for a quick look in a
chat window rather than for reproducing a diagram someone drew.

## Development

```bash
cargo test
cargo xtask verify   # the generated tables still match assets/archi
```

The corpus in `tests/corpus/` is real Archi output. The identity test asserts
that parsing and writing every file in it is a byte-for-byte no-op; the property
tests apply random edit sequences and assert the result still re-parses, still
means the same thing, and left every untouched subtree alone.

`assets/archi/` holds MIT-licensed files vendored from `archimatetool/archi`;
`cargo xtask codegen` turns them into the type tables and the packed relationship
matrix, and CI fails if the committed output goes stale.

## Status

Read, write, validate, views and SVG all work and are covered by tests. Not yet
built: coArchi's grafico directory format, The Open Group's Open Exchange XML,
PNG output (render to SVG and convert), and publication to a Homebrew tap.

## Licence and trademarks

Apache-2.0. See [NOTICE](NOTICE) for the vendored Archi assets and their MIT
licence.

ArchiMate® is a registered trademark of The Open Group. Archi® is a trademark of
Phillip Beauvoir. This project is independent and is not affiliated with,
endorsed by, or certified by either. It reads and writes their file formats for
interoperability.
