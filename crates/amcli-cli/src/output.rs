//! The output contract.
//!
//! Two decisions shape everything here.
//!
//! **Text is the default, not JSON.** Text means one record per line, tab
//! separated, in a documented column order — not a pretty table. On a
//! twenty-result search that is roughly half the tokens of the equivalent JSON,
//! and an agent studying an unfamiliar model runs dozens of these. JSON is one
//! flag away for when the caller needs to read nested structure.
//!
//! **Records go to stdout; counts, truncation notices and hints go to stderr.**
//! That keeps stdout pipeable into `cut -f2` while a human still sees the
//! context. A terminal changes colour, never structure, so what an agent gets
//! through a pipe is byte-identical to what a person sees.

use std::fmt::Write as _;
use std::io::Write;

/// How a command failed, and the exit code that says so.
///
/// Distinct codes exist so an agent can branch on a number instead of parsing
/// prose. The one that matters most is the split between "nothing matched" and
/// "several things matched": they have different remedies, and reporting the
/// second as the first is what sent people looking for the wrong problem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // the full set is the contract, whether or not each is reachable today
pub enum Code {
    Ok = 0,
    Failed = 1,
    Usage = 2,
    NotFound = 3,
    Ambiguous = 4,
    Invalid = 5,
    Conflict = 6,
    Io = 7,
    Unsupported = 8,
}

pub struct CliError {
    pub code: Code,
    pub kind: &'static str,
    pub message: String,
    pub hint: Option<String>,
    /// Rows describing candidates or suggestions, printed as records.
    pub rows: Vec<Row>,
}

impl CliError {
    pub fn new(code: Code, kind: &'static str, message: impl Into<String>) -> CliError {
        CliError { code, kind, message: message.into(), hint: None, rows: Vec::new() }
    }

    pub fn hint(mut self, h: impl Into<String>) -> CliError {
        self.hint = Some(h.into());
        self
    }

    pub fn rows(mut self, rows: Vec<Row>) -> CliError {
        self.rows = rows;
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Text,
    Json,
    Jsonl,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "text" => Format::Text,
            "json" => Format::Json,
            "jsonl" => Format::Jsonl,
            _ => return None,
        })
    }
}

