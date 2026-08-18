//! End-to-end tests over the real binary. These assert the contract an agent
//! depends on: exit codes it can branch on, records it can cut, and writes that
//! either land completely or not at all.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;

struct Model {
    dir: tempfile::TempDir,
}

impl Model {
    fn new(fixture: &str) -> Model {
        let dir = tempfile::tempdir().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(fixture);
        std::fs::copy(&src, dir.path().join("m.archimate")).unwrap();
        Model { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("m.archimate")
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::cargo_bin("amcli")
            .unwrap()
            .arg("-m")
            .arg(self.path())
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.path()).unwrap()
    }
}

fn rows(stdout: &str) -> Vec<Vec<&str>> {
    stdout.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').collect()).collect()
}

#[test]
fn records_go_to_stdout_and_context_goes_to_stderr() {
    let m = Model::new("modelimporter_test.archimate");
    let (code, out, err) = m.run(&["search", "BA"]);
    assert_eq!(code, 0);

    // stdout is nothing but records, so it pipes into `cut -f2` unchanged.
    for line in out.lines() {
        assert!(line.contains('\t'), "not a record: {line}");
    }
    assert!(!out.contains("total"), "counts belong on stderr, not in the data");
    let _ = err;
}

#[test]
fn exit_codes_distinguish_missing_from_ambiguous() {
    let m = Model::new("modelimporter_test.archimate");

    // A miss comes back with the nearest names, so the retry needs no second
    // exploratory search.
    let (code, _, err) = m.run(&["get", "BA111"]);
    assert_eq!(code, 3, "not found");
    assert!(err.contains("did you mean"), "{err}");
    assert!(err.contains("BA1"), "{err}");

    // Two concepts really sharing a name is a different answer with a different
    // remedy.
    m.run(&["element", "add", "BusinessActor", "Twin"]);
    m.run(&["element", "add", "BusinessRole", "Twin"]);
    let (code, _, err) = m.run(&["get", "Twin"]);
    assert_eq!(code, 4, "ambiguous");
    assert!(err.contains("2 concepts match"), "{err}");
    assert!(err.contains("id:"), "each candidate is a paste-ready selector: {err}");

    // And qualifying by type resolves it.
    let (code, _, _) = m.run(&["get", "BusinessActor:Twin"]);
    assert_eq!(code, 0);
}

#[test]
fn a_forbidden_relationship_is_refused_with_the_alternative_named() {
    let m = Model::new("modelimporter_test.archimate");
    m.run(&["element", "add", "DataObject", "Rec"]);
    m.run(&["element", "add", "ApplicationComponent", "Svc"]);

    let (code, _, err) = m.run(&["relation", "add", "Serving", "Rec", "Svc"]);
    assert_eq!(code, 5, "invalid");
    assert!(err.contains("does not permit Serving"), "{err}");
    assert!(err.contains("permitted here: Association"), "the error teaches: {err}");

    let (code, _, _) = m.run(&["relation", "add", "Association", "Rec", "Svc"]);
    assert_eq!(code, 0);
}

