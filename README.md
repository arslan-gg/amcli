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
4e9a7c21  ApplicationComponent  Payment API      /Application/Payments  7  12  3  name
1b90ccde  ApplicationComponent  Payment Gateway  /Application/Payments  2   4  1  name

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

**A model can be rebuilt reproducibly.** Keep the batches in the repository and
pass `--id-seed`: ids are derived from what they name rather than drawn at random,
so regenerating an unchanged model produces an unchanged file. Without a seed —
still the default, because an id only has to be unique — every rebuild reissues
every id and the whole file looks changed.

## Install

Install the skill; it installs the binary.

```bash
npx skills add arslan-gg/amcli -y
```

That writes the [Agent Skills](https://agentskills.io) package into
`~/.agents/skills/amcli/` — the documented cross-tool location Codex reads
natively — and symlinks `~/.claude/skills/amcli` at it. The `-y` matters:
without it the CLI opens an interactive agent picker, which hangs in a
non-interactive shell.

The skill's own setup section then tells the agent to run the installer that
came with it, so in the normal case you never run this yourself:

```bash
sh ~/.agents/skills/amcli/scripts/install.sh
```

It resolves the newest release, verifies a sha256, and installs to
`~/.local/bin`. It prints the absolute path of the binary on stdout and nothing
else, so `AMCLI=$(sh …/install.sh)` works — which matters because a fresh
install is usually not on the current shell's PATH yet. It never uses `sudo`
and never edits a shell config. `AMCLI_VERSION`, `AMCLI_INSTALL_DIR` and
`AMCLI_DRY_RUN` override the obvious things.

If no prebuilt binary matches the platform, or no release exists yet, it builds
with cargo instead. That path also works by hand, and needs no release at all:

```bash
cargo install --git https://github.com/arslan-gg/amcli --locked amcli-cli
```

Note the `--git`. Plain `cargo install amcli-cli` does not work and is not
planned: the binary embeds the skill with `include_str!` from outside its own
package directory, which cargo will not put in a `.crate` tarball.

Binary first instead? `amcli skill install` writes the same skill from the
binary. It refuses to touch a directory `npx skills add` owns, since that
tool's lock file records an upstream tree hash and would silently overwrite
anything written underneath it.

On Windows use the PowerShell installer, which follows the same contract:

```powershell
& ~\.agents\skills\amcli\scripts\install.ps1
```

## Rendering

`amcli view render` draws the geometry the model stores: every figure on the
bounds Archi recorded, every connection on the polyline Archi computes. It does
**not** promise pixel identity with Archi, which is not achievable in principle —
Archi's default view font is the platform system font, so its own export differs
between macOS and Windows.

`amcli view auto` lays a new view out from the dependency graph. The ArchiMate
layer is deliberately not consulted: most relationships in a real model run
*within* a layer, so ranking by layer puts them in one row and turns each into a
horizontal line through whatever sits between its ends.

Each connected component is laid out on its own and the components are packed
side by side, so ten unconnected pairs come out as ten vertical pairs and
nothing can leave a hole between them. Within a component, nodes are ranked by
network simplex, so the rows are as few as the edges allow and each edge is as
short as it can be; ordered within a row by median and sifting to cut
crossings; and given x by Brandes and Köpf — aligned into blocks with a
median neighbour, a corridor and the boxes at both ends of its long edge
first so the whole edge is one column, and the blocks packed as tightly as
the boxes allow. A rank too wide to read — a hundred motivation elements two
ranks deep gives layering nothing to stack — is folded onto several lines
instead of run off the page.

Every edge is one straight line, centre to centre; no bendpoints are ever
written. The layout keeps lines off boxes by where it puts the boxes: a long
edge reserves a corridor in each row it crosses, sequenced where the line
will run and as wide as its slant, and the boxes pack around it; an edge
between neighbouring rows is kept off its own row's boxes by the row gap,
which is computed so that every such line has dropped clear of the row band
before it reaches a neighbour. On a graph that admits a clean drawing the
result has no edge through a box and no two edges crossing; a test asserts
both, and another sweeps four hundred random graphs asserting that no edge
between neighbouring rows ever cuts a box, and that the share of long edges
drawn through a rank they skip stays under a bound. That share is not zero —
a slanted line across a crowded rank sometimes has no seat for its corridor
that does not cost more crossings than it saves — and it is reported.

Boxes are sized to their labels: the width a name wraps into two lines at,
from the stock 120 up to 264, and taller only when it must be. `view layout`
writes sizes back along with positions and straightens every connection, so a
relaid view is redrawn, not just shuffled.

`--layout auto` is layered unless a grid would be both squarer and no more
tangled — crossings plus edges through boxes, which a grid can never route
around — which in practice means the fallback is for edgeless sets.
`--layout layered` and `--layout grid` force one or the other.

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
and PNG output (render to SVG and convert).

No Homebrew tap, deliberately. `brew` does not exist in the Linux containers
where most agents run, and a tap costs a second repository, a fine-grained PAT
that the default `GITHUB_TOKEN` cannot substitute for, and Gatekeeper
quarantine handling — all for a route the agent never takes.

## Licence and trademarks

Apache-2.0. See [NOTICE](NOTICE) for the vendored Archi assets and their MIT
licence.

ArchiMate® is a registered trademark of The Open Group. Archi® is a trademark of
Phillip Beauvoir. This project is independent and is not affiliated with,
endorsed by, or certified by either. It reads and writes their file formats for
interoperability.
