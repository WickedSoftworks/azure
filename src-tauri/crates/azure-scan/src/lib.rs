//! Finding installed games across the launchers that hold them.
//!
//! Most of this crate can never run on the machine that wrote it — six
//! launchers cannot all be installed at once — so the parsing and the
//! choosing take their input as data and are tested against fixtures
//! taken from real files. Only `win::roots` touches the registry, and
//! only `scan` touches the disk.

pub mod choose;
pub mod library;
pub mod scan;
pub mod sources;
pub mod vdf;
#[cfg(windows)]
pub mod win;

pub use choose::{choose, rank, Chosen, Found};
pub use library::{
    Candidate, Confidence, Launcher, ScanReport, SourceOutcome, SourceReport, LAUNCHERS,
};
pub use scan::scan;