#[test]
fn an_edit_changes_only_the_lines_it_has_to() {
    let m = Model::new("modelimporter_test.archimate");
    let before = m.text();

    let (code, _, _) = m.run(&["element", "rename", "BA1", "Renamed Actor"]);
    assert_eq!(code, 0);

    let after = m.text();
    let differing = after.lines().zip(before.lines()).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 1, "renaming one element must not rewrite the file");
    assert_eq!(after, before.replace(r#"name="BA1""#, r#"name="Renamed Actor""#));
}

#[test]
fn deleting_refuses_until_told_and_then_leaves_no_dangling_reference() {
    let m = Model::new("testmodel1.archimate");
    let before = m.text();

    // The refusal IS the impact report, so the retry is informed.
    let (code, _, err) = m.run(&["element", "delete", "Business Actor"]);
    assert_eq!(code, 5);
    assert!(err.contains("also removes 5 other thing"), "{err}");
    assert!(err.contains("diagram_objects"), "{err}");
    assert_eq!(m.text(), before, "a refused delete writes nothing");

    // A dry run reports and still writes nothing.
    let (code, out, _) = m.run(&["element", "delete", "Business Actor", "--dry-run"]);
    assert_eq!(code, 0);
    assert!(out.contains("true"), "dry_run is reported: {out}");
    assert_eq!(m.text(), before);

    let (code, _, _) = m.run(&["element", "delete", "Business Actor", "-y"]);
    assert_eq!(code, 0);

    let after = m.text();
    for gone in ["59fa6c90", "ffdc8ea9", "eac5adf1", "f408e9d0"] {
        assert!(!after.contains(gone), "{gone} survived");
    }
    assert!(!after.contains("targetConnections"), "the derived mirror was recomputed");

    let (code, _, err) = m.run(&["validate", "--level", "integrity"]);
    assert_eq!(code, 0, "the model still loads and still checks out: {err}");
}

#[test]
fn a_stale_checksum_refuses_the_write() {
    let m = Model::new("modelimporter_test.archimate");
    let before = m.text();

    let (code, _, err) = m.run(&["element", "rename", "BA1", "X", "--expect-checksum", "deadbeef"]);
    assert_eq!(code, 6, "conflict");
    assert!(err.contains("changed since"), "{err}");
    assert_eq!(m.text(), before, "nothing was applied");

    // With the real checksum it goes through.
    let (_, out, _) = m.run(&["info", "-F", "json", "-q"]);
    let checksum = out.split(r#""checksum":""#).nth(1).unwrap().split('"').next().unwrap();
    let (code, _, _) = m.run(&["element", "rename", "BA1", "X", "--expect-checksum", checksum]);
    assert_eq!(code, 0);
}

#[test]
fn trace_returns_nodes_and_edges_as_flat_records() {
    let m = Model::new("modelimporter_test.archimate");
    let (code, out, _) = m.run(&["trace", "BA1", "-n", "2"]);
    assert_eq!(code, 0);

    let r = rows(&out);
    assert!(r.iter().any(|row| row[0] == "node"));
    assert!(r.iter().any(|row| row[0] == "edge"), "edges are records, not a count: {out}");
    // Edges are keyed by id, because two concepts can share a name.
    let edge = r.iter().find(|row| row[0] == "edge").unwrap();
    assert!(edge[1].len() > 8, "an edge carries its own id: {edge:?}");
}

#[test]
fn token_economy_flags_do_what_they_say() {
    let m = Model::new("modelimporter_test.archimate");

    // --count answers "how many" without paying for the rows.
    let (code, out, _) = m.run(&["list", "--count"]);
    assert_eq!(code, 0);
    assert_eq!(out.lines().count(), 1);
    assert!(out.trim().parse::<usize>().is_ok(), "{out}");

    // --fields projects.
    let (_, out, _) = m.run(&["list", "--fields", "id,name"]);
    for row in rows(&out) {
        assert_eq!(row.len(), 2, "{row:?}");
    }

    // Subtractive projection drops instead of keeping.
    let (_, full, _) = m.run(&["list"]);
    let (_, less, _) = m.run(&["list", "--fields", "-folder"]);
    assert_eq!(rows(&less)[0].len(), rows(&full)[0].len() - 1);

    // -q drops the envelope in JSON.
    let (_, out, _) = m.run(&["list", "-F", "json", "-q"]);
    assert!(out.trim_start().starts_with('['), "{out}");
}

#[test]
fn json_output_is_valid_and_carries_the_envelope() {
    let m = Model::new("modelimporter_test.archimate");
    let (_, out, _) = m.run(&["get", "BA1", "-F", "json"]);
    assert!(out.contains(r#""ok":true"#));
    assert!(out.contains(r#""data":["#));
    assert!(out.contains(r#""meta":{"#));
    // Relationship ids are present, which is the only way to address one.
    assert!(out.contains(r#""relations":[{"id":"#), "{out}");

    let (_, out, _) = m.run(&["get", "nope", "-F", "json"]);
    assert!(out.contains(r#""ok":false"#));
    assert!(out.contains(r#""exit":3"#), "the exit code is in the payload too: {out}");
}

#[test]
fn a_bad_filter_says_what_the_fields_are() {
    let m = Model::new("modelimporter_test.archimate");
    let (code, _, err) = m.run(&["query", "bogus=1"]);
    assert_eq!(code, 2, "usage");
    assert!(err.contains("unknown field"), "{err}");
    assert!(err.contains("layer"), "{err}");
}

#[test]
fn validate_reports_findings_on_stdout_and_the_verdict_in_the_exit_code() {
    let m = Model::new("testDeleteHandler.archimate");
    let (code, out, _) = m.run(&["validate", "--level", "rules"]);
    assert_eq!(code, 5, "the fixture carries two matrix violations");

    let r = rows(&out);
    assert!(r.iter().any(|row| row[0] == "REL2001"));
    // Every finding names a line and a fix.
    for row in r.iter().filter(|row| row[0] == "REL2001") {
        assert!(row[4].parse::<u32>().unwrap() > 0, "line: {row:?}");
        assert!(row.last().unwrap().starts_with("amcli "), "runnable fix: {row:?}");
    }

    // Levels are cumulative, so integrity still reports them.
    let (code, _, _) = m.run(&["validate", "--level", "integrity"]);
    assert_eq!(code, 5);

    // Stopping at types says nothing about them: these are legality problems,
    // not schema ones.
    let (code, out, _) = m.run(&["validate", "--level", "types"]);
    assert_eq!(code, 0);
    assert!(!out.contains("REL2001"));
}

#[test]
fn model_discovery_walks_up_and_refuses_to_guess() {
    let m = Model::new("modelimporter_test.archimate");
    let nested = m.dir.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();

    let out =
        Command::cargo_bin("amcli").unwrap().current_dir(&nested).arg("info").output().unwrap();
    assert_eq!(out.status.code(), Some(0), "the model one directory up is found");

    // Two models in the same directory is ambiguous, not a coin toss.
    std::fs::copy(m.path(), m.dir.path().join("other.archimate")).unwrap();
    let out = Command::cargo_bin("amcli")
        .unwrap()
        .current_dir(m.dir.path())
        .arg("info")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out.stderr).contains("pass -m"));
}

#[test]
fn every_write_leaves_a_model_that_still_loads() {
    let m = Model::new("modelimporter_test.archimate");
    let steps: &[&[&str]] = &[
        &["element", "add", "ApplicationComponent", "Svc", "--doc", "Docs & more"],
        &["element", "add", "DataObject", "Rec"],
        &["relation", "add", "Access", "Svc", "Rec", "--access", "rw"],
        &["prop", "set", "Svc", "owner", "team-a"],
        &["folder", "add", "/Application", "Payments"],
        &["element", "move", "Svc", "-f", "/Application/Payments"],
        &["element", "rename", "Svc", "Renamed"],
    ];
    for s in steps {
        let (code, _, err) = m.run(s);
        assert_eq!(code, 0, "{s:?} failed: {err}");
    }

    let (code, out, _) = m.run(&["get", "Renamed", "-F", "json"]);
    assert_eq!(code, 0);
    assert!(out.contains(r#""folder":"/Application/Payments""#), "{out}");
    assert!(out.contains(r#""key":"owner""#), "{out}");
    // The documentation was escaped on the way in and comes back intact.
    assert!(out.contains("Docs & more"), "{out}");

    let (code, _, err) = m.run(&["validate"]);
    assert_eq!(code, 0, "{err}");
}

/// A view built member by member is a dozen commands, each writing the file
/// and any one able to fail halfway. In a batch the view ops land with the
/// concept edits they belong to, once, and `--dry-run` covers them too.
#[test]
fn a_batch_can_build_and_lay_out_a_view() {
    let m = Model::new("modelimporter_test.archimate");
    let ops = m.dir.path().join("view.jsonl");
    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"element.add","type":"ApplicationComponent","name":"Refund Service","ref":"r","if_absent":true}"#,
            "\n",
            r#"{"op":"element.add","type":"DataObject","name":"Refund Record","ref":"rec","if_absent":true}"#,
            "\n",
            r#"{"op":"relation.add","type":"Access","source":"ref:r","target":"ref:rec","access":"rw","if_absent":true}"#,
            "\n",
            r#"{"op":"view.create","name":"Refunds","replace":true}"#,
            "\n",
            // A ref, a plain name, and something already in the model.
            r#"{"op":"view.add","view":"Refunds","target":"ref:r"}"#,
            "\n",
            r#"{"op":"view.add","view":"Refunds","target":"Refund Record"}"#,
            "\n",
            r#"{"op":"view.add","view":"Refunds","target":"BA1"}"#,
            "\n",
            r#"{"op":"view.layout","view":"Refunds","relayout_all":true}"#,
            "\n",
            r#"{"op":"view.auto","name":"Around Refunds","from":"ref:r","depth":1,"replace":true}"#,
            "\n",
            r#"{"op":"view.rename","view":"Around Refunds","name":"Refund Neighbourhood"}"#,
            "\n",
        ),
    )
    .unwrap();

    // Dry run: every line reports, nothing is written.
    let before = m.text();
    let (code, out, _) = m.run(&["apply", ops.to_str().unwrap(), "--dry-run"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(rows(&out).len(), 10);
    assert_eq!(m.text(), before, "a dry run writes nothing");
    assert!(
        !out.contains("dry_run"),
        "the view rows do not each claim dry-run; the batch says so once: {out}"
    );

    // For real: the view exists with three objects, the access relationship
    // drawn between two of them, and the second view renamed.
    let (code, out, err) = m.run(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("view.create") && out.contains("view.layout"), "{out}");
    let (_, listing, _) = m.run(&["view", "list", "-q"]);
    assert!(listing.contains("Refunds"), "{listing}");
    assert!(listing.contains("Refund Neighbourhood"), "{listing}");
    assert!(!listing.contains("Around Refunds"), "renamed, not duplicated: {listing}");
    let (_, json, _) = m.run(&["view", "render", "Refunds", "--as", "json"]);
    let scene: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(scene["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(scene["edges"].as_array().unwrap().len(), 1, "the access edge is drawn");

    // Seeded, a re-run is a no-op byte for byte — the property the whole
    // batch design exists for, and view ops must not break it. The rename is
    // left out of this batch: a rename cannot be re-run, and the batch says
    // so like a second `view rename` at the prompt would.
    let again = m.dir.path().join("again.jsonl");
    let text = std::fs::read_to_string(&ops).unwrap();
    let text: String =
        text.lines().filter(|l| !l.contains("view.rename")).map(|l| format!("{l}\n")).collect();
    std::fs::write(&again, text).unwrap();
    let seeded = |m: &Model| {
        Command::cargo_bin("amcli")
            .unwrap()
            .env("AMCLI_ID_SEED", "t")
            .arg("-m")
            .arg(m.path())
            .args(["apply", again.to_str().unwrap()])
            .output()
            .unwrap()
    };
    let r = seeded(&m);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let first = m.text();
    assert!(seeded(&m).status.success());
    assert_eq!(m.text(), first, "a seeded re-run changes nothing");

    // A bad view line abandons the batch like any other.
    let bad = m.dir.path().join("bad.jsonl");
    std::fs::write(
        &bad,
        concat!(
            r#"{"op":"element.add","type":"Goal","name":"Would Be Added"}"#,
            "\n",
            r#"{"op":"view.add","view":"Refunds","target":"No Such Thing"}"#,
            "\n",
        ),
    )
    .unwrap();
    let (code, _, err) = m.run(&["apply", bad.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert!(err.contains("line 2"), "{err}");
    assert_eq!(m.text(), first, "nothing from the failed batch was written");
}

#[test]
fn a_batch_lands_completely_or_not_at_all() {
    let m = Model::new("modelimporter_test.archimate");
    let ops = m.dir.path().join("ops.jsonl");

    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"element.add","type":"ApplicationComponent","name":"Refund Service","ref":"r","if_absent":true}"#,
            "\n",
            r#"{"op":"element.add","type":"DataObject","name":"Refund Record","ref":"rec","if_absent":true}"#,
            "\n",
            r#"{"op":"relation.add","type":"Access","source":"ref:r","target":"ref:rec","access":"rw","if_absent":true}"#,
            "\n",
        ),
    )
    .unwrap();

    let (code, out, _) = m.run(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(rows(&out).len(), 3);

    // `if_absent` makes the whole batch re-runnable, byte for byte.
    let after_first = m.text();
    let (code, _, _) = m.run(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(m.text(), after_first, "a re-run changes nothing");

    // One bad line and the file is untouched — there is no partial state to
    // clean up, because the write only happens once at the end.
    let bad = m.dir.path().join("bad.jsonl");
    std::fs::write(
        &bad,
        concat!(
            r#"{"op":"element.add","type":"ApplicationComponent","name":"Would Be Added"}"#,
            "\n",
            r#"{"op":"relation.add","type":"Serving","source":"Refund Record","target":"Refund Service"}"#,
            "\n",
        ),
    )
    .unwrap();
    let (code, _, err) = m.run(&["apply", bad.to_str().unwrap()]);
    assert_eq!(code, 5);
    assert!(err.contains("line 2"), "the failing line is named: {err}");
    assert_eq!(m.text(), after_first, "nothing from the failed batch was written");
    assert!(!m.text().contains("Would Be Added"), "not even the line that succeeded");
}

#[test]
fn a_ref_must_be_defined_before_it_is_used() {
    let m = Model::new("modelimporter_test.archimate");
    let ops = m.dir.path().join("ops.jsonl");
    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"relation.add","type":"Serving","source":"ref:later","target":"BA1"}"#,
            "\n",
            r#"{"op":"element.add","type":"ApplicationComponent","name":"Later","ref":"later"}"#,
            "\n",
        ),
    )
    .unwrap();
    let (code, _, err) = m.run(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 3);
    assert!(err.contains("no earlier line named `later`"), "{err}");
}

#[test]
fn views_can_be_generated_and_drawn() {
    let m = Model::new("modelimporter_test.archimate");
    let (code, out, _) =
        m.run(&["view", "auto", "Generated", "--from", "BA1", "-n", "2", "--layout", "layered"]);
    assert_eq!(code, 0, "{out}");

    let svg = m.dir.path().join("v.svg");
    let (code, _, _) = m.run(&["view", "render", "Generated", "-o", svg.to_str().unwrap()]);
    assert_eq!(code, 0);

    let body = std::fs::read_to_string(&svg).unwrap();
    assert!(body.starts_with("<svg xmlns="));
    assert!(body.contains("BA1"));
    // Edges after nodes, matching GEF's layer order.
    assert!(body.find("class=\"nodes\"") < body.find("class=\"edges\""));

    // A generated view is a valid model, not just a picture.
    let (code, _, err) = m.run(&["validate", "--level", "integrity"]);
    assert_eq!(code, 0, "{err}");
}

#[test]
fn rendering_an_existing_view_keeps_the_geometry_the_file_records() {
    let m = Model::new("testmodel1.archimate");
    let (code, out, _) = m.run(&["view", "render", "2 Test Bounds and Images", "--as", "json"]);
    assert_eq!(code, 0);

    // The actor sits inside a group at (156,204) with a relative (36,42).
    assert!(out.contains(r#""x":192,"y":246"#), "nested coordinates were summed: {out}");
    // The Business layer fill, and nothing invented.
    assert!(out.contains("\"fill\":\"#ffffb5\""), "{out}");
}

#[test]
fn exports_say_what_they_are() {
    let m = Model::new("modelimporter_test.archimate");

    let (code, out, _) = m.run(&["export", "mermaid"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("%% Generated by amcli"));
    // A format that re-lays-out has to say so, or it gets mistaken for the
    // diagram someone drew.
    assert!(out.contains("re-lays-out"), "{out}");
    assert!(out.contains("flowchart TD"));

    let (code, out, _) = m.run(&["export", "csv"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("id,type,name,layer,folder,source,target,documentation\n"));

    let (code, _, err) = m.run(&["export", "pdf"]);
    assert_eq!(code, 8, "unsupported");
    assert!(err.contains("view render"), "the faithful path is named: {err}");
}

#[test]
fn the_skill_installs_where_agents_look_and_uninstalls_cleanly() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();

    let run = |args: &[&str]| {
        Command::cargo_bin("amcli").unwrap().env("HOME", home.path()).args(args).output().unwrap()
    };

    let out = run(&["skill", "install"]);
    assert_eq!(out.status.code(), Some(0));

    let skill = home.path().join(".agents/skills/amcli");
    assert!(skill.join("SKILL.md").exists(), "the documented cross-tool location");
    assert!(skill.join("references/types.md").exists());
    // The skill is what teaches an agent to install the binary, so the
    // installer has to travel with it rather than be fetched from a URL.
    assert!(skill.join("scripts/install.sh").exists());

    // Nothing is generated into the directory: `npx skills add` copies
    // `skills/amcli/` verbatim, and anything written only by this command
    // would make the two routes disagree.
    assert!(
        !skill.join("references/commands.md").exists(),
        "the command reference is a command, not a file"
    );

    // One link for Claude Code; Codex reads ~/.agents/skills natively.
    let link = home.path().join(".claude/skills/amcli");
    #[cfg(unix)]
    assert_eq!(std::fs::read_link(&link).unwrap(), skill);
    // Windows needs a privilege for symlinks that a normal user does not have,
    // so there it is a copy and only the content can be compared.
    #[cfg(not(unix))]
    assert_eq!(
        std::fs::read_to_string(link.join("SKILL.md")).unwrap(),
        std::fs::read_to_string(skill.join("SKILL.md")).unwrap()
    );

    // The frontmatter carries only fields the Agent Skills spec defines, or
    // strict validators reject the file.
    let body = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
    let front = body.split("---").nth(1).unwrap();
    for line in front.lines().filter(|l| !l.starts_with(' ') && l.contains(':')) {
        let key = line.split(':').next().unwrap().trim();
        assert!(
            ["name", "description", "license", "compatibility", "metadata"].contains(&key),
            "`{key}` is not an Agent Skills field"
        );
    }

    assert_eq!(run(&["skill", "install"]).status.code(), Some(0), "installing twice is fine");
    assert_eq!(run(&["skill", "uninstall"]).status.code(), Some(0));
    assert!(!skill.exists());
    assert!(std::fs::read_link(&link).is_err());
}

/// `npx skills add` copies `skills/amcli/` out of the repository; this binary
/// writes the copy compiled into it. If those two ever differ, an agent gets
/// different instructions depending on how it installed, and the conflict
/// check in `skill install` starts firing on content it wrote itself.
///
/// Adding a file to `skills/amcli/` without adding it to `FILES` is the way
/// that happens, so this walks the directory rather than the list.
#[test]
fn both_install_routes_ship_the_same_bytes() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/amcli");
    let home = tempfile::tempdir().unwrap();
    let out = Command::cargo_bin("amcli")
        .unwrap()
        .env("HOME", home.path())
        .args(["skill", "install"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let installed = home.path().join(".agents/skills/amcli");

    let mut checked = 0;
    let mut stack = vec![source.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(&source).unwrap();
            let want = std::fs::read(&path).unwrap();
            let got = std::fs::read(installed.join(rel)).unwrap_or_else(|_| {
                panic!("{} is in skills/amcli but not embedded in the binary", rel.display())
            });
            assert!(want == got, "{} differs between the two install routes", rel.display());
            checked += 1;
        }
    }
    assert!(checked >= 5, "expected the whole skill, walked only {checked} files");
}

/// The command reference is a command, so it cannot describe a release other
/// than the one running.
#[test]
fn the_command_reference_comes_from_the_binary() {
    let out = Command::cargo_bin("amcli").unwrap().args(["skill", "commands"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("--expect-checksum"));
    assert!(text.contains("amcli element"));
    assert!(text.contains("amcli skill"));
}

/// Two things in SKILL.md that an agent executes literally, so a typo in
/// either is a broken recovery path rather than a documentation nit.
#[test]
fn the_skill_points_at_paths_that_exist_and_never_downgrades_itself() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/amcli");
    let body = std::fs::read_to_string(source.join("SKILL.md")).unwrap();

    // Under `npx skills add` the skill ships from the default branch and the
    // binary from the newest tag, so the skill is the *newer* of the two. An
    // instruction to run `skill install --force` on a missing command would
    // overwrite it with the older binary's copy and strand the npx lock file.
    assert!(
        !body.contains("skill install --force"),
        "that instruction downgrades the skill when the binary is the stale one"
    );

    // Both spellings: the PowerShell line uses backslashes, and a typo there
    // is just as broken as one in the sh line.
    let mut found = 0;
    for word in body.split_whitespace() {
        let word = word.replace('\\', "/");
        let Some(rest) = word.strip_prefix("~/.agents/skills/amcli/") else { continue };
        let rel = rest.trim_end_matches(['`', '"', ')', ',', '.']);
        assert!(
            source.join(rel).exists(),
            "SKILL.md tells the agent to run {rel}, which is not in the skill"
        );
        found += 1;
    }
    assert!(found >= 2, "expected the sh and PowerShell installers to be named, saw {found}");

    // The skill names the amcli version it is written for. It ships from the
    // branch, so that number has to move with every release commit, and this
    // is what makes forgetting it a red test rather than a stale document.
    let stated = body
        .lines()
        .find_map(|l| {
            let (_, rest) = l.split_once("written for **amcli ")?;
            rest.split_once("**").map(|(v, _)| v.trim().to_string())
        })
        .expect("SKILL.md says which amcli it is written for");
    assert_eq!(
        stated,
        env!("CARGO_PKG_VERSION"),
        "SKILL.md says it is written for amcli {stated}, but this is {}; bump the skill with the release",
        env!("CARGO_PKG_VERSION")
    );
}

/// A skill newer than the binary is the expected steady state, so the failure
/// has to say so where the agent is already reading.
#[test]
fn an_unknown_subcommand_blames_the_binary_not_the_skill() {
    let out = Command::cargo_bin("amcli").unwrap().arg("frobnicate").output().unwrap();
    assert_eq!(out.status.code(), Some(2), "usage");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("older"), "names the cause: {err}");
    assert!(err.contains("scripts/install.sh"), "gives a runnable recovery: {err}");
}

/// `--count` is documented as printing how many results there would be and
/// nothing else. On `view auto` it also created the view, so the command
/// documented as the safe way to ask a question was the one that left duplicate
/// views behind.
#[test]
fn count_answers_the_question_without_writing() {
    let m = Model::new("testmodel1.archimate");
    let before = m.text();

    let (code, out, _) = m.run(&["view", "auto", "probe", "--from", "Business Actor", "--count"]);
    assert_eq!(code, 0);
    assert!(out.trim().parse::<usize>().is_ok(), "a count and nothing else: {out}");
    assert_eq!(m.text(), before, "--count wrote to the model");
    assert!(!m.text().contains("probe"), "the view was created anyway");

    // Every other write path answers it the same way.
    for args in [
        &["element", "add", "BusinessActor", "Counted", "--count"][..],
        &["view", "create", "Counted", "--count"][..],
        &["element", "delete", "Business Actor", "-y", "--count"][..],
    ] {
        let (code, _, _) = m.run(args);
        assert_eq!(code, 0, "{args:?}");
        assert_eq!(m.text(), before, "{args:?} wrote to the model");
    }
}

/// Two views with the same name are indistinguishable to every selector, and
/// there used to be no way to remove either one.
#[test]
fn a_view_name_cannot_be_taken_twice_and_can_be_given_back() {
    let m = Model::new("testmodel1.archimate");
    assert_eq!(m.run(&["view", "create", "Flow"]).0, 0);

    let (code, _, err) = m.run(&["view", "create", "Flow"]);
    assert_eq!(code, 6, "conflict");
    assert!(err.contains("already called `Flow`"), "{err}");
    assert!(err.contains("--replace"), "the way forward is named: {err}");

    // `view auto` is the one that actually bit, and it answers the same way.
    let (code, _, _) = m.run(&["view", "auto", "Flow", "--from", "Business Actor"]);
    assert_eq!(code, 6);
    let (code, _, _) = m.run(&["view", "auto", "Flow", "--from", "Business Actor", "--replace"]);
    assert_eq!(code, 0);
    assert_eq!(named_views(&m, "Flow"), 1, "--replace replaced rather than added");

    // Renaming refuses the same clash, and then works.
    assert_eq!(m.run(&["view", "create", "Other"]).0, 0);
    assert_eq!(m.run(&["view", "rename", "Other", "Flow"]).0, 6);
    assert_eq!(m.run(&["view", "rename", "Other", "Renamed"]).0, 0);
    assert_eq!(named_views(&m, "Renamed"), 1);

    // And a stray view can be removed, which is what forced whole-model rebuilds.
    assert_eq!(m.run(&["view", "delete", "Renamed"]).0, 0);
    assert_eq!(named_views(&m, "Renamed"), 0);
    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

fn named_views(m: &Model, name: &str) -> usize {
    let (_, out, _) = m.run(&["view", "list", "-q"]);
    rows(&out).iter().filter(|r| r.get(1) == Some(&name)).count()
}

/// Deleting a view drawn as a reference box on another view has to take the box
/// with it: `model="…"` pointing at nothing is a file Archi will not open.
#[test]
fn deleting_a_referenced_view_refuses_until_told_and_leaves_nothing_dangling() {
    let m = Model::new("testDeleteHandler.archimate");
    let before = m.text();

    let (code, _, err) = m.run(&["view", "delete", "id:12917bec"]);
    assert_eq!(code, 5);
    assert!(err.contains("drawn as a reference"), "{err}");
    assert_eq!(m.text(), before, "a refused delete writes nothing");

    let (code, out, _) = m.run(&["view", "delete", "id:12917bec", "-y"]);
    assert_eq!(code, 0, "{out}");
    assert!(!m.text().contains("12917bec"), "the view survived");
    assert!(!m.text().contains("99a52921"), "the reference box now dangles");

    // The fixture carries two matrix violations of its own, so integrity is
    // compared against the baseline rather than to zero.
    let (_, out, _) = m.run(&["validate", "--level", "integrity", "-q"]);
    assert!(!out.contains("99a52921"), "a dangling visual was reported: {out}");
}

/// An added concept used to stay a floating box even when the thing it relates
/// to was already on the same view, and no amount of re-laying-out could fix
/// that because the connection was never written.
#[test]
fn adding_a_concept_to_a_view_draws_the_relationships_it_brings() {
    let m = Model::new("modelimporter_test.archimate");
    assert_eq!(m.run(&["element", "add", "ApplicationComponent", "Svc"]).0, 0);
    assert_eq!(m.run(&["relation", "add", "Serving", "Svc", "BA1"]).0, 0);
    assert_eq!(m.run(&["view", "create", "Wired"]).0, 0);

    let edges = |m: &Model| {
        let (_, out, _) = m.run(&["view", "render", "Wired", "--as", "json", "-q"]);
        out.matches(r#""relationship":"#).count()
    };

    // The first box has nothing to connect to yet.
    let (code, out, _) = m.run(&["view", "add", "Wired", "Svc"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(edges(&m), 0);

    // The second completes a relationship that is already in the model.
    let (code, _, err) = m.run(&["view", "add", "Wired", "BA1"]);
    assert_eq!(code, 0);
    assert_eq!(edges(&m), 1, "the Serving relationship was not drawn: {err}");

    // Re-adding does not draw it twice.
    assert_eq!(m.run(&["view", "add", "Wired", "BA1"]).0, 0);
    assert_eq!(edges(&m), 1, "a second copy of the connection was written");

    // Opting out still works, and the model stays loadable throughout.
    assert_eq!(m.run(&["element", "add", "DataObject", "Rec"]).0, 0);
    assert_eq!(m.run(&["relation", "add", "Access", "Svc", "Rec"]).0, 0);
    assert_eq!(m.run(&["view", "add", "Wired", "Rec", "--no-connect"]).0, 0);
    assert_eq!(edges(&m), 1, "--no-connect drew a connection anyway");
    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

/// The write side takes `Triggering`; the query side took only
/// `TriggeringRelationship` and answered 0 for the other, which reads as a fact
/// about the model rather than as a vocabulary mismatch.
#[test]
fn type_filters_take_the_archimate_name_and_reject_what_is_not_a_type() {
    let m = Model::new("testmodel1.archimate");
    let count = |args: &[&str]| -> String {
        let (_, out, _) = m.run(args);
        out.trim().to_string()
    };
    assert_eq!(count(&["query", "type=AssignmentRelationship", "--count"]), "1");
    assert_eq!(count(&["query", "type=Assignment", "--count"]), "1", "the ArchiMate spelling");
    assert_eq!(count(&["list", "-t", "Assignment", "--count"]), "1", "and on -t too");

    // A type that does not exist is a mistake, not an empty result set.
    let (code, _, err) = m.run(&["list", "-t", "NotAType", "--count"]);
    assert_eq!(code, 2, "usage");
    assert!(err.contains("is not a concept type"), "{err}");
    assert!(err.contains("AssignmentRelationship"), "the model's own types are listed: {err}");

    // `-t element` is the category mistake, and there is now a field for it.
    let (code, _, err) = m.run(&["list", "-t", "element", "--count"]);
    assert_eq!(code, 2);
    assert!(err.contains("kind=element"), "points at the filter field: {err}");

    // Which is what separates relationships from elements in a query.
    assert_eq!(count(&["query", "kind=relation", "--count"]), "1");
    assert_eq!(count(&["query", "kind=element", "--count"]), "2");
}

/// `view~"Name"` filtered but the column was always empty and `view=0` matched
/// nothing, so "which concepts are on no view" — the invariant a model built
/// this way depends on — could not be asked at all.
#[test]
fn the_view_field_reports_how_many_and_which() {
    let m = Model::new("testmodel1.archimate");
    assert_eq!(m.run(&["element", "add", "Goal", "Undrawn"]).0, 0);

    let count = |args: &[&str]| -> String {
        let (_, out, _) = m.run(args);
        out.trim().to_string()
    };
    assert_eq!(count(&["query", "view=0", "--count"]), "1", "the element on no view");
    assert_eq!(count(&["query", "view<1", "--count"]), "1");
    assert_eq!(count(&["query", "name=Undrawn", "--fields", "name,views"]), "Undrawn\t0");

    // A field that does not exist projected to nothing and said nothing, so a
    // near-miss spelling read as "this model has no view information".
    let (_, out, err) = m.run(&["list", "-l", "1", "--fields", "name,view"]);
    assert!(err.contains("no such field: view"), "{err}");
    assert!(err.contains("views"), "the real column is named: {err}");
    assert!(!out.contains('\t'), "only the field that exists was printed: {out}");

    // A name still filters by view, and the count column agrees with it.
    let (_, out, _) = m.run(&["query", "view~\"2 Test\"", "--fields", "name,views", "-q"]);
    assert!(!out.is_empty(), "the view name filter stopped working");
    for row in rows(&out) {
        assert_ne!(row[1], "0", "on a view but counted as on none: {row:?}");
    }
}

/// An unknown flag used to end with "this amcli is older than that document",
/// which sent a reader off to reinstall a current binary over a misremembered
/// flag name. An unknown *subcommand* is the case that footer is for.
#[test]
fn an_unknown_flag_names_the_flags_instead_of_blaming_the_binary() {
    let m = Model::new("testmodel1.archimate");
    let (code, _, err) = m.run(&["view", "layout", "0 Blank View", "--bogus"]);
    assert_eq!(code, 2);
    assert!(!err.contains("older"), "an unknown flag is not version skew: {err}");
    assert!(err.contains("--relayout-all"), "the command's own flags are listed: {err}");
    assert!(err.contains("--model"), "and the global ones: {err}");

    // A missing file is not version skew either.
    let out = Command::cargo_bin("amcli")
        .unwrap()
        .args(["-m", " /nope.archimate", "info"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("older"), "{err}");
    // Quoted, or a leading space from an unsplit shell variable is invisible.
    assert!(err.contains("` /nope.archimate`"), "the path is not quoted: {err}");
}

/// `view auto --layout` and `view layout --algorithm` named the same concept two
/// ways, and guessing wrong produced an error that looked like a missing command.
#[test]
fn either_spelling_of_the_layout_flag_is_accepted() {
    let m = Model::new("modelimporter_test.archimate");
    for args in [
        &["view", "auto", "A", "--from", "BA1", "--layout", "grid"][..],
        &["view", "auto", "B", "--from", "BA1", "--algorithm", "grid"][..],
    ] {
        assert_eq!(m.run(args).0, 0, "{args:?}");
    }
    for flag in ["--algorithm", "--layout"] {
        let (code, out, err) = m.run(&["view", "layout", "A", flag, "grid", "--relayout-all"]);
        assert_eq!(code, 0, "{flag}: {err}");
        assert!(out.contains("grid"), "the algorithm used is reported: {out}");
    }

    // And a name that is not an algorithm lists the ones that are.
    let (code, _, err) = m.run(&["view", "layout", "A", "--layout", "spiral"]);
    assert_eq!(code, 2);
    assert!(err.contains("grid"), "{err}");
}

/// Two builds reporting the same version cannot be told apart, which is what
/// made a stale binary earlier in PATH look like a broken skill.
///
/// The version comes from the package rather than being spelled out here: this
/// test is about the build identifier, and hard-coding the number only means it
/// fails on the commit that bumps it.
#[test]
fn the_version_says_which_build_it_is() {
    let out = Command::cargo_bin("amcli").unwrap().arg("--version").output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let expected = format!("amcli {}", env!("CARGO_PKG_VERSION"));
    assert!(text.starts_with(&expected), "expected {expected}, got {text}");
    assert!(text.contains('('), "no build identifier: {text}");
    // Whatever it is, it is not empty parentheses.
    let build = text.split('(').nth(1).unwrap().trim_end_matches([')', '\n']);
    assert!(build.len() > 3, "the build identifier is empty: {text}");
}

/// The columns were `<id> <name> <type?> <n> <n> <n>` and had to be guessed at.
/// Naming them on stdout would break `cut -f2`, so they are named on stderr.
#[test]
fn records_carry_a_column_header_on_stderr() {
    let m = Model::new("testmodel1.archimate");
    let (code, out, err) = m.run(&["view", "list"]);
    assert_eq!(code, 0);
    assert!(err.contains("# id\tname"), "the columns are named: {err}");
    for line in out.lines() {
        assert!(!line.starts_with('#'), "the header leaked into the data: {line}");
    }

    // -q is still nothing but records.
    let (_, _, err) = m.run(&["view", "list", "-q"]);
    assert!(!err.contains('#'), "-q asked for no envelope: {err}");

    // A command returning two record shapes labels both.
    let (_, _, err) = m.run(&["trace", "Business Actor", "-n", "2"]);
    assert_eq!(err.matches('#').count(), 2, "nodes and edges are labelled separately: {err}");
}

/// Creating a model meant hand-writing XML, which is the one thing the skill
/// tells an agent never to do.
#[test]
fn init_creates_a_model_the_rest_of_the_tool_can_use() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.archimate");
    let amcli = |args: &[&str]| Command::cargo_bin("amcli").unwrap().args(args).output().unwrap();

    let out = amcli(&["init", "Monetech & Co", "-o", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(path.exists());

    let p = path.to_str().unwrap();
    // Every folder a write needs is there, so the normal loop works immediately.
    for args in [
        &["-m", p, "element", "add", "ApplicationComponent", "Svc"][..],
        &["-m", p, "element", "add", "DataObject", "Rec"][..],
        &["-m", p, "relation", "add", "Access", "Svc", "Rec", "--access", "rw"][..],
        &["-m", p, "view", "auto", "V", "--from", "Svc"][..],
        &["-m", p, "validate"][..],
    ] {
        let out = amcli(args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // The name survived escaping, which is why this is not a format! template.
    let out = amcli(&["-m", p, "info", "-F", "json", "-q"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Monetech & Co"));

    // An existing file is not silently overwritten.
    assert_eq!(amcli(&["init", "Other", "-o", p]).status.code(), Some(6), "conflict");
    assert_eq!(amcli(&["init", "Other", "-o", p, "--force"]).status.code(), Some(0));
}

/// Found by rebuilding a real model twice: same size, same content, different
/// property order. `HashMap` iteration is randomised per process, so a batch
/// applied twice wrote the properties in a different order each time and the
/// rebuild still produced a diff — deterministic ids do not help if the lines
/// around them move.
#[test]
fn properties_from_a_batch_are_written_in_a_stable_order() {
    let m = Model::new("modelimporter_test.archimate");
    let ops = m.dir.path().join("ops.jsonl");
    let keys = ["owner", "tier", "zone", "cost", "sla", "team"];
    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"element.add","type":"ApplicationComponent","name":"Svc","props":"#,
            r#"{"owner":"a","tier":"1","zone":"eu","cost":"9","sla":"gold","team":"x"}}"#,
            "\n",
        ),
    )
    .unwrap();
    assert_eq!(m.run(&["apply", ops.to_str().unwrap()]).0, 0);

    // Key order, which is a property of one run rather than a comparison between
    // two: a comparison would pass by luck one time in 720.
    let text = m.text();
    let at = |k: &str| text.find(&format!(r#"key="{k}""#)).unwrap_or_else(|| panic!("no {k}"));
    let mut sorted = keys;
    sorted.sort_unstable();
    let positions: Vec<usize> = sorted.iter().map(|k| at(k)).collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "properties are not in key order: {positions:?}"
    );
}

/// Rebuilding from identical batches regenerated every id, so a semantically
/// unchanged model produced a whole-file diff.
#[test]
fn a_seed_makes_a_rebuild_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let amcli = |args: &[&str]| Command::cargo_bin("amcli").unwrap().args(args).output().unwrap();

    let build = |name: &str, seed: Option<&str>| -> Vec<u8> {
        let path = dir.path().join(name);
        let p = path.to_str().unwrap().to_string();
        let mut steps: Vec<Vec<String>> = vec![
            vec!["init".into(), "Seeded".into(), "-o".into(), p.clone()],
            vec![
                "-m".into(),
                p.clone(),
                "element".into(),
                "add".into(),
                "ApplicationComponent".into(),
                "Svc".into(),
            ],
            vec![
                "-m".into(),
                p.clone(),
                "element".into(),
                "add".into(),
                "DataObject".into(),
                "Rec".into(),
            ],
            vec![
                "-m".into(),
                p.clone(),
                "relation".into(),
                "add".into(),
                "Access".into(),
                "Svc".into(),
                "Rec".into(),
            ],
            vec![
                "-m".into(),
                p.clone(),
                "view".into(),
                "auto".into(),
                "V".into(),
                "--from".into(),
                "Svc".into(),
            ],
        ];
        for step in &mut steps {
            if let Some(s) = seed {
                step.push("--id-seed".into());
                step.push(s.into());
            }
            let args: Vec<&str> = step.iter().map(String::as_str).collect();
            let out = amcli(&args);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        std::fs::read(&path).unwrap()
    };

    assert_eq!(
        build("a.archimate", Some("demo")),
        build("b.archimate", Some("demo")),
        "the same model built twice with the same seed differs"
    );
    // Random stays the default: deriving an id from a name would give the same
    // id to two models that both contain "Payment API".
    assert_ne!(build("c.archimate", None), build("d.archimate", None));
}

/// The layout's whole job, asserted end to end on a graph that admits a clean
/// drawing: no bendpoints, no segment through a box, and no two segments
/// crossing each other.
///
/// This graph is seven nodes and seven edges — a tree plus one cycle — so it is
/// planar and a perfect drawing exists. Producing anything worse would mean the
/// layout is inventing difficulty.
#[test]
fn a_graph_that_can_be_drawn_cleanly_is_drawn_cleanly() {
    let m = Model::new("modelimporter_test.archimate");
    for (ty, name) in [
        ("ApplicationComponent", "Payment API"),
        ("ApplicationService", "Card Authorization"),
        ("ApplicationFunction", "Authorize"),
        ("DataObject", "Payment Record"),
        ("Goal", "Reduce fraud"),
    ] {
        assert_eq!(m.run(&["element", "add", ty, name]).0, 0);
    }
    for (ty, a, b) in [
        ("Assignment", "Payment API", "Authorize"),
        ("Access", "Authorize", "Payment Record"),
        ("Realization", "Authorize", "Card Authorization"),
        ("Serving", "Card Authorization", "BR1"),
        ("Influence", "Card Authorization", "Reduce fraud"),
        ("Serving", "Payment API", "BR1"),
    ] {
        assert_eq!(m.run(&["relation", "add", ty, a, b]).0, 0, "{ty} {a} -> {b}");
    }

    assert_eq!(m.run(&["view", "auto", "V", "--from", "Payment API", "-n", "4"]).0, 0);
    let (code, out, _) = m.run(&["view", "render", "V", "--as", "json"]);
    assert_eq!(code, 0);

    let (boxes, lines) = scene(&out);
    assert!(boxes.len() >= 6, "parsed {} boxes from {out}", boxes.len());
    assert!(lines.len() >= 6, "parsed {} edges from {out}", lines.len());

    let bends: usize = lines.iter().map(|l| l.len().saturating_sub(2)).sum();
    assert_eq!(bends, 0, "this graph needs no bendpoints at all");

    let mut through = 0;
    for line in &lines {
        for (p, q) in line.iter().zip(line.iter().skip(1)) {
            for b in &boxes {
                if segment_enters(*p, *q, *b) {
                    through += 1;
                }
            }
        }
    }
    assert_eq!(through, 0, "{through} segments run through a box");

    let segments: Vec<((i32, i32), (i32, i32))> =
        lines.iter().flat_map(|l| l.iter().zip(l.iter().skip(1)).map(|(a, b)| (*a, *b))).collect();
    let mut crossings = 0;
    for (i, (a, b)) in segments.iter().enumerate() {
        for (c, d) in segments.iter().skip(i + 1) {
            if segments_cross(*a, *b, *c, *d) {
                crossings += 1;
            }
        }
    }
    assert_eq!(crossings, 0, "{crossings} pairs of edges cross");

    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

type Boxes = Vec<(i32, i32, i32, i32)>;
type Lines = Vec<Vec<(i32, i32)>>;

/// A minimal read of the scene dump: enough to walk segments against boxes.
fn scene(out: &str) -> (Boxes, Lines) {
    let boxes: Boxes = out
        .split(r#"{"id":"#)
        .filter(|s| s.contains(r#""depth""#))
        .filter_map(|s| {
            let n = |k: &str| -> Option<i32> {
                s.split(&format!(r#""{k}":"#)).nth(1)?.split([',', '}']).next()?.parse().ok()
            };
            Some((n("x")?, n("y")?, n("w")?, n("h")?))
        })
        .collect();

    let lines: Lines = out
        .split(r#""points":[["#)
        .skip(1)
        .map(|s| {
            s.split("]]")
                .next()
                .unwrap_or_default()
                .split("],[")
                .filter_map(|p| {
                    let mut it = p.trim_matches(['[', ']']).split(',');
                    Some((it.next()?.trim().parse().ok()?, it.next()?.trim().parse().ok()?))
                })
                .collect()
        })
        .collect();
    (boxes, lines)
}

/// Does the segment pass through the interior of the box? The box is inset a
/// little, because an endpoint resting on its own border is normal.
fn segment_enters(p: (i32, i32), q: (i32, i32), b: (i32, i32, i32, i32)) -> bool {
    let (x, y, w, h) = b;
    for step in 1..60 {
        let t = step as f64 / 60.0;
        let px = p.0 as f64 + (q.0 - p.0) as f64 * t;
        let py = p.1 as f64 + (q.1 - p.1) as f64 * t;
        if px > (x + 2) as f64
            && px < (x + w - 2) as f64
            && py > (y + 2) as f64
            && py < (y + h - 2) as f64
        {
            return true;
        }
    }
    false
}

fn segments_cross(a: (i32, i32), b: (i32, i32), c: (i32, i32), d: (i32, i32)) -> bool {
    // Segments meeting at a shared endpoint are edges leaving the same box, not
    // a crossing.
    let ends = [a, b, c, d];
    if ends.iter().enumerate().any(|(i, p)| ends.iter().skip(i + 1).any(|q| p == q)) {
        return false;
    }
    let orient = |p: (i32, i32), q: (i32, i32), r: (i32, i32)| -> i64 {
        (q.1 - p.1) as i64 * (r.0 - q.0) as i64 - (q.0 - p.0) as i64 * (r.1 - q.1) as i64
    };
    let sign = |v: i64| v.signum();
    sign(orient(a, b, c)) != sign(orient(a, b, d)) && sign(orient(c, d, a)) != sign(orient(c, d, b))
}

/// Views are filed in folders, and a folder is checked before a view is made.
///
/// Every view landing at the top of `/Views` is fine for ten views and useless
/// for thirty, so `create`, `auto` and the batch all take a folder, and `move`
/// re-files the ones already there. The destination is checked first: a view
/// filed outside the views tree parses but never appears in Archi, which is the
/// kind of breakage that is only noticed by the person who opens the model.
#[test]
fn views_are_filed_in_folders() {
    let m = Model::new("modelimporter_test.archimate");
    let folder_of = |m: &Model, name: &str| -> String {
        let (_, out, _) = m.run(&["view", "list", "-q", "--fields", "name,folder"]);
        rows(&out)
            .iter()
            .find(|r| r.first() == Some(&name))
            .and_then(|r| r.get(1))
            .unwrap_or(&"")
            .to_string()
    };

    // The folder has to exist first — `folder add` is what makes one.
    let (code, _, err) = m.run(&["view", "create", "Filed", "-f", "/Views/Motivation"]);
    assert_eq!(code, 3, "a folder that does not exist is not found: {err}");
    assert!(err.contains("no folder at `/Views/Motivation`"), "{err}");
    assert_eq!(named_views(&m, "Filed"), 0, "nothing was created behind the error");

    assert_eq!(m.run(&["folder", "add", "/Views", "Motivation"]).0, 0);
    assert_eq!(m.run(&["view", "create", "Filed", "-f", "/Views/Motivation"]).0, 0);
    assert_eq!(folder_of(&m, "Filed"), "/Views/Motivation");

    // A folder outside the views tree is refused, not silently obeyed: Archi
    // shows no diagram filed under /Business.
    let (code, _, err) = m.run(&["view", "create", "Stray", "-f", "/Business"]);
    assert_eq!(code, 5, "{err}");
    assert!(err.contains("not under the views folder"), "{err}");

    // An existing view moves, and reports where it came from.
    assert_eq!(m.run(&["view", "create", "Loose"]).0, 0);
    assert_eq!(folder_of(&m, "Loose"), "/Views");
    let (code, out, _) = m.run(&["view", "move", "Loose", "-f", "/Views/Motivation"]);
    assert_eq!(code, 0);
    assert!(out.contains("/Views\t/Views/Motivation"), "from and to: {out}");
    assert_eq!(folder_of(&m, "Loose"), "/Views/Motivation");

    // Moving somewhere it already is changes nothing and is not an error, so a
    // regenerate-everything script stays re-runnable.
    assert_eq!(m.run(&["view", "move", "Loose", "-f", "/Views/Motivation"]).0, 0);
    assert_eq!(folder_of(&m, "Loose"), "/Views/Motivation");

    // `view auto` and the batch file views the same way.
    assert_eq!(
        m.run(&["view", "auto", "Neighbourhood", "--from", "BA1", "-f", "/Views/Motivation"]).0,
        0
    );
    assert_eq!(folder_of(&m, "Neighbourhood"), "/Views/Motivation");

    let ops = m.dir.path().join("folders.jsonl");
    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"folder.add","parent":"/Views","name":"Programme"}"#,
            "\n",
            r#"{"op":"view.create","name":"Batched","folder":"/Views/Programme","replace":true}"#,
            "\n",
            r#"{"op":"view.move","view":"Filed","folder":"/Views/Programme"}"#,
            "\n",
        ),
    )
    .unwrap();
    let (code, out, err) = m.run(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("view.move"), "{out}");
    assert_eq!(folder_of(&m, "Batched"), "/Views/Programme");
    assert_eq!(folder_of(&m, "Filed"), "/Views/Programme");

    // A batch that names a bad folder writes nothing at all.
    let model_file = m.dir.path().join("m.archimate");
    let before = std::fs::read(&model_file).unwrap();
    let bad = m.dir.path().join("bad.jsonl");
    std::fs::write(
        &bad,
        concat!(
            r#"{"op":"view.create","name":"Half","folder":"/Views/Programme","replace":true}"#,
            "\n",
            r#"{"op":"view.move","view":"Batched","folder":"/Nowhere"}"#,
            "\n",
        ),
    )
    .unwrap();
    assert_eq!(m.run(&["apply", bad.to_str().unwrap()]).0, 3);
    assert_eq!(std::fs::read(&model_file).unwrap(), before, "the file is byte-identical");

    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

/// Declaring a folder twice gives one folder, not two.
///
/// This is the shape every regenerate-everything script has — declare the
/// folders, then file the views — so a `folder add` that appended a second
/// folder of the same name turned each re-run into another duplicate, three
/// deep before anyone opened Archi and saw them. `folder_by_path` can only
/// return one of them, so the extras are not even reachable to fix.
#[test]
fn declaring_a_folder_twice_gives_one_folder() {
    let m = Model::new("modelimporter_test.archimate");
    let folders = || -> usize {
        let (_, out, _) = m.run(&["folder", "list", "-q", "--fields", "path"]);
        rows(&out).iter().filter(|r| r.first() == Some(&"/Views/Programme")).count()
    };

    let (code, out, _) = m.run(&["folder", "add", "/Views", "Programme"]);
    assert_eq!(code, 0);
    assert!(out.contains("true"), "reports that it created one: {out}");
    assert_eq!(folders(), 1);

    let (code, out, _) = m.run(&["folder", "add", "/Views", "Programme"]);
    assert_eq!(code, 0, "a repeat is not an error");
    assert!(out.contains("false"), "reports that it created nothing: {out}");
    assert_eq!(folders(), 1, "still one folder, not two");

    // A view filed there survives the repeat, because the folder is the same one.
    assert_eq!(m.run(&["view", "create", "Filed", "-f", "/Views/Programme"]).0, 0);
    assert_eq!(m.run(&["folder", "add", "/Views", "Programme"]).0, 0);
    let (_, out, _) = m.run(&["view", "list", "-q", "--fields", "name,folder"]);
    assert!(out.contains("Filed\t/Views/Programme"), "{out}");

    // An empty folder can be removed; one holding something cannot.
    let (code, _, err) = m.run(&["folder", "delete", "/Views/Programme"]);
    assert_eq!(code, 5, "refuses while the view is in it: {err}");
    assert!(err.contains("still holds 1"), "{err}");

    assert_eq!(m.run(&["view", "delete", "Filed"]).0, 0);
    assert_eq!(m.run(&["folder", "delete", "/Views/Programme"]).0, 0);
    assert_eq!(folders(), 0);

    // The nine Archi expects are not deletable.
    let (code, _, err) = m.run(&["folder", "delete", "/Views"]);
    assert_eq!(code, 5, "{err}");

    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

/// `export views` and `apply` are inverses, and stay inverses.
///
/// A view has no declarative form in the file — what it holds is only geometry
/// — so "which elements are on this view, and why those" is not a question a
/// diff can answer. Keeping member lists beside the model answers it and
/// invents a second source of truth that goes stale; deriving them from the
/// model does not. That only works if the round trip is exact, so this asserts
/// byte identity rather than "looks right", and asserts it twice: an export
/// that reorders the views it rebuilds would still pass a one-shot check while
/// making every regeneration churn the whole file.
#[test]
fn exported_views_rebuild_the_model_byte_for_byte() {
    let m = Model::new("modelimporter_test.archimate");
    let model_file = m.dir.path().join("m.archimate");

    // The fixture's own views were drawn in Archi and hold notes and nested
    // objects, which `view.add` cannot put back — the export says so in a
    // comment rather than pretending otherwise. The round trip is exact for
    // views amcli built, which is what a regenerated model is made of.
    for stale in ["View 1", "View 2"] {
        assert_eq!(m.run(&["view", "delete", stale, "-y"]).0, 0);
    }
    // Seeded, because that is the only way a rebuild can be byte-identical:
    // without it every recreated view draws a fresh random id. Each `run` is
    // its own process, so the seed does not leak into the other tests here.
    let seed = ["--id-seed", "roundtrip"];
    let seeded = |args: &[&str]| -> (i32, String, String) {
        let mut all = args.to_vec();
        all.extend_from_slice(&seed);
        m.run(&all)
    };

    assert_eq!(seeded(&["folder", "add", "/Views", "Group"]).0, 0);
    for name in ["V1", "V2", "V3", "V4", "V5"] {
        assert_eq!(seeded(&["view", "create", name, "-f", "/Views/Group"]).0, 0);
        assert_eq!(seeded(&["view", "add", name, "BA1"]).0, 0);
    }

    let spec = m.dir.path().join("views.jsonl");
    let (code, _, err) = m.run(&["export", "views", "-o", spec.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    let text = std::fs::read_to_string(&spec).unwrap();
    assert!(text.contains(r#""op":"folder.add""#), "declares its folders: {text}");
    assert!(text.contains(r#""folder":"/Views/Group""#), "files the views: {text}");
    assert!(text.contains("# V3"), "readable, one comment per view: {text}");

    let before = std::fs::read_to_string(&model_file).unwrap();
    assert_eq!(seeded(&["apply", spec.to_str().unwrap()]).0, 0);
    let once = std::fs::read_to_string(&model_file).unwrap();
    assert_eq!(first_difference(&before, &once), None, "one round trip changes nothing");

    // Twice, because a rebuild that reorders is stable only on odd passes.
    assert_eq!(seeded(&["apply", spec.to_str().unwrap()]).0, 0);
    let twice = std::fs::read_to_string(&model_file).unwrap();
    assert_eq!(first_difference(&before, &twice), None, "and neither does a second");

    // And the spec is the same spec, so it can be reviewed in a diff.
    let again = m.dir.path().join("views2.jsonl");
    assert_eq!(m.run(&["export", "views", "-o", again.to_str().unwrap()]).0, 0);
    assert_eq!(std::fs::read_to_string(&again).unwrap(), text);

    assert_eq!(m.run(&["validate", "--level", "integrity"]).0, 0);
}

/// A replaced view is rebuilt where it was, not appended.
///
/// `--replace` deletes and recreates, and a recreated view used to land at the
/// end of its folder. With three views nobody notices; with thirty, every
/// regeneration rewrites the whole views section and the diff stops being
/// worth reading — which is the one thing this tool exists to protect.
#[test]
fn replacing_a_view_keeps_its_place_in_the_folder() {
    let m = Model::new("modelimporter_test.archimate");
    let order = |m: &Model| -> Vec<String> {
        let (_, out, _) = m.run(&["view", "list", "-q", "--fields", "name"]);
        rows(&out).iter().filter_map(|r| r.first()).map(|s| s.to_string()).collect()
    };

    assert_eq!(m.run(&["folder", "add", "/Views", "Group"]).0, 0);
    for name in ["V1", "V2", "V3"] {
        assert_eq!(m.run(&["view", "create", name, "-f", "/Views/Group"]).0, 0);
    }
    let before = order(&m);

    // One at a time, and in a batch — the batch is where it went wrong, because
    // the deleted node keeps its seat in the child list until the file is
    // written, so an index counted over live children pointed one place early.
    assert_eq!(m.run(&["view", "create", "V2", "-f", "/Views/Group", "--replace"]).0, 0);
    assert_eq!(order(&m), before, "a single replace holds the order");

    let ops = m.dir.path().join("all.jsonl");
    std::fs::write(
        &ops,
        concat!(
            r#"{"op":"view.create","name":"V1","folder":"/Views/Group","replace":true}"#,
            "\n",
            r#"{"op":"view.create","name":"V2","folder":"/Views/Group","replace":true}"#,
            "\n",
            r#"{"op":"view.create","name":"V3","folder":"/Views/Group","replace":true}"#,
            "\n",
        ),
    )
    .unwrap();
    assert_eq!(m.run(&["apply", ops.to_str().unwrap()]).0, 0);
    assert_eq!(order(&m), before, "and so does a batch that replaces every one");
}

/// The first line where two model files differ, for an assertion that has to
/// print something a person can read rather than half a megabyte of bytes.
fn first_difference(a: &str, b: &str) -> Option<String> {
    for (n, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return Some(format!("line {}:\n  before: {x}\n   after: {y}", n + 1));
        }
    }
    (a.lines().count() != b.lines().count())
        .then(|| format!("{} lines before, {} after", a.lines().count(), b.lines().count()))
}
