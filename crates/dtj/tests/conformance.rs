//! DTJ v1 conformance suite (see specs/dtj-format-v1.md §10 / §11).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use dtj::{
    low_level::{
        crc32, encode_committed_chunk, ChunkHeader, CHUNK_TRAILER_SIZE, CHUNK_TYPE_DICTIONARY,
        CHUNK_TYPE_EVENT, FILE_MAGIC,
    },
    AppendEvent, DictEntry, DictKind, Dictionary, Error, EventRecord, FileHeader, SessionReader,
    SessionWriter, Severity, TypedPayload, Value, COMMITTED_MARKER, MAX_CHUNK_PAYLOAD,
};

fn session_id() -> [u8; 16] {
    *b"fixture-session\0"
}

fn header() -> FileHeader {
    FileHeader::new(session_id(), 1_722_470_400_000, 0, "dtj-ref", "0.1.0").unwrap()
}

fn write_minimal(path: &std::path::Path) {
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
}

fn append_event_chunk(path: &std::path::Path, sequence: u64, payload: &[u8]) {
    let chunk = encode_committed_chunk(CHUNK_TYPE_EVENT, sequence, payload).unwrap();
    let mut f = OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(&chunk).unwrap();
}

fn empty_event_record(domain: u32, category: u32, event: u32, severity: u8, seq: u64) -> Vec<u8> {
    let empty_typed = TypedPayload::new().encode().unwrap();
    let mut payload = Vec::new();
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(&seq.to_le_bytes());
    payload.extend_from_slice(&domain.to_le_bytes());
    payload.extend_from_slice(&category.to_le_bytes());
    payload.extend_from_slice(&event.to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.push(severity);
    payload.push(0);
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&(empty_typed.len() as u32).to_le_bytes());
    payload.extend_from_slice(&empty_typed);
    payload
}

#[test]
fn round_trip_dictionary_and_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("round.dtj");
    write_minimal(&path);

    let r = SessionReader::open(&path).unwrap();
    assert_eq!(r.header().producer_name, "dtj-ref");
    assert_eq!(r.dictionary().get_name(DictKind::Domain, 1), Some("wire"));
    assert_eq!(r.events().len(), 2);
    assert_eq!(r.iter_events().count(), 2);
    assert!(matches!(
        r.events()[0].payload.fields[0].value,
        Value::F64(v) if (v - 12.5).abs() < f64::EPSILON
    ));
}

#[test]
fn stable_sequence_ordering() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("seq.dtj");
    write_minimal(&path);
    let r = SessionReader::open(&path).unwrap();
    let seqs: Vec<u64> = r.iter_events().map(|e| e.event_sequence).collect();
    assert_eq!(seqs, vec![1, 2]);
    assert_eq!(r.chunks_committed(), 2);
}

#[test]
fn torn_trailing_chunk_keeps_prior_commits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("torn.dtj");
    write_minimal(&path);

    let payload = 0u32.to_le_bytes().to_vec();
    let mut incomplete = encode_committed_chunk(CHUNK_TYPE_EVENT, 3, &payload).unwrap();
    incomplete.truncate(incomplete.len() - 4);

    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(&incomplete).unwrap();
    drop(f);

    let r = SessionReader::open(&path).unwrap();
    assert!(r.had_torn_tail());
    assert_eq!(r.events().len(), 2);
    assert_eq!(r.chunks_committed(), 2);
}

#[test]
fn corrupted_checksum_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("badcrc.dtj");
    write_minimal(&path);

    let mut bytes = fs::read(&path).unwrap();
    let flip_at = 128 + 24 + 4;
    bytes[flip_at] ^= 0xFF;
    fs::write(&path, &bytes).unwrap();

    match SessionReader::open(&path) {
        Err(Error::ChecksumMismatch { .. }) => {}
        Err(e) => panic!("expected ChecksumMismatch, got {e}"),
        Ok(_) => panic!("expected ChecksumMismatch, got Ok"),
    }
}

#[test]
fn unknown_dictionary_id_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("baddict.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let _ = w.intern_domain("wire").unwrap();
        let category = w.intern_category("gesture").unwrap();
        let event = w.intern_event_name("X").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
        let payload = empty_event_record(99, category, event, 2, 1);
        append_event_chunk(&path, 2, &payload);
    }

    match SessionReader::open(&path) {
        Err(Error::UnknownDictionaryId { id: 99, .. }) => {}
        Err(e) => panic!("expected UnknownDictionaryId(99), got {e}"),
        Ok(_) => panic!("expected UnknownDictionaryId(99), got Ok"),
    }
}

