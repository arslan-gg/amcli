//! `amcli web` — the model in a browser, read-only.
//!
//! Binds the loopback interface on a free port, prints the URL, opens the
//! browser unless told not to, and serves until interrupted. Nothing here
//! writes: the page has no verb but GET, and the process holds the file open
//! only long enough to read it, so an agent may keep editing the model with
//! amcli while a person watches it change.
//!
//! The URL is the command's output, printed through the ordinary printer —
//! so `-q` gives just the URL and `-F json` the usual envelope — and *then*
//! the server runs, as the continuation `Output::then`. That ordering is the
//! whole trick: whoever started the process reads one line and has the link
//! while the process goes on serving.

mod api;
mod http;
mod state;

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use amcli_model::Model;

use crate::output::{CliError, Code, Output, Row};

pub fn run(
    model: Model,
    path: PathBuf,
    port: Option<u16>,
    no_open: bool,
) -> Result<Output, CliError> {
    let listener = TcpListener::bind(("127.0.0.1", port.unwrap_or(0))).map_err(|e| {
        CliError::new(
            Code::Io,
            "io",
            format!("cannot listen on 127.0.0.1:{}: {e}", port.unwrap_or(0)),
        )
        .hint("pass another --port, or none to let the OS pick a free one")
    })?;
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let url = format!("http://127.0.0.1:{port}/");

    let state = Arc::new(state::State::new(model, path.clone(), port));
    let name = state.current().model.name();

    let out = Output::one(Row::new().s("url", url.clone()).s("model", path.display().to_string()))
        .note(format!(
            "serving `{name}` at {url} — read-only; the page follows the file; Ctrl-C stops it"
        ));
    let out = if no_open {
        out
    } else {
        out.note("opening the browser; --no-open to just print the URL")
    };

    let mut out = out;
    out.then = Some(Box::new(move || {
        if !no_open {
            open_browser(&url);
        }
        http::serve(listener, state);
    }));
    Ok(out)
}

/// Hand the URL to the desktop's opener and do not wait for it. Failure is
/// not an error: the URL has already been printed, and that is the fallback.
fn open_browser(url: &str) {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    let _ = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;

    use super::state::State;

    fn corpus(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name)
    }

    /// A server on a free port over a scratch copy of a corpus model.
    fn start(fixture: &str) -> (u16, Arc<State>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.archimate");
        std::fs::copy(corpus(fixture), &path).unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let model = amcli_model::Model::open(&path).unwrap();
        let state = Arc::new(State::new(model, path, port));
        let served = Arc::clone(&state);
        std::thread::spawn(move || super::http::serve(listener, served));
        (port, state, dir)
    }

    fn get(port: u16, path: &str) -> (u16, String, String) {
        request(port, &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"))
    }

    fn request(port: u16, raw: &str) -> (u16, String, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).unwrap();
        // A body may be a PNG, so only the head is required to be text.
        let at = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8(buf[..at].to_vec()).unwrap();
        let body = String::from_utf8_lossy(&buf[at + 4..]).into_owned();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, head, body)
    }

    #[test]
    fn serves_the_page_the_model_and_a_view() {
        let (port, _state, _dir) = start("testmodel1.archimate");

        let (status, head, body) = get(port, "/");
        assert_eq!(status, 200);
        assert!(head.contains("text/html"), "{head}");
        assert!(head.contains("Content-Security-Policy"), "{head}");
        assert!(body.contains("<script type=\"module\""), "{body}");

        let (status, _, body) = get(port, "/api/model");
        assert_eq!(status, 200);
        let json: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let elements = json["elements"].as_array().unwrap();
        let relations = json["relations"].as_array().unwrap();
        let views = json["views"].as_array().unwrap();
        assert!(!elements.is_empty() && !relations.is_empty() && !views.is_empty());
        assert_eq!(json["types"]["BusinessActor"]["fill"], "#ffffb5");
        assert!(json["types"]["BusinessActor"]["icon"].as_str().unwrap().starts_with('M'));
        assert_eq!(json["relTypes"]["Serving"]["target"], "arrow-open");
        // A relationship's ends index into the elements array.
        let r = &relations[0];
        let src = r["src"].as_i64().unwrap();
        assert!(src >= -1 && (src as usize) < elements.len() || src == -1);

        let view_id = views[0]["id"].as_str().unwrap();
        let (status, head, body) = get(port, &format!("/api/view/{view_id}.svg"));
        assert_eq!(status, 200);
        assert!(head.contains("image/svg+xml"), "{head}");
        assert!(body.starts_with("<svg xmlns="), "{body}");
        assert!(!body.contains(" width=\""), "unsized, so the page can fit it: {}", &body[..80]);

        let (status, head, _) = get(port, &format!("/api/view/{view_id}.png"));
        assert_eq!(status, 200);
        assert!(head.contains("image/png"), "{head}");

        let elem_id = elements[0]["id"].as_str().unwrap();
        let (status, _, body) = get(port, &format!("/api/concept/{elem_id}"));
        assert_eq!(status, 200);
        assert!(body.contains("\"properties\":["), "{body}");

        let (status, _, body) = get(port, "/api/status");
        assert_eq!(status, 200);
        assert!(body.contains("\"error\":null"), "{body}");
    }

    #[test]
    fn refuses_what_it_does_not_serve() {
        let (port, _state, _dir) = start("testmodel1.archimate");
        assert_eq!(get(port, "/nope").0, 404);
        assert_eq!(get(port, "/api/view/nope.svg").0, 404);
        assert_eq!(get(port, "/api/concept/nope").0, 404);
        assert_eq!(get(port, "/../Cargo.toml").0, 404);
        let (status, head, _) =
            request(port, &format!("POST /api/model HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
        assert_eq!(status, 405);
        assert!(head.contains("Allow: GET, HEAD"), "{head}");
        let (status, _, _) = request(port, "GET /api/model HTTP/1.1\r\nHost: evil.example\r\n\r\n");
        assert_eq!(status, 403);
        let (status, head, body) =
            request(port, &format!("HEAD /api/model HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
        assert_eq!(status, 200);
        assert!(head.contains("Content-Length: "), "{head}");
        assert!(body.is_empty());
    }

    #[test]
    fn follows_the_file_and_survives_a_broken_one() {
        let (port, state, dir) = start("testmodel1.archimate");
        let path = dir.path().join("m.archimate");
        let (_, _, before) = get(port, "/api/status");

        // A different model lands in the same file: the page sees it.
        std::fs::copy(corpus("modelimporter_test.archimate"), &path).unwrap();
        state.expire();
        let (_, _, after) = get(port, "/api/status");
        assert_ne!(before, after, "checksum did not change");
        let (_, _, body) = get(port, "/api/model");
        assert!(body.contains("\"name\":\"BA1\""), "the new model is served");

        // Garbage lands in the file: the last good model stays, and the page
        // is told the file is broken.
        std::fs::write(&path, b"<not a model at all").unwrap();
        state.expire();
        let (_, _, status) = get(port, "/api/status");
        assert!(status.contains("\"error\":\""), "{status}");
        let (_, _, body) = get(port, "/api/model");
        assert!(body.contains("\"name\":\"BA1\""), "the last good model is still served");
    }
}
