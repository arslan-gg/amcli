---
name: amcli
description: >-
  Read, search, traverse, edit, validate and draw ArchiMate architecture models
  (.archimate files) from the command line with the `amcli` binary. Use when the
  user asks about an enterprise, solution or application architecture model;
  when they mention ArchiMate, Archi, coArchi, a .archimate file, application
  components, business processes, capabilities or an EA repository; or when a
  repository contains a *.archimate file. Also use for: finding which services
  depend on a component, tracing dependencies, assessing the blast radius of a
  change, listing what reads or writes a data object, adding or renaming
  elements and relationships, checking a model for rule violations, and
  producing a diagram. Prefer this over opening the model XML directly — these
  files run to megabytes, reading one wastes context, and hand-editing the XML
  corrupts models in ways Archi then refuses to open.
license: Apache-2.0
compatibility: >-
  Needs a shell and the `amcli` binary; if the binary is missing, the Setup
  section installs it and needs network for that one step. After that: no
  network, no daemon, no server, and no Archi installation — amcli works
  directly on the model file.
metadata:
  homepage: https://github.com/arslan-gg/amcli
  binary: amcli
  spec: ArchiMate 3.2
---

# amcli — ArchiMate models from the command line

A single binary that reads and edits ArchiMate model files directly. No GUI, no
JVM, no server.

**Never read a `.archimate` file with Read or cat.** They run to megabytes of
XML. Every question you have is one `amcli` command, and editing that XML by
hand corrupts models.

## Setup

Run this before anything else:

    amcli --version

If it prints a version, carry on to the next section. If it does not:

**Wherever `sh` runs** — macOS, Linux, WSL, Git Bash — use the installer that
came with this skill. It sits next to this file, which is normally
`~/.agents/skills/amcli`, or `./.agents/skills/amcli` for a project install:

    AMCLI=$(sh ~/.agents/skills/amcli/scripts/install.sh)

It prints the absolute path of the binary on stdout and nothing else. **Use
`$AMCLI` for the rest of the session.** A newly installed binary is usually not
on the current shell's PATH yet, so plain `amcli` will still report "command not
found" even though the install succeeded — that is the single most likely way
this goes wrong.

**Native Windows PowerShell**, where there is no `sh`: there is no Windows build
yet. Use WSL, or install Rust from <https://rustup.rs> and run

    cargo install --git https://github.com/arslan-gg/amcli --locked amcli-cli

The installer asks for nothing, never uses `sudo`, and never edits a shell
config. If no prebuilt binary matches the platform it builds one with cargo on
its own. Do not pipe it from a URL — it is already on disk.

## Finding the model

amcli finds the model on its own: `-m PATH`, else `$AMCLI_MODEL`, else the
nearest `*.archimate` walking up from the working directory. If several are
found it exits 4 and lists them — pass `-m`.

## The loop

Start every architecture question here. Do not open source code first.

    amcli stats                       # how big is this thing, and of what
    amcli search <term>               # find the concept, get its id
    amcli get <id-or-name>            # what it is, and everything it touches
    amcli trace <id-or-name> -n 2     # the neighbourhood
    # only now read source code, and only the files the model pointed you at

## Flags mean one thing each

    -t concept type   -r relationship type   -f folder   -D direction(out|in|both)
    -n depth          -l limit               -m model    -F format   -o output file

Subjects are positional, never flags:

    amcli element  add ApplicationComponent "Refund Service" -f /Application
    amcli relation add Serving "Refund Service" "Checkout Service"

## Output and token economy

The default output is tab-separated records, one per line — cheap to read and
easy to `cut -f2`. Counts and hints go to stderr. Add `-F json` only when you
need nested structure, such as the relationship ids inside `get`.

    amcli query 'layer=Application' --count    # ask "how many" FIRST, always
    amcli search auth -l 10 --fields id,name   # project down
    amcli list --fields -documentation         # or drop a field

Never run an unfiltered `list` on an unfamiliar model. Run `amcli stats` first.

