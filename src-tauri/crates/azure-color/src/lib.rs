//! Colour maths and capability routing for Azure.
//!
//! Pure: no Windows API, no Tauri, no I/O, no clock. Everything here is a
//! function of its arguments so that the parts of the product that are hard
//! to be sure about — what a backend can express, what actually landed —
//! are decided in code that a test can pin down on any machine.

pub mod channel;

pub use channel::{ChannelId, ChannelRange, ColorState, Unit};
pub mod matrix;
pub mod ops;

pub use matrix::Mat5;
pub use ops::{AffineOp, MixOp, OpGroup, PowerOp};
pub mod ramp;

pub use ramp::Ramp;
