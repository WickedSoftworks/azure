//! Prints what Windows did with Azure's hotkeys, and every press it
//! delivers afterwards.
//!
//! The companion to `--example watch`. That one answered "does the
//! watcher see the foreground"; this one answers the two questions
//! registration raises: did the chords register on this machine, and does
//! a press actually arrive.
//!
//! ```text
//! cargo run -p azure-display --example keys
//! ```
//!
//! It binds hold-to-bypass to `ALT+PAUSE`, which the shipped defaults
//! leave unset, so that the press-and-release pair can be watched — that
//! is the half Windows does not deliver and Azure polls for.

use std::sync::mpsc;

use azure_display::{Hotkeys, Outcome};
use azure_settings::{BindingSet, Chord};

fn main() {
    let bindings = BindingSet {
        hold_bypass: Chord::parse("ALT+PAUSE").ok(),
        ..Default::default()
    };

    let (tx, rx) = mpsc::channel();
    let hotkeys = Hotkeys::spawn(&bindings, move |event| {
        let _ = tx.send(event);
    });

    println!("registrations:");
    for registration in hotkeys.registrations() {
        match &registration.outcome {
            Outcome::Registered => {
                println!("  {:>16}  registered", registration.chord.to_string());
            }
            Outcome::Refused { reason } => {
                println!(
                    "  {:>16}  refused — {reason}",
                    registration.chord.to_string()
                );
            }
        }
    }

    println!("\nwaiting for presses; ctrl-c to stop.");
    println!("hold ALT+PAUSE to see a press and its release.\n");

    while let Ok(event) = rx.recv() {
        println!("  {:?} {:?}", event.action, event.phase);
    }
}