/// One record. Fields are ordered; the order is the documented column order.
#[derive(Clone, Debug, Default)]
pub struct Row(pub Vec<(&'static str, Value)>);

impl Row {
    pub fn new() -> Row {
        Row(Vec::new())
    }

    pub fn s(mut self, k: &'static str, v: impl Into<String>) -> Row {
        self.0.push((k, Value::Str(v.into())));
        self
    }

    pub fn n(mut self, k: &'static str, v: i64) -> Row {
        self.0.push((k, Value::Num(v)));
        self
    }

    pub fn b(mut self, k: &'static str, v: bool) -> Row {
        self.0.push((k, Value::Bool(v)));
        self
    }

    pub fn list(mut self, k: &'static str, v: Vec<Row>) -> Row {
        self.0.push((k, Value::Rows(v)));
        self
    }

    pub fn opt(self, k: &'static str, v: Option<String>) -> Row {
        match v {
            Some(v) => self.s(k, v),
            None => self,
        }
    }

    /// Keep only these fields, or drop them when the spec is subtractive.
    fn project(&mut self, fields: &[String]) {
        let subtractive = fields.iter().all(|f| f.starts_with('-'));
        if subtractive {
            self.0.retain(|(k, _)| !fields.iter().any(|f| &f[1..] == *k));
        } else {
            self.0.retain(|(k, _)| fields.iter().any(|f| f == *k));
        }
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Str(String),
    Num(i64),
    Bool(bool),
    Rows(Vec<Row>),
}

/// A successful result: records plus whatever the caller needs to know about
/// them that is not itself a record.
#[derive(Default)]
pub struct Output {
    pub rows: Vec<Row>,
    /// A non-zero code for a command that succeeded in running but whose
    /// verdict is failure. `validate` is the case: its findings are the output,
    /// and throwing them away to report an error would leave nothing to act on.
    pub exit: Option<Code>,
    /// Extra facts — counts, the model path, whether a limit was hit.
    pub meta: Vec<(&'static str, Value)>,
    /// Shown on stderr for a human; never on stdout.
    pub notes: Vec<String>,
}

impl Output {
    pub fn rows(rows: Vec<Row>) -> Output {
        Output { rows, ..Default::default() }
    }

    pub fn one(row: Row) -> Output {
        Output { rows: vec![row], ..Default::default() }
    }

    pub fn empty() -> Output {
        Output::default()
    }

    pub fn meta(mut self, k: &'static str, v: impl Into<String>) -> Output {
        self.meta.push((k, Value::Str(v.into())));
        self
    }

    pub fn meta_n(mut self, k: &'static str, v: i64) -> Output {
        self.meta.push((k, Value::Num(v)));
        self
    }

    pub fn meta_b(mut self, k: &'static str, v: bool) -> Output {
        self.meta.push((k, Value::Bool(v)));
        self
    }

    pub fn note(mut self, n: impl Into<String>) -> Output {
        self.notes.push(n.into());
        self
    }

    pub fn exit(mut self, code: Code) -> Output {
        self.exit = Some(code);
        self
    }
}

pub struct Printer {
    pub format: Format,
    pub quiet: bool,
    pub fields: Option<Vec<String>>,
    pub count_only: bool,
}

impl Printer {
    pub fn print(&self, mut out: Output, stdout: &mut impl Write, stderr: &mut impl Write) {
        if let Some(f) = &self.fields {
            for r in &mut out.rows {
                r.project(f);
            }
        }
        if self.count_only {
            let total = out
                .meta
                .iter()
                .find(|(k, _)| *k == "total")
                .and_then(|(_, v)| match v {
                    Value::Num(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(out.rows.len() as i64);
            let _ = writeln!(stdout, "{total}");
            return;
        }

        match self.format {
            Format::Text => self.print_text(&out, stdout, stderr),
            Format::Json => self.print_json(&out, stdout),
            Format::Jsonl => self.print_jsonl(&out, stdout),
        }
    }

    fn print_text(&self, out: &Output, stdout: &mut impl Write, stderr: &mut impl Write) {
        for r in &out.rows {
            let line: Vec<String> = r.0.iter().map(|(_, v)| v.to_text()).collect();
            let _ = writeln!(stdout, "{}", line.join("\t"));
        }
        if self.quiet {
            return;
        }
        for n in &out.notes {
            let _ = writeln!(stderr, "{n}");
        }
    }

    fn print_json(&self, out: &Output, stdout: &mut impl Write) {
        let data = json_rows(&out.rows);
        if self.quiet {
            let _ = writeln!(stdout, "{data}");
            return;
        }
        let mut meta = String::from("{");
        for (i, (k, v)) in out.meta.iter().enumerate() {
            if i > 0 {
                meta.push(',');
            }
            let _ = write!(meta, "{}:{}", quote(k), v.to_json());
        }
        meta.push('}');
        let _ = writeln!(stdout, r#"{{"ok":true,"data":{data},"meta":{meta}}}"#);
    }

    fn print_jsonl(&self, out: &Output, stdout: &mut impl Write) {
        for r in &out.rows {
            let _ = writeln!(stdout, "{}", r.to_json());
        }
    }

    pub fn print_error(&self, e: &CliError, stdout: &mut impl Write, stderr: &mut impl Write) {
        match self.format {
            Format::Text => {
                let _ = writeln!(stderr, "error: {}", e.message);
                // Hint before the rows: it says what the rows are for.
                if let Some(h) = &e.hint {
                    let _ = writeln!(stderr, "hint: {h}");
                }
                for r in &e.rows {
                    let line: Vec<String> = r.0.iter().map(|(_, v)| v.to_text()).collect();
                    let _ = writeln!(stderr, "  {}", line.join("\t"));
                }
            }
            Format::Json | Format::Jsonl => {
                let rows = json_rows(&e.rows);
                let hint = match &e.hint {
                    Some(h) => format!(r#","hint":{}"#, quote(h)),
                    None => String::new(),
                };
                let _ = writeln!(
                    stdout,
                    r#"{{"ok":false,"error":{{"code":{},"exit":{},"message":{}{hint},"candidates":{rows}}}}}"#,
                    quote(e.kind),
                    e.code as i32,
                    quote(&e.message)
                );
            }
        }
    }
}

fn json_rows(rows: &[Row]) -> String {
    let mut s = String::from("[");
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&r.to_json());
    }
    s.push(']');
    s
}

impl Row {
    pub fn to_json(&self) -> String {
        let mut s = String::from("{");
        for (i, (k, v)) in self.0.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, "{}:{}", quote(k), v.to_json());
        }
        s.push('}');
        s
    }
}

impl Value {
    fn to_text(&self) -> String {
        match self {
            // Tabs and newlines would break the one-record-per-line contract.
            Value::Str(s) => s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n"),
            Value::Num(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Rows(r) => r.len().to_string(),
        }
    }

    fn to_json(&self) -> String {
        match self {
            Value::Str(s) => quote(s),
            Value::Num(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Rows(r) => json_rows(r),
        }
    }
}

/// Minimal JSON string escaping. Writing this by hand rather than pulling in a
/// serialiser: the output shapes here are small, fixed, and fully under our
/// control.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
