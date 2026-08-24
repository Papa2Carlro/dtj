use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use dtj::{AppendEvent, FileHeader, SessionReader, SessionWriter, Severity, TypedPayload, Value};

const DEFAULT_EVENT_COUNT: u64 = 100_000;
const MAX_EVENT_COUNT: u64 = 1_000_000;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(output) = args.next() else {
        print_usage();
        return ExitCode::from(2);
    };

    let event_count = match args.next() {
        Some(value) => match parse_event_count(&value) {
            Ok(count) => count,
            Err(message) => {
                eprintln!("dtj-stress: {message}");
                return ExitCode::from(2);
            }
        },
        None => DEFAULT_EVENT_COUNT,
    };

    if args.next().is_some() {
        print_usage();
        return ExitCode::from(2);
    }

    match run(Path::new(&output), event_count) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("dtj-stress: {message}");
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!("usage: dtj-stress <output.dtj> [event_count]");
    eprintln!(
        "event_count defaults to {DEFAULT_EVENT_COUNT} and must be at most {MAX_EVENT_COUNT}"
    );
}

fn parse_event_count(value: &str) -> Result<u64, String> {
    let count = value
        .parse::<u64>()
        .map_err(|_| "event_count must be an unsigned integer".to_string())?;
    if count == 0 || count > MAX_EVENT_COUNT {
        return Err(format!(
            "event_count must be between 1 and {MAX_EVENT_COUNT}"
        ));
    }
    Ok(count)
}

fn run(output: &Path, event_count: u64) -> Result<(), String> {
    let started = Instant::now();
    let start_utc_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before Unix epoch: {error}"))?
        .as_millis() as i64;
    let header = FileHeader::new(
        *b"dtj-stress-run\0\0",
        start_utc_unix_ms,
        0,
        "dtj-stress",
        "0.1.0",
    )
    .map_err(|error| error.to_string())?;

    let mut writer = SessionWriter::create(output, header).map_err(|error| error.to_string())?;
    let domain = writer
        .intern_domain("stress")
        .map_err(|error| error.to_string())?;
    let category = writer
        .intern_category("throughput")
        .map_err(|error| error.to_string())?;
    let event_name = writer
        .intern_event_name("Append")
        .map_err(|error| error.to_string())?;
    let index_field = writer
        .intern_string("index")
        .map_err(|error| error.to_string())?;

    for index in 0..event_count {
        let mut payload = TypedPayload::new();
        payload.push(index_field, Value::U64(index));
        writer
            .append_event(AppendEvent {
                monotonic_ns: index.saturating_mul(1_000),
                domain_id: domain,
                category_id: category,
                event_name_id: event_name,
                correlation_id: 0,
                severity: Severity::Info,
                payload,
            })
            .map_err(|error| error.to_string())?;
    }
    writer.finish().map_err(|error| error.to_string())?;

    let reader = SessionReader::open(output).map_err(|error| error.to_string())?;
    if reader.events().len() != event_count as usize || reader.had_torn_tail() {
        return Err(format!(
            "verification failed: events={}, torn_tail={}",
            reader.events().len(),
            reader.had_torn_tail()
        ));
    }

    let bytes = std::fs::metadata(output)
        .map_err(|error| error.to_string())?
        .len();
    let elapsed = started.elapsed();
    let events_per_second = event_count as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
    println!(
        "ok events={event_count} bytes={bytes} chunks={} elapsed_ms={} events_per_second={events_per_second:.0}",
        reader.chunks_committed(),
        elapsed.as_millis()
    );
    Ok(())
}