#[test]
fn malformed_length_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("malformed.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        w.intern_domain("wire").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
    }
    let mut bytes = fs::read(&path).unwrap();
    let mut bad_payload = Vec::new();
    bad_payload.extend_from_slice(&1u32.to_le_bytes());
    bad_payload.push(1);
    bad_payload.extend_from_slice(&[0, 0, 0]);
    bad_payload.extend_from_slice(&2u32.to_le_bytes());
    bad_payload.extend_from_slice(&500u16.to_le_bytes());
    let chunk = encode_committed_chunk(dtj::CHUNK_TYPE_DICTIONARY, 2, &bad_payload).unwrap();
    bytes.extend_from_slice(&chunk);
    fs::write(&path, bytes).unwrap();

    match SessionReader::open(&path) {
        Err(Error::MalformedRecord(_)) | Err(Error::LimitExceeded(_)) => {}
        Err(e) => panic!("expected MalformedRecord/LimitExceeded, got {e}"),
        Ok(_) => panic!("expected MalformedRecord/LimitExceeded, got Ok"),
    }
}

#[test]
fn event_payload_has_no_formatted_string() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("typed.dtj");
    write_minimal(&path);
    let r = SessionReader::open(&path).unwrap();
    for ev in r.iter_events() {
        for field in &ev.payload.fields {
            match &field.value {
                Value::Bytes(_)
                | Value::InternedString(_)
                | Value::Bool(_)
                | Value::F64(_)
                | Value::Vec2F32(_) => {}
                other => panic!("unexpected value shape: {other:?}"),
            }
        }
    }
    let raw = fs::read(&path).unwrap();
    let as_utf = String::from_utf8_lossy(&raw);
    assert!(!as_utf.contains("{\"durationMs\""));
    assert!(!as_utf.contains("\"eventName\""));
}

#[test]
fn golden_fixture_reads_via_public_api() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures/minimal_session.dtj");
    assert!(
        path.is_file(),
        "missing golden fixture at {}",
        path.display()
    );
    let r = SessionReader::open(&path).unwrap();
    assert_eq!(r.header().format_version, 1);
    assert_eq!(r.header().producer_name, "dtj-ref");
    assert_eq!(r.events().len(), 2);
    assert_eq!(r.dictionary().get_name(DictKind::Domain, 1), Some("wire"));
    assert_eq!(
        r.dictionary().get_name(DictKind::EventName, 1),
        Some("KnotHit")
    );
}

#[test]
fn crc32_helper_stable() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn invalid_severity_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sev.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let d = w.intern_domain("wire").unwrap();
        let c = w.intern_category("gesture").unwrap();
        let e = w.intern_event_name("X").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
        let payload = empty_event_record(d, c, e, 9, 1);
        append_event_chunk(&path, 2, &payload);
    }
    match SessionReader::open(&path) {
        Err(Error::InvalidSeverity(9)) => {}
        Err(e) => panic!("expected InvalidSeverity(9), got {e}"),
        Ok(_) => panic!("expected InvalidSeverity, got Ok"),
    }
}

#[test]
fn unknown_type_tag_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tag.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let d = w.intern_domain("wire").unwrap();
        let c = w.intern_category("gesture").unwrap();
        let e = w.intern_event_name("X").unwrap();
        let name = w.intern_string("f").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();

        let mut typed = Vec::new();
        typed.extend_from_slice(&1u16.to_le_bytes());
        typed.extend_from_slice(&name.to_le_bytes());
        typed.push(0x7F); // unknown tag
        typed.push(0);
        typed.push(0); // one dummy byte — decoder fails on tag before consuming

        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());
        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.extend_from_slice(&d.to_le_bytes());
        payload.extend_from_slice(&c.to_le_bytes());
        payload.extend_from_slice(&e.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.push(2);
        payload.push(0);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&(typed.len() as u32).to_le_bytes());
        payload.extend_from_slice(&typed);
        append_event_chunk(&path, 2, &payload);
    }
    match SessionReader::open(&path) {
        Err(Error::UnknownTypeTag(0x7F)) => {}
        Err(e) => panic!("expected UnknownTypeTag, got {e}"),
        Ok(_) => panic!("expected UnknownTypeTag, got Ok"),
    }
}

