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
