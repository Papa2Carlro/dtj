//! DTJ CLI — versioned local adapter boundary for Debug Trace MCP.
//!
//! Production command:
//!   `dtj read-session <path>`
//!
//! Prints one JSON object to stdout. Exit code 0 when a structured result was
//! written (`ok: true|false`). Non-zero only for CLI usage / stdout failures.
//! Does not compile Rust at runtime and is not a C ABI.

use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use dtj::{Error, SessionReader};

const ADAPTER_NAME: &str = "dtj-cli";
const ADAPTER_VERSION: u32 = 1;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_usage();
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "--version" | "-V" => {
            println!("dtj {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "read-session" => {
            let Some(path) = args.next() else {
                eprintln!("usage: dtj read-session <session_path>");
                return ExitCode::from(2);
            };
            if args.next().is_some() {
                eprintln!("usage: dtj read-session <session_path>");
                return ExitCode::from(2);
            }
            let json = read_session_json(&path);
            if let Err(err) = writeln!(io::stdout(), "{json}") {
                eprintln!("failed to write stdout: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "init" => {
            let apply = args.next().map(|s| s == "--apply").unwrap_or(false);
            if args.next().is_some() {
                eprintln!("usage: dtj init [--apply]");
                return ExitCode::from(2);
            }
            match init_command(apply) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("dtj init error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "--help" | "-h" | "help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!("dtj — DTJ v1 reference CLI (adapter v{ADAPTER_VERSION})");
    eprintln!("  dtj read-session <session_path>");
    eprintln!("  dtj init [--apply]");
}

fn read_session_json(session_path: &str) -> String {
    match SessionReader::open(Path::new(session_path)) {
        Ok(reader) => success_json(session_path, &reader),
        Err(err) => error_json(session_path, &err),
    }
}

fn success_json(session_path: &str, reader: &SessionReader) -> String {
    let header = reader.header();
    let dict = reader.dictionary();
    let mut out = String::with_capacity(1024);
    out.push_str("{\"ok\":true");
    push_adapter(&mut out);
    push_str_field(&mut out, "session_path", session_path);
    out.push_str(",\"header\":{");
    out.push_str("\"format_version\":");
    out.push_str(&header.format_version.to_string());
    out.push_str(",\"flags\":");
    out.push_str(&header.flags.to_string());
    push_str_field(&mut out, "session_id_hex", &hex_bytes(&header.session_id));
    out.push_str(",\"start_utc_unix_ms\":");
    out.push_str(&header.start_utc_unix_ms.to_string());
    out.push_str(",\"mono_origin_ns\":");
    out.push_str(&header.mono_origin_ns.to_string());
    push_str_field(&mut out, "producer_name", &header.producer_name);
    push_str_field(&mut out, "producer_version", &header.producer_version);
    out.push('}');
    out.push_str(",\"chunks_committed\":");
    out.push_str(&reader.chunks_committed().to_string());
    out.push_str(",\"torn_tail\":");
    out.push_str(if reader.had_torn_tail() {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"event_count\":");
    out.push_str(&reader.events().len().to_string());

    out.push_str(",\"dictionary\":[");
    for (i, (kind, id, name)) in dict.iter_entries().into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        push_str_field_first(&mut out, "kind", dict_kind_name(kind));
        out.push_str(",\"id\":");
        out.push_str(&id.to_string());
        push_str_field(&mut out, "name", name);
        out.push('}');
    }
    out.push(']');

    out.push_str(",\"events\":[");
    for (i, ev) in reader.events().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str("\"monotonic_ns\":");
        out.push_str(&ev.monotonic_ns.to_string());
        out.push_str(",\"event_sequence\":");
        out.push_str(&ev.event_sequence.to_string());
        out.push_str(",\"domain_id\":");
        out.push_str(&ev.domain_id.to_string());
        push_optional_name(
            &mut out,
            "domain",
            dict.get_name(dtj::DictKind::Domain, ev.domain_id),
        );
        out.push_str(",\"category_id\":");
        out.push_str(&ev.category_id.to_string());
        push_optional_name(
            &mut out,
            "category",
            dict.get_name(dtj::DictKind::Category, ev.category_id),
        );
        out.push_str(",\"event_name_id\":");
        out.push_str(&ev.event_name_id.to_string());
        push_optional_name(
            &mut out,
            "event_name",
            dict.get_name(dtj::DictKind::EventName, ev.event_name_id),
        );
        out.push_str(",\"correlation_id\":");
        out.push_str(&ev.correlation_id.to_string());
        if ev.correlation_id == 0 {
            out.push_str(",\"correlation\":null");
        } else {
            push_optional_name(
                &mut out,
                "correlation",
                dict.get_name(dtj::DictKind::String, ev.correlation_id),
            );
        }
        push_str_field(&mut out, "severity", severity_name(ev.severity));
        out.push_str(",\"payload\":[");
        for (j, field) in ev.payload.fields.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_field_json(&mut out, dict, field.name_id, &field.value);
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn error_json(session_path: &str, err: &Error) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("{\"ok\":false");
    push_adapter(&mut out);
    push_str_field(&mut out, "session_path", session_path);
    out.push_str(",\"error\":{");
    match err {
        Error::ChecksumMismatch { sequence } => {
            push_str_field_first(&mut out, "kind", "ChecksumMismatch");
            out.push_str(",\"sequence\":");
            out.push_str(&sequence.to_string());
        }
        Error::SequenceGap { expected, found } => {
            push_str_field_first(&mut out, "kind", "SequenceGap");
            out.push_str(",\"expected\":");
            out.push_str(&expected.to_string());
            out.push_str(",\"found\":");
            out.push_str(&found.to_string());
        }
        Error::PayloadTooLarge { len, max } => {
            push_str_field_first(&mut out, "kind", "PayloadTooLarge");
            out.push_str(",\"len\":");
            out.push_str(&len.to_string());
            out.push_str(",\"max\":");
            out.push_str(&max.to_string());
        }
        Error::UnknownDictionaryId { kind, id } => {
            push_str_field_first(&mut out, "kind", "UnknownDictionaryId");
            out.push_str(",\"dict_kind\":");
            out.push_str(&kind.to_string());
            out.push_str(",\"id\":");
            out.push_str(&id.to_string());
        }
        Error::DuplicateDictionaryId { kind, id } => {
            push_str_field_first(&mut out, "kind", "DuplicateDictionaryId");
            out.push_str(",\"dict_kind\":");
            out.push_str(&kind.to_string());
            out.push_str(",\"id\":");
            out.push_str(&id.to_string());
        }
        Error::UnsupportedVersion(v) => {
            push_str_field_first(&mut out, "kind", "UnsupportedVersion");
            out.push_str(",\"format_version\":");
            out.push_str(&v.to_string());
        }
        Error::InvalidHeaderSize(n) => {
            push_str_field_first(&mut out, "kind", "InvalidHeaderSize");
            out.push_str(",\"header_size\":");
            out.push_str(&n.to_string());
        }
        Error::UnknownTypeTag(t) => {
            push_str_field_first(&mut out, "kind", "UnknownTypeTag");
            out.push_str(",\"tag\":");
            out.push_str(&t.to_string());
        }
        Error::InvalidSeverity(s) => {
            push_str_field_first(&mut out, "kind", "InvalidSeverity");
            out.push_str(",\"severity\":");
            out.push_str(&s.to_string());
        }
        Error::Io(_) => push_str_field_first(&mut out, "kind", "Io"),
        Error::InvalidMagic => push_str_field_first(&mut out, "kind", "InvalidMagic"),
        Error::InvalidEndian => push_str_field_first(&mut out, "kind", "InvalidEndian"),
        Error::InvalidChunkMagic => push_str_field_first(&mut out, "kind", "InvalidChunkMagic"),
        Error::MalformedRecord(_) => push_str_field_first(&mut out, "kind", "MalformedRecord"),
        Error::LimitExceeded(_) => push_str_field_first(&mut out, "kind", "LimitExceeded"),
        Error::SessionClosed => push_str_field_first(&mut out, "kind", "SessionClosed"),
    }
    push_str_field(&mut out, "message", &err.to_string());
    out.push_str("}}");
    out
}

fn push_adapter(out: &mut String) {
    out.push_str(",\"adapter\":{");
    push_str_field_first(out, "name", ADAPTER_NAME);
    out.push_str(",\"version\":");
    out.push_str(&ADAPTER_VERSION.to_string());
    out.push('}');
}

fn push_field_json(out: &mut String, dict: &dtj::Dictionary, name_id: u32, value: &dtj::Value) {
    out.push('{');
    out.push_str("\"name_id\":");
    out.push_str(&name_id.to_string());
    push_optional_name(out, "name", dict.get_name(dtj::DictKind::String, name_id));
    match value {
        dtj::Value::Bool(v) => {
            push_str_field(out, "type", "bool");
            out.push_str(",\"value\":");
            out.push_str(if *v { "true" } else { "false" });
        }
        dtj::Value::I32(v) => {
            push_str_field(out, "type", "i32");
            out.push_str(",\"value\":");
            out.push_str(&v.to_string());
        }
        dtj::Value::I64(v) => {
            push_str_field(out, "type", "i64");
            out.push_str(",\"value\":");
            out.push_str(&v.to_string());
        }
        dtj::Value::U32(v) => {
            push_str_field(out, "type", "u32");
            out.push_str(",\"value\":");
            out.push_str(&v.to_string());
        }
        dtj::Value::U64(v) => {
            push_str_field(out, "type", "u64");
            out.push_str(",\"value\":");
            out.push_str(&v.to_string());
        }
        dtj::Value::F32(v) => {
            push_str_field(out, "type", "f32");
            out.push_str(",\"value\":");
            push_f64(out, f64::from(*v));
        }
        dtj::Value::F64(v) => {
            push_str_field(out, "type", "f64");
            out.push_str(",\"value\":");
            push_f64(out, *v);
        }
        dtj::Value::Enum(v) => {
            push_str_field(out, "type", "enum");
            out.push_str(",\"value\":");
            out.push_str(&v.to_string());
        }
        dtj::Value::Vec2F32([x, y]) => {
            push_str_field(out, "type", "vec2_f32");
            out.push_str(",\"value\":[");
            push_f64(out, f64::from(*x));
            out.push(',');
            push_f64(out, f64::from(*y));
            out.push(']');
        }
        dtj::Value::Vec3F32([x, y, z]) => {
            push_str_field(out, "type", "vec3_f32");
            out.push_str(",\"value\":[");
            push_f64(out, f64::from(*x));
            out.push(',');
            push_f64(out, f64::from(*y));
            out.push(',');
            push_f64(out, f64::from(*z));
            out.push(']');
        }
        dtj::Value::InternedString(id) => {
            push_str_field(out, "type", "interned_string");
            out.push_str(",\"id\":");
            out.push_str(&id.to_string());
            push_optional_name(out, "value", dict.get_name(dtj::DictKind::String, *id));
        }
        dtj::Value::Bytes(bytes) => {
            push_str_field(out, "type", "bytes");
            push_str_field(out, "hex", &hex_bytes(bytes));
        }
    }
    out.push('}');
}

fn push_optional_name(out: &mut String, key: &str, name: Option<&str>) {
    out.push(',');
    push_json_string(out, key);
    out.push(':');
    match name {
        Some(v) => {
            out.push('"');
            push_escaped(out, v);
            out.push('"');
        }
        None => out.push_str("null"),
    }
}

fn push_str_field(out: &mut String, key: &str, value: &str) {
    out.push(',');
    push_str_field_first(out, key, value);
}

fn push_str_field_first(out: &mut String, key: &str, value: &str) {
    push_json_string(out, key);
    out.push(':');
    out.push('"');
    push_escaped(out, value);
    out.push('"');
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    push_escaped(out, s);
    out.push('"');
}

fn push_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

fn push_f64(out: &mut String, v: f64) {
    if !v.is_finite() {
        // JSON has no NaN/±Inf; omit non-finite floats rather than inventing strings.
        out.push_str("null");
        return;
    }
    let s = format!("{v}");
    out.push_str(&s);
    if !(s.contains('.') || s.contains('e') || s.contains('E')) {
        out.push_str(".0");
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn severity_name(severity: dtj::Severity) -> &'static str {
    match severity {
        dtj::Severity::Trace => "trace",
        dtj::Severity::Debug => "debug",
        dtj::Severity::Info => "info",
        dtj::Severity::Warn => "warn",
        dtj::Severity::Error => "error",
        dtj::Severity::Fatal => "fatal",
    }
}

fn dict_kind_name(kind: dtj::DictKind) -> &'static str {
    match kind {
        dtj::DictKind::Domain => "domain",
        dtj::DictKind::Category => "category",
        dtj::DictKind::EventName => "event_name",
        dtj::DictKind::String => "string",
    }
}

fn init_command(apply: bool) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let (lang, sdks) = detect_lang_and_sdks(&cwd);
    if !apply {
        println!("dtj init");
        println!("language: {}", lang);
        println!();
        if sdks.is_empty() {
            println!("SDKs: none");
        } else {
            println!("SDKs: {}", sdks.join(", "));
        }
        println!("next: dtj init --apply");
        println!("files:");
        println!("   .dtj/config.toml (created)");
        println!("   .gitignore (line added: .dtj/)");
        println!("   DTJ_AGENT.md (created)");
        println!("   AGENTS.md (DTJ block inserted)");
        return Ok(());
    }
    let dtj_dir = cwd.join(".dtj");
    std::fs::create_dir_all(&dtj_dir).map_err(|e| e.to_string())?;
    apply_config(&dtj_dir.join("config.toml"))?;
    apply_gitignore(&cwd.join(".gitignore"))?;
    apply_dtj_agent(&cwd.join("DTJ_AGENT.md"), &lang, &sdks)?;
    apply_agents(&cwd.join("AGENTS.md"))?;
    println!("Applied DTJ bootstrap");
    Ok(())
}

fn detect_lang_and_sdks(root: &Path) -> (String, Vec<String>) {
    let mut lang = "unknown".to_string();
    if std::fs::exists(root.join("pyproject.toml")).unwrap_or(false)
        || std::fs::exists(root.join("requirements.txt")).unwrap_or(false)
    {
        lang = "Python".to_string();
    } else if std::fs::exists(root.join("package.json")).unwrap_or(false) {
        lang = "TypeScript".to_string();
    } else if std::fs::exists(root.join("go.mod")).unwrap_or(false) {
        lang = "Go".to_string();
    } else if find_files(root, " *.csproj").is_some()
        || std::fs::exists(root.join("*.csproj")).unwrap_or(false)
    {
        lang = "C#".to_string();
    } else if std::fs::exists(root.join("CMakeLists.txt")).unwrap_or(false) {
        lang = "C/C++".to_string();
    }
    let sdks = vec![];
    (lang, sdks)
}

fn find_files(_root: &Path, _pat: &str) -> Option<()> {
    None
}

fn apply_config(path: &Path) -> Result<(), String> {
    let s = r#"[storage]
data_dir = "traces"
"#;
    if !path.exists() {
        std::fs::write(path, s).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

fn apply_gitignore(path: &Path) -> Result<(), String> {
    let line = ".dtj/";
    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.contains(line) {
            return Ok(());
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(line);
        std::fs::write(path, new_content).map_err(|e| e.to_string())
    } else {
        std::fs::write(path, line).map_err(|e| e.to_string())
    }
}

fn apply_dtj_agent(path: &Path, lang: &str, sdks: &[String]) -> Result<(), String> {
    let s = format!(
        "DTJ Agent for {lang} with SDKs: {}\n",
        if sdks.is_empty() {
            "none".to_string()
        } else {
            sdks.join(", ")
        }
    );
    if !path.exists() {
        std::fs::write(path, s).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

fn apply_agents(path: &Path) -> Result<(), String> {
    let block = "### DTJ Agent\n";
    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.contains("DTJ Agent") {
            return Ok(());
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(block);
        std::fs::write(path, new_content).map_err(|e| e.to_string())
    } else {
        std::fs::write(path, block).map_err(|e| e.to_string())
    }
}
