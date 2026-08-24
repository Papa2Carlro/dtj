use dtj::{AppendEvent, FileHeader, SessionWriter, Severity, TypedPayload, Value};

fn session_id() -> [u8; 16] {
    *b"fixture-session\0"
}

fn header() -> FileHeader {
    FileHeader::new(session_id(), 1_722_470_400_000, 0, "dtj-ref", "0.1.0").unwrap()
}

fn main() {
    let path = std::path::Path::new("tests/fixtures/minimal_session.dtj");
    let mut w = SessionWriter::create(path, header()).unwrap();
    let domain = w.intern_domain("wire").unwrap();
    let category = w.intern_category("gesture").unwrap();
    let event = w.intern_event_name("KnotHit").unwrap();
    let corr = w.intern_string("gesture-7f3a").unwrap();
    let dur = w.intern_string("durationMs").unwrap();
    let pos = w.intern_string("pos").unwrap();

    let mut payload = TypedPayload::new();
    payload.push(dur, Value::F64(12.5));
    payload.push(pos, Value::Vec2F32([1.0, 2.5]));
    payload.push(w.intern_string("ok").unwrap(), Value::Bool(true));

    w.append_event(AppendEvent {
        monotonic_ns: 1_250_000_000,
        domain_id: domain,
        category_id: category,
        event_name_id: event,
        correlation_id: corr,
        severity: Severity::Info,
        payload,
    })
    .unwrap();

    let mut payload2 = TypedPayload::new();
    payload2.push(dur, Value::F64(3.0));
    w.append_event(AppendEvent {
        monotonic_ns: 1_260_000_000,
        domain_id: domain,
        category_id: category,
        event_name_id: event,
        correlation_id: corr,
        severity: Severity::Debug,
        payload: payload2,
    })
    .unwrap();

    w.finish().unwrap();
    println!("Fixture generated at {}", path.display());
}
