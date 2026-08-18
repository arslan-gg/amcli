# amcli — notes for agents working on this repo

`amcli` is a CLI over ArchiMate model files. Rust workspace, one static binary,
no runtime dependencies.

## The rule that overrides everything else

**A parse-then-write round trip must be byte-identical.** If a change makes
`tests/corpus` fail the identity test, the change is wrong — not the test. The
whole product proposition is that editing a model produces a diff a human can
review, and that only holds if untouched bytes stay untouched.

Practically: never re-serialize what you did not edit. `amcli-xml` blits
untouched subtrees straight out of the source buffer, and any new code that
touches the document must preserve that property.

## Layout

| Crate | Job |
|---|---|
| `crates/amcli-xml` | Format-preserving XML tree with byte spans. Knows nothing about ArchiMate. |
| `crates/amcli-model` | The ArchiMate IR: types, folders, concepts, views, containers (plain XML / ZIP / grafico). |
| `crates/amcli-view` | View geometry, layout, notation — and `icons.rs`, the type icons. |
| `crates/amcli-render` | A compiled view to SVG, or to PNG through `resvg` (pure Rust, so the binary stays static). |
| `crates/amcli-cli` | The binary; `src/web/` is `amcli web`, the read-only viewer. |
| `xtask` | Codegen from the vendored Archi assets. |

## `skills/amcli/` is shipped, not documentation

`npx skills add arslan-gg/amcli` copies that directory out of the default
branch verbatim, and `crates/amcli-cli/src/skill.rs` embeds the same files with
`include_str!` for the binary-first route. **Both routes must produce identical
bytes** — `both_install_routes_ship_the_same_bytes` walks the directory and
fails if a file is there but not in `FILES`.

Consequences worth knowing before you edit it:

- **Nothing may be generated into it.** A file written only by `skill install`
  makes the two routes differ; a committed generated file goes stale on the
  next release. That is why there is no `references/commands.md` and why
  `amcli skill commands` exists instead.
- **The executable bit does not survive.** The skills CLI has two install
  paths, and its blob fast path writes every file 0644. Invoke scripts as
  `sh scripts/install.sh`, never `./scripts/install.sh`.
- **The skill ships from the branch, the binary from the newest tag**, so the
  skill is normally the *newer* of the two. Never tell an agent to reconcile
  that by reinstalling the skill — it would downgrade itself. The reconciliation
  lives in `parse_or_hint` in `main.rs`.
- **Every release updates the skill, and the version line is only half of it.**
  "written for **amcli X.Y.Z**" in `SKILL.md` is pinned to the workspace
  version by a test, so a release commit bumps `Cargo.toml`, `Cargo.lock` *and*
  that line or `cargo test` is red. The half no test can check is the prose:
  a command or flag the release adds is invisible to an agent until `SKILL.md`
  describes it, because that file — not `--help` — is what it reads. So before
  tagging, diff the release against `amcli skill commands` and ask of each new
  subcommand, flag and batch op: is it in `SKILL.md`, or in
  `references/batch.md` if it only matters inside a batch? Shipping a feature
  nobody is told about is the same as not shipping it.
- **The skill's Setup runs the installer every session.** That is only fine
  because `install.sh` / `install.ps1` short-circuit when the newest release is
  already installed and keep the installed binary when there is no network.
  Keep those two properties if you touch the installers.
- Files beginning with a dot are never copied, so no `.gitattributes` or
  `.version` inside the skill folder.

## `amcli web` and `crates/amcli-cli/src/web/`

The viewer is a hand-rolled HTTP/1.1 GET server on `std::net` plus a page of
plain ES modules, all compiled into the binary with `include_str!`. There is
no build step, no bundler and no CDN, and that is a property to keep: the
binary is the whole product, and it works offline.

- **Every file under `src/web/assets/` must be in `ASSETS` in `api.rs`.** A
  test starts the binary and fetches each file on disk; a file that is there
  but not listed is unreachable and fails the test.
- **Server threads never print.** `main` holds the stdout/stderr locks until
  it hands over to the server via `Output::then`, and after that the terminal
  belongs to the person, not to a request log.
- **The URL is printed before the server serves.** That is the whole
  contract for an agent: read one line, keep the process running.
- **Nothing on the page writes.** The only HTTP verb is GET; the viewer is
  read-only by construction, not by policy.
- **The page draws with the same notation as the renderer.** `/api/model`
  carries the fills, figures, icons and line ends from `amcli-view`'s
  `notation.rs` and `icons.rs`, so the graph cannot drift from a rendered view.
- **Type icons are hand-ported code**, in `crates/amcli-view/src/icons.rs`,
  one entry per Archi figure class named in a comment. They are not
  `assets/archi` inputs and `xtask` does not touch them. Path data must stay
  within the 16×16 box and never contain `-0` — the render byte-stability
  test forbids it.

`assets/archi/` holds MIT-licensed files vendored from `archimatetool/archi`.
They are **generated inputs, not hand-edited** — `assets/archi/PROVENANCE.toml`
records the upstream tag and checksums, and updating them is a deliberate,
reviewable change. `cargo xtask verify` enforces both halves: the assets
against the recorded checksums, and the generated tables against the assets.
The refresh procedure is in the header of PROVENANCE.toml itself.

## Format traps worth knowing before you touch the model layer

- Elements *and* relationships both serialize as `<element xsi:type="archimate:X">`.
  They are told apart by the type, not the tag.
- Five types are renamed on the way out: `DiagramModelArchimateObject` →
  `DiagramObject`, `DiagramModelArchimateConnection` → `Connection`,
  `DiagramModelGroup` → `Group`, `DiagramModelNote` → `Note`, and the root
  `ArchimateModel` → `model`.
- `documentation` and `purpose` are child *elements*, not attributes.
- `AccessRelationship/@accessType`: **0 is Write**, 1 Read, 2 Unspecified,
  3 Read/Write. The obvious guess is wrong.
- `<bounds>` is a child element. `x`/`y` are parent-relative and may be negative;
  `width`/`height` of `-1` mean "the figure's default size" (120×55 for elements).
- Bendpoints are relative deltas from the source and target anchors, not points.
- A `.archimate` file may be a **ZIP** containing `model.xml` plus images. Sniff
  before assuming XML.
- EMF omits attributes whose value equals the schema default, so writing one back
  explicitly breaks byte identity.

## Two process-wide things, and what they cost

- **`ids::set_seed` is a `OnceLock`.** `--id-seed` switches new ids from random to
  derived-from-content, and `new_id` is called from deep inside `edit.rs`, so the
  seed is process-wide rather than threaded through every signature. The
  consequence for tests: within one test binary the first caller wins and every
  other test in that file sees the seed. That is why the seeded test lives in its
  own file (`crates/amcli-model/tests/seeded_ids.rs`) — cargo gives each file its
  own binary. Do not set a seed from a shared test file.
- **`--version` carries the commit**, from `crates/amcli-cli/build.rs`. It uses
  the commit *date*, never the build date: a wall clock would make every rebuild
  of the same source a different binary.

## Testing

```bash
cargo test          # identity, involution and property tests
cargo clippy --all-targets
cargo fmt
```

Property tests have already caught two real bugs that hand-written cases missed.
When you fix a bug they find, add the minimal case to `roundtrip.rs` as a named
regression test rather than relying on the random search to catch it again.
