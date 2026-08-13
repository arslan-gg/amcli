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
- Files beginning with a dot are never copied, so no `.gitattributes` or
  `.version` inside the skill folder.

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

## Testing

```bash
cargo test          # identity, involution and property tests
cargo clippy --all-targets
cargo fmt
```

Property tests have already caught two real bugs that hand-written cases missed.
When you fix a bug they find, add the minimal case to `roundtrip.rs` as a named
regression test rather than relying on the random search to catch it again.
