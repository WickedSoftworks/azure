//! The Windows half. Nothing outside this module reads the registry.

pub mod roots;

pub use roots::{ea_installs, gog_installs, steam_path, ubisoft_installs};
