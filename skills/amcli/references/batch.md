# Batches

    amcli apply ops.jsonl
    amcli apply -            # from stdin

One JSON operation per line. Blank lines and lines starting with `#` or `//`
are ignored.

**All or nothing.** Every line is applied in memory, and the file is written
once at the end. If any line fails, the file is byte-identical to what it was
and the error names the line number. There is no partial application to clean
up, and no rollback step that could itself fail.

## Operations

    {"op":"element.add","type":"ApplicationComponent","name":"X","folder":"/Application","doc":"…","props":{"owner":"team"},"ref":"x","if_absent":true}
    {"op":"relation.add","type":"Serving","source":"ref:x","target":"Y","access":"rw","ref":"r","if_absent":true}
    {"op":"element.rename","target":"ref:x","name":"New name"}
    {"op":"element.doc","target":"id:abc","text":"…"}
    {"op":"element.delete","target":"id:abc"}
    {"op":"prop.set","target":"ref:x","key":"owner","value":"team-a"}
    {"op":"folder.add","parent":"/Application","name":"Payments"}

Views too — each mirrors the `view` subcommand of the same name, with the
same fields, and takes a `ref:` wherever it takes a concept:

    {"op":"view.create","name":"Payments","viewpoint":"application_cooperation","folder":"/Views/Payments","replace":true}
    {"op":"view.add","view":"Payments","target":"ref:x"}
    {"op":"view.add","view":"Payments","target":"Checkout","x":240,"y":0,"no_connect":true}
    {"op":"view.auto","name":"Around X","from":"ref:x","depth":2,"direction":"both","layout":"auto","folder":"/Views/Payments","replace":true}
    {"op":"view.layout","view":"Payments","algorithm":"auto","relayout_all":true}
    {"op":"view.rename","view":"Payments","name":"Payments and Checkout"}
    {"op":"view.move","view":"Payments","folder":"/Views/Programme"}
    {"op":"view.delete","view":"Old Sketch"}

A view built member by member — create it, add each element, lay it out —
is a dozen or a hundred lines that would otherwise be a dozen or a hundred
`amcli` invocations, each parsing and writing the whole file, and any one
of them able to fail and leave the view half drawn. In a batch they land
with the concept edits they belong to, once, or not at all; `--dry-run`
covers them; and with `replace` on the create and a seed set, re-running
the batch is a no-op in git. `view.rename` is the exception: like a second
`view rename` at the prompt it fails on the re-run, so keep it out of a
batch meant to be re-run.

## `ref`

A line names its result; later lines address it as `ref:name`. This is what
makes a batch composable: you cannot know the generated id in advance.

Refs resolve forwards only. A typo fails at the line that used it, rather than
silently deferring the problem.

## `if_absent`

Skip the operation if the thing already exists, and bind the `ref` to the
existing one. This is what makes a batch **re-runnable** — after a half-finished
attempt, or against a second model.

Without it, adding the same relationship twice is refused, because a duplicate
relationship of the same type between the same pair adds nothing to the model.

## Rebuilding a model from its batches

Keeping the batches in the repository and regenerating the model from them is a
good workflow, but by default every rebuild mints fresh random ids, so a model
that is semantically unchanged produces a whole-file diff.

Pass the same seed on every command that writes — `init`, `apply`, `view auto` —
or set it once in the environment:

    export AMCLI_ID_SEED=monetech
    amcli init "Monetech" -o model.archimate
    amcli apply 01-capabilities.jsonl
    amcli apply 02-applications.jsonl

Ids are then a function of what they name: an element's from its type and name, a
relationship's from its type and endpoints, a view's from its name. Rebuild
twice and the files are byte-identical, so the diff shows only what changed.

One seed per model, chosen once. Changing it reissues every id, and two models
sharing a seed will give the same id to two elements that share a type and name.

## Checking before writing

    amcli apply ops.jsonl --dry-run      # reports, writes nothing
    amcli apply ops.jsonl --expect-checksum "$CS"   # exit 6 if the file moved
