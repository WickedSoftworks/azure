//! Prints what a scan finds on this machine, and what each launcher said.
//!
//! The third diagnostic, after `--example probe` and `--example watch`.
//! Those answered what the machine looks like and what the watcher sees;
//! this one answers the question that cannot be unit tested — whether the
//! executable Azure picked is really the one the game runs.
//!
//! ```text
//! cargo run -p azure-scan --example library
//! ```

use azure_scan::{scan, SourceOutcome};

fn main() {
    let started = std::time::Instant::now();
    let report = scan();
    let took = started.elapsed();

    println!("sources:");
    for source in &report.sources {
        let said = match &source.outcome {
            SourceOutcome::Scanned { found } => format!("{found} found"),
            SourceOutcome::NotInstalled => "not installed".to_string(),
            SourceOutcome::Unreadable { reason } => format!("unreadable — {reason}"),
        };
        println!("  {:>10?}  {said}", source.launcher);
    }

    println!("\n{} candidates in {:?}:", report.candidates.len(), took);
    for candidate in &report.candidates {
        println!(
            "  {:<44} {:?}/{:?}\n      {}",
            candidate.name, candidate.launcher, candidate.confidence, candidate.exe
        );
    }
}
