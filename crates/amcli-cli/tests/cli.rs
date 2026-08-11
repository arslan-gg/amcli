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
    // Generated from the command tree, so it cannot describe a version of amcli
    // that is not this one.
    let commands = std::fs::read_to_string(skill.join("references/commands.md")).unwrap();
    assert!(commands.contains("--expect-checksum"));
    assert!(commands.contains("amcli element"));

    // One symlink for Claude Code; Codex reads ~/.agents/skills natively.
    let link = home.path().join(".claude/skills/amcli");
    assert_eq!(std::fs::read_link(&link).unwrap(), skill);

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