#[test]
fn torn_oversized_declared_tail_keeps_prior_commits() {
    // Declared payload_len > MAX on a physically incomplete final DTJC frame must
    // recover prior commits (torn_tail), not fail closed as PayloadTooLarge.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("torn_oversize.dtj");
    write_minimal(&path);

    let huge = MAX_CHUNK_PAYLOAD.saturating_add(1);
    let hdr = ChunkHeader {
        chunk_type: CHUNK_TYPE_EVENT,
        sequence: 3,
        payload_len: huge,
    };
    // Enter the recovery scan (needs ≥ header+trailer bytes) without writing the
    // declared payload — only a fixed header plus a short physical stub.
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(&hdr.encode()).unwrap();
    f.write_all(&[0u8; CHUNK_TRAILER_SIZE]).unwrap();
    drop(f);

    // Structural decode alone must not apply MAX (completeness is a reader concern).
    assert!(ChunkHeader::decode(&hdr.encode()).is_ok());

    let r = SessionReader::open(&path).unwrap();
    assert!(r.had_torn_tail());
    assert_eq!(r.events().len(), 2);
    assert_eq!(r.chunks_committed(), 2);
}

#[test]
fn full_oversized_chunk_payload_fail_closed() {
    // Physically complete chunk with payload_len > MAX → PayloadTooLarge (after
    // completeness, before any reader payload allocation of that length).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full_oversize.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        w.intern_domain("wire").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
    }

    let len = MAX_CHUNK_PAYLOAD.saturating_add(1);
    let hdr = ChunkHeader {
        chunk_type: CHUNK_TYPE_EVENT,
        sequence: 2,
        payload_len: len,
    };
    // Write exact declared size with zeros; CRC/commit valid so rejection is
    // semantic MAX — not torn-tail, CRC, or commit-marker recovery.
    let payload = vec![0u8; len as usize];
    let sum = crc32(&payload);
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(&hdr.encode()).unwrap();
    f.write_all(&payload).unwrap();
    f.write_all(&sum.to_le_bytes()).unwrap();
    f.write_all(&COMMITTED_MARKER.to_le_bytes()).unwrap();
    drop(f);

    match SessionReader::open(&path) {
        Err(Error::PayloadTooLarge { len: got, max }) => {
            assert_eq!(got, len);
            assert_eq!(max, MAX_CHUNK_PAYLOAD);
        }
        Err(e) => panic!("expected PayloadTooLarge, got {e}"),
        Ok(_) => panic!("expected PayloadTooLarge, got Ok"),
    }
}

#[test]
fn chunk_sequence_gap_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chunk_gap.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let d = w.intern_domain("wire").unwrap();
        let c = w.intern_category("gesture").unwrap();
        let e = w.intern_event_name("X").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
        // Committed chunk sequence 1 then 3 (skip 2).
        let payload = empty_event_record(d, c, e, 2, 1);
        append_event_chunk(&path, 3, &payload);
    }
    match SessionReader::open(&path) {
        Err(Error::SequenceGap {
            expected: 2,
            found: 3,
        }) => {}
        Err(e) => panic!("expected SequenceGap(2,3), got {e}"),
        Ok(_) => panic!("expected SequenceGap(2,3), got Ok"),
    }
}

#[test]
fn event_sequence_gap_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("event_gap.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let d = w.intern_domain("wire").unwrap();
        let c = w.intern_category("gesture").unwrap();
        let e = w.intern_event_name("X").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
        let payload = EventRecord::encode(&[
            EventRecord {
                monotonic_ns: 1,
                event_sequence: 1,
                domain_id: d,
                category_id: c,
                event_name_id: e,
                correlation_id: 0,
                severity: Severity::Info,
                payload: TypedPayload::new(),
            },
            EventRecord {
                monotonic_ns: 2,
                event_sequence: 3,
                domain_id: d,
                category_id: c,
                event_name_id: e,
                correlation_id: 0,
                severity: Severity::Info,
                payload: TypedPayload::new(),
            },
        ])
        .unwrap();
        append_event_chunk(&path, 2, &payload);
    }
    match SessionReader::open(&path) {
        Err(Error::SequenceGap {
            expected: 2,
            found: 3,
        }) => {}
        Err(e) => panic!("expected SequenceGap(2,3), got {e}"),
        Ok(_) => panic!("expected SequenceGap(2,3), got Ok"),
    }
}

