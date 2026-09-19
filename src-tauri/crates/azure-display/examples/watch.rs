//! Prints every foreground change for a few seconds.
//!
//! `cargo run -p azure-display --example watch`
//!
//! The quickest way to see what the watcher sees: which executable took
//! focus, whether its path could be read, and therefore whether a preset
//! would match it by path or only by name.

use std::time::{Duration, Instant};

fn main() {
    #[cfg(windows)]
    {
        use azure_display::win::Watcher;

        let seconds = std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(8u64);

        println!("watching for {seconds}s — focus something\n");
        let _watcher = Watcher::spawn(|found| match &found.path {
            Some(path) => println!("  {:<28} {path}", found.exe),
            None => println!("  {:<28} (path refused: elevated, so name matching only)", found.exe),
        });

        let until = Instant::now() + Duration::from_secs(seconds);
        while Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("\ndone");
    }

    #[cfg(not(windows))]
    println!("the foreground watcher is Windows-only");
}
