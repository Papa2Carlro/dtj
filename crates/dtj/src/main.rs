// DTJ v1 CLI — minimal main.rs
// Built from: cargo build --bin dtj in crates/dtj/

use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: dtj <command> [args...]");
        eprintln!("Commands:");
        eprintln!("  read-session <file.dtj>   Read and display a DTJ session");
        eprintln!("  tail <file.dtj> [N]       Show last N events (default: 10)");
        eprintln!("  info <file.dtj>           Show session metadata");
        eprintln!("  verify <file.dtj>         Verify DTJ file integrity");
        process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "read-session" => {
            if args.len() < 3 {
                eprintln!("Usage: dtj read-session <file.dtj>");
                process::exit(1);
            }
            let path = &args[2];
            println!("DTJ read-session: reading {}", path);
            // TODO: actual read-session logic
        }
        "tail" => {
            let n = if args.len() >= 3 { args[2].parse::<usize>().unwrap_or(10) } else { 10 };
            println!("DTJ tail: showing last {} events from {}", n, &args[1]);
            // TODO: actual tail logic
        }
        "info" => {
            if args.len() < 3 {
                eprintln!("Usage: dtj info <file.dtj>");
                process::exit(1);
            }
            let path = &args[2];
            println!("DTJ info: session metadata from {}", path);
            // TODO: actual info logic
        }
        "verify" => {
            if args.len() < 3 {
                eprintln!("Usage: dtj verify <file.dtj>");
                process::exit(1);
            }
            let path = &args[2];
            println!("DTJ verify: checking integrity of {}", path);
            // TODO: actual verify logic
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Usage: dtj <command> [args...]");
            process::exit(1);
        }
    }
}