## Addressing concepts

    id:5dde26f7                      an id — always unambiguous, always prefer it
    "Payment API"                    an exact name
    ApplicationComponent:"Payment"   a name qualified by type
    "*Payment*"                      a glob

Filter expressions, quoted as one argument:

    amcli query 'type=ApplicationComponent and name~payment'
    amcli query 'prop:owner=team-a and not folder^=/Technology'
    amcli query 'layer=Application and deg>10'
    amcli query 'out:Access~Customer'    # everything that accesses something matching

Operators: `=` exact · `~` contains · `^=` prefix · `=~` regex · `!=` · `>` `<`
on `deg`. Fields: `id name type layer folder doc deg view prop:KEY in:RelType
out:RelType`.

## Exit codes — branch on these, do not parse messages

    0 ok   2 usage   3 not found   4 ambiguous   5 invalid   6 conflict
    7 io   8 unsupported

On **3**, the response lists the nearest names — retry with one, do not run
another search. On **4**, it lists candidates, each with a ready-to-paste
`id:` selector. Never work around an ambiguity by guessing; re-run with the id.

## Graph questions

    amcli path "Web App" "Customer Database"   # how are these connected?
    amcli impact id:5dde26f7 -D in             # what breaks if this changes?
    amcli neighbors id:5dde26f7 -r Serving     # only Serving relationships
    amcli descendants "Payments Capability"    # the composition tree
    amcli cycles                               # dependency cycles

## Editing

    amcli element  add ApplicationComponent "Refund Service" --doc "…"
    amcli element  rename id:c40a19b7 "Refunds Service"
    amcli relation add Access "Refunds Service" "Refund Record" --access rw
    amcli prop set id:c40a19b7 owner team-payments
    amcli element  delete id:c40a19b7 -y

Every write is checked against the ArchiMate relationship matrix first and
refused (exit 5) if the standard forbids it — and the refusal names what *is*
permitted between those two types, so read it rather than guessing again.

Deleting refuses by default when it would take other things with it, and the
refusal is the impact report. Add `-y` once you have read it.

Use `--dry-run` when unsure. Use `--expect-checksum` when you read the model on
an earlier turn and are writing now:

    CS=$(amcli info -F json -q | jq -r '.[0].checksum')
    amcli element rename id:x "New" --expect-checksum "$CS"    # exit 6 if it moved

**For more than two edits, use one atomic batch rather than a sequence.**

    amcli apply - <<'EOF'
    {"op":"element.add","type":"ApplicationComponent","name":"Refund Service","ref":"r","if_absent":true}
    {"op":"element.add","type":"DataObject","name":"Refund Record","ref":"rec","if_absent":true}
    {"op":"relation.add","type":"Access","source":"ref:r","target":"ref:rec","access":"rw","if_absent":true}
    EOF

`ref` names a line's result so a later line can point at it before its id
exists. `if_absent` makes the batch safe to re-run. If any line fails, nothing
is written and the file is byte-identical.

## Before you finish any edit

    amcli validate

Exit 5 means the model has errors. Each finding names a line in the file and
carries a `fix` command. `amcli validate --fix` applies only the repairs that
are derived rather than chosen — orphaned diagram objects and stale view
mirrors — and never deletes anyone's modelling.

## Views and diagrams

    amcli view list
    amcli view auto "Refund Flow" --from "Refund Service" -n 2 --layout layered
    amcli view render "Refund Flow" -o refund.svg
    amcli export mermaid                       # a quick inline diagram for chat

`view render` draws the geometry the model actually stores. `export mermaid`
and `export dot` re-lay-out, so they are for a quick look, not for reproducing
someone's diagram.

## Going deeper

| Where | When |
|---|---|
| `amcli skill commands` | you need a subcommand or flag not shown above — it prints the whole tree, read out of the binary you are actually running, so it is never out of date |
| `amcli <command> --help` | you need one command's flags in detail |
| `references/types.md` | you need an exact ArchiMate 3.2 type name, or which relationships are legal between two types |
| `references/batch.md` | you are writing a batch of more than about ten operations |