#[test]
fn duplicate_dictionary_id_different_name_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dup_dict.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        w.intern_domain("wire").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();
        let payload = Dictionary::encode_entries(&[DictEntry {
            kind: DictKind::Domain,
            id: 1,
            name: "other".into(),
        }])
        .unwrap();
        let chunk = encode_committed_chunk(CHUNK_TYPE_DICTIONARY, 2, &payload).unwrap();
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&chunk).unwrap();
    }
    match SessionReader::open(&path) {
        Err(Error::DuplicateDictionaryId { id: 1, .. }) => {}
        Err(e) => panic!("expected DuplicateDictionaryId(1), got {e}"),
        Ok(_) => panic!("expected DuplicateDictionaryId(1), got Ok"),
    }
}

#[test]
fn interned_string_unknown_dictionary_id_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad_intern.dtj");
    {
        let mut w = SessionWriter::create(&path, header()).unwrap();
        let d = w.intern_domain("wire").unwrap();
        let c = w.intern_category("gesture").unwrap();
        let e = w.intern_event_name("X").unwrap();
        let name = w.intern_string("label").unwrap();
        w.flush_chunk().unwrap();
        w.finish().unwrap();

        let mut typed = TypedPayload::new();
        typed.push(name, Value::InternedString(999));
        let payload = EventRecord::encode(&[EventRecord {
            monotonic_ns: 1,
            event_sequence: 1,
            domain_id: d,
            category_id: c,
            event_name_id: e,
            correlation_id: 0,
            severity: Severity::Info,
            payload: typed,
        }])
        .unwrap();
        append_event_chunk(&path, 2, &payload);
    }
    match SessionReader::open(&path) {
        Err(Error::UnknownDictionaryId { id: 999, .. }) => {}
        Err(e) => panic!("expected UnknownDictionaryId(999), got {e}"),
        Ok(_) => panic!("expected UnknownDictionaryId(999), got Ok"),
    }
}

#[test]
fn unknown_committed_chunk_type_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skip.dtj");
    write_minimal(&path);
    // Append unknown type 99 with valid CRC/commit; then it must not fail the file.
    let body = b"ignored-forward-compat";
    let chunk = encode_committed_chunk(99, 3, body).unwrap();
    let mut f = OpenOptions::new().append(true).open(&path).unwrap();
    f.write_all(&chunk).unwrap();
    drop(f);

    let r = SessionReader::open(&path).unwrap();
    assert_eq!(r.events().len(), 2);
    assert_eq!(r.chunks_committed(), 3);
}

#[test]
fn invalid_magic_and_truncated_header_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.dtj");
    fs::write(&bad, b"XXXX").unwrap();
    assert!(matches!(
        SessionReader::open(&bad),
        Err(Error::MalformedRecord(_)) | Err(Error::Io(_))
    ));

    let mut hdr = vec![0u8; 128];
    hdr[0..4].copy_from_slice(FILE_MAGIC);
    hdr[4..6].copy_from_slice(&1u16.to_le_bytes());
    hdr[6..8].copy_from_slice(&128u16.to_le_bytes());
    // wrong endian
    hdr[8..12].copy_from_slice(&0u32.to_le_bytes());
    let path = dir.path().join("endian.dtj");
    fs::write(&path, &hdr).unwrap();
    assert!(matches!(
        SessionReader::open(&path),
        Err(Error::InvalidEndian)
    ));
}

#[test]
fn open_new_writer_alias_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alias.dtj");
    let mut w = SessionWriter::open_new(&path, header()).unwrap();
    let d = w.intern_domain("sim").unwrap();
    let c = w.intern_category("tick").unwrap();
    let e = w.intern_event_name("Step").unwrap();
    w.append_event(AppendEvent {
        monotonic_ns: 1,
        domain_id: d,
        category_id: c,
        event_name_id: e,
        correlation_id: 0,
        severity: Severity::Trace,
        payload: TypedPayload::new(),
    })
    .unwrap();
    w.finish().unwrap();
    let r = SessionReader::open(&path).unwrap();
    assert_eq!(r.events().len(), 1);
}

/// Bounded malformed-input regression: flipping each byte of a valid journal
/// must not panic (Result only). No external fuzz framework in this repo.
#[test]
fn zero_panic_on_single_byte_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("base.dtj");
    write_minimal(&path);
    let original = fs::read(&path).unwrap();

    for i in 0..original.len() {
        let mut mutated = original.clone();
        mutated[i] ^= 0xFF;
        let corrupt = dir.path().join(format!("c{i}.dtj"));
        fs::write(&corrupt, &mutated).unwrap();
        let result = std::panic::catch_unwind(|| SessionReader::open(&corrupt));
        assert!(
            result.is_ok(),
            "SessionReader::open panicked on byte flip at {i}"
        );
    }
}
