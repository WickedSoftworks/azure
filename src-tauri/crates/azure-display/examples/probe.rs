//! Prints what this machine looks like to the colour core.
//!
//! `cargo run -p azure-display --example probe`
//!
//! Writes nothing. Every number here is read back from the OS, and it is
//! the first thing to run when a display behaves in a way the surface did
//! not predict.

use azure_display::{environment, real_core};
#[cfg(windows)]
use azure_display::win::current_foreground;

fn main() {
    let (core, notices) = real_core();
    let displays = core.displays();
    let env = environment(&displays, true, !displays.is_empty());

    println!("displays: {}", displays.len());
    for d in &displays {
        println!(
            "  {}{}  {}\n    key {}",
            d.name,
            if d.primary { " (primary)" } else { "" },
            if d.hdr { "HDR" } else { "SDR" },
            d.key
        );
    }

    println!("\nenvironment:");
    println!("  matrix available       {}", env.matrix_available);
    println!("  lut available          {}", env.lut_available);
    println!("  hdr active             {}", env.hdr_active);
    println!("  gamma range unlocked   {}", env.gamma_range_unlocked);
    println!("  colour filters active  {}", env.color_filters_active);
    println!("  exclusive fullscreen   {} (never probed before M6)", env.exclusive_fullscreen);

    #[cfg(windows)]
    {
        println!("
foreground right now:");
        match current_foreground() {
            Some(f) => {
                println!("  exe   {}", f.exe);
                match f.path {
                    Some(p) => println!("  path  {p}"),
                    None => println!("  path  (refused: elevated, so presets match by name)"),
                }
            }
            None => println!("  (nothing resolvable)"),
        }
    }

    if notices.is_empty() {
        println!("\nboth stages initialised");
    } else {
        println!();
        for n in &notices {
            println!("notice: {n}");
        }
    }
}
