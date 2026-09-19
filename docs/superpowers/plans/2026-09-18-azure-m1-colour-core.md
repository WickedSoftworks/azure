# Azure M1 — Colour Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the synthetic frontend data with a real Rust colour core that composes the two vendor-neutral Windows colour APIs, applies them, verifies what landed by readback, and reports per-channel stage and fidelity to the UI.

**Architecture:** Three layers. `azure-color` is pure maths with no OS dependency — channel model, 5×5 matrix, 256×3 scanout ramp, op-group decomposition, capability routing, fidelity reporting; it is where every test that matters lives. `azure-display` owns the backend traits, a mock backend, the Windows implementations (`MagSetFullscreenColorEffect`, `SetDeviceGammaRamp`), display and environment probing, and the engine thread that owns all display state. `src-tauri` is a thin command layer over the engine. The maths exists once, in Rust, because the effect must hold while no window exists.

**Tech Stack:** Rust 1.97, Tauri 2.11, `windows` 0.62 (Win32_Foundation, Win32_Graphics_Gdi, Win32_Graphics_Dwm, Win32_UI_Magnification, Win32_Devices_Display, Win32_System_Registry), `ts-rs` 12 for generated TypeScript types, `thiserror` 2. Frontend React 19 + Vite 8, package manager **bun**.

**Spec:** `docs/superpowers/specs/2026-09-17-azure-design.md` (approved). Product context: `PRODUCT.md`.

## Global Constraints

Copied from the spec; every task inherits these.

- **No injection of any kind.** No DLL injection, no in-process hooks, no overlay, no driver, no `PROCESS_VM_READ`. Load-bearing for anti-cheat coexistence.
- **Device order is fixed by hardware:** framebuffer → matrix → LUT → panel. No code may assume gamma before saturation.
- **Routing happens at op-group granularity** — `Mix`, `Affine`, `Power` — never per channel. Splitting the affine group across both backends would insert a second clamp to [0,1] at the DWM boundary and silently change the result.
- **Fidelity is `Exact | Approximate | Clamped | Inert | Unrealised`, never a boolean.** Vibrance is genuinely approximate on both vendor-neutral backends.
- **`SetDeviceGammaRamp` returns TRUE while silently not applying** under the GDI range clamp. Every write is verified by readback.
- **Never lie about what landed.** No comment, type name, log line or UI string may claim a capability the code does not have. This includes doc comments about future work.
- **Never strand the display.** Every path that writes display state has a restore path that runs on exit, on panic, and on next launch after an unclean shutdown.
- **Windows Colour Filters occupies the identical fullscreen colour-effect slot.** Detect it and yield; never silently disable an accessibility setting.
- **bun only.** `bun install`, `bun run build`. Only `bun.lock` is committed.
- Strings the user reads are English and centralised for later translation.

## Scope

M1 is the colour core and its wiring to the existing surface. Explicitly **not** in this plan, each of which needs its own: launcher scanners (M5), focus/process watcher and preset activation (M6), global hotkeys and tray residency (M7), preset persistence on disk (M4).

The existing UI already renders everything this plan produces — `src/lib/model.ts` is the contract, and Task 9 deletes the synthetic half of it.

## File Structure

```
src-tauri/Cargo.toml                    workspace root; members = ["crates/*"]
src-tauri/crates/azure-color/           PURE. no windows, no tauri, no I/O.
  src/lib.rs                            re-exports; crate-level doc
  src/channel.rs                        ChannelId, Unit, ChannelRange, ColorState
  src/matrix.rs                         Mat5, saturation, hue, affine-as-matrix
  src/ramp.rs                           Ramp, build from affine+power, deviation
  src/ops.rs                            OpGroup, MixOp, AffineOp, PowerOp
  src/route.rs                          Environment, Stage, Fidelity, plan()
src-tauri/crates/azure-display/         OS-facing. windows crate lives here only.
  src/lib.rs                            re-exports
  src/backend.rs                        MatrixBackend, RampBackend, errors, DisplayInfo
  src/mock.rs                           MockMatrix, MockRamp — clamp simulation
  src/engine.rs                         Core: apply/restore, ApplyReport
  src/thread.rs                         EngineHandle, the owning thread
  src/probe.rs                          Environment probe (cfg-split)
  src/win/mod.rs                        #[cfg(windows)] module root
  src/win/magnifier.rs                  MagSetFullscreenColorEffect
  src/win/lut.rs                        Set/GetDeviceGammaRamp + readback verify
  src/win/displays.rs                   enumeration, HDR, Colour Filters, gamma range
src-tauri/src/lib.rs                    tauri builder, engine handle, lifecycle
src-tauri/src/commands.rs               #[tauri::command] surface
src/lib/bindings/*.ts                   GENERATED by ts-rs. never hand-edited.
src/lib/ipc.ts                          typed invoke wrappers + browser fallback
src/lib/model.ts                        UI-only helpers; types re-exported from bindings
```

---

### Task 1: Workspace and channel model

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `[workspace]`)
- Create: `src-tauri/crates/azure-color/Cargo.toml`
- Create: `src-tauri/crates/azure-color/src/lib.rs`
- Create: `src-tauri/crates/azure-color/src/channel.rs`
- Test: inline `#[cfg(test)]` in `channel.rs`

**Interfaces:**
- Produces: `ChannelId` (8 unit variants, `ChannelId::ALL`, `.key()`, `.name()`, `.range()`), `Unit::{Percent,Degrees,Factor}`, `ChannelRange{min,max,neutral,step,unit}`, `ColorState` (struct of 8 `i32` fields), `ColorState::neutral()`, `.get(ChannelId)`, `.set(ChannelId, i32)` (clamping), `.is_neutral(ChannelId)`, `.touched()`.
- The eight ranges must match `src/lib/model.ts` exactly: vibrance 0..200 n100, saturation 0..200 n100, hue -180..180 n0, brightness -50..50 n0, contrast 30..200 n100, gamma 40..280 n100, temperature -100..100 n0, tint -100..100 n0.

- [x] **Step 1: Make src-tauri a workspace root**

Add to the top of `src-tauri/Cargo.toml`, above `[package]`:

```toml
[workspace]
members = ["crates/*"]
```

- [x] **Step 2: Create the pure crate manifest**

`src-tauri/crates/azure-color/Cargo.toml`:

```toml
[package]
name = "azure-color"
version = "0.1.0"
edition = "2021"
description = "Colour maths and capability routing for Azure. No OS dependency."

[dependencies]
serde = { version = "1", features = ["derive"] }
ts-rs = "12"
```

- [x] **Step 3: Write the failing test**

`src-tauri/crates/azure-color/src/channel.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_state_is_neutral_on_every_channel() {
        let s = ColorState::neutral();
        for id in ChannelId::ALL {
            assert!(s.is_neutral(id), "{id:?} not neutral in ColorState::neutral()");
            assert_eq!(s.get(id), id.range().neutral);
        }
        assert_eq!(s.touched().count(), 0);
    }

    #[test]
    fn set_clamps_to_the_channel_range() {
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 9000);
        assert_eq!(s.get(ChannelId::Gamma), 280);
        s.set(ChannelId::Gamma, -9000);
        assert_eq!(s.get(ChannelId::Gamma), 40);
        s.set(ChannelId::Hue, -181);
        assert_eq!(s.get(ChannelId::Hue), -180);
    }

    #[test]
    fn keys_are_fixed_width_because_the_field_is_a_table() {
        for id in ChannelId::ALL {
            assert_eq!(id.key().len(), 3, "{id:?} key is not 3 characters");
        }
    }

    #[test]
    fn touched_lists_only_moved_channels() {
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        let touched: Vec<_> = s.touched().collect();
        assert_eq!(touched, vec![ChannelId::Vibrance, ChannelId::Gamma]);
    }
}
```

- [x] **Step 4: Run it and watch it fail**

Run: `cargo test -p azure-color`
Expected: FAIL — `cannot find type ColorState in this scope`.

- [x] **Step 5: Implement the channel model**

`src-tauri/crates/azure-color/src/channel.rs`, above the test module:

```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The eight controls. Order is the order they are drawn in the field, which
/// is the order of the device path: mix first, then the tone curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum ChannelId {
    Vibrance, Saturation, Hue,
    Brightness, Contrast, Gamma, Temperature, Tint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum Unit { Percent, Degrees, Factor }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct ChannelRange {
    pub min: i32,
    pub max: i32,
    pub neutral: i32,
    pub step: i32,
    pub unit: Unit,
}

impl ChannelId {
    pub const ALL: [ChannelId; 8] = [
        ChannelId::Vibrance, ChannelId::Saturation, ChannelId::Hue,
        ChannelId::Brightness, ChannelId::Contrast, ChannelId::Gamma,
        ChannelId::Temperature, ChannelId::Tint,
    ];

    /// Fixed-width key. The field is a table; keys must not vary in width.
    pub fn key(self) -> &'static str {
        match self {
            ChannelId::Vibrance => "VIB",
            ChannelId::Saturation => "SAT",
            ChannelId::Hue => "HUE",
            ChannelId::Brightness => "BRT",
            ChannelId::Contrast => "CON",
            ChannelId::Gamma => "GAM",
            ChannelId::Temperature => "TMP",
            ChannelId::Tint => "TNT",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ChannelId::Vibrance => "VIBRANCE",
            ChannelId::Saturation => "SATURATION",
            ChannelId::Hue => "HUE",
            ChannelId::Brightness => "BRIGHTNESS",
            ChannelId::Contrast => "CONTRAST",
            ChannelId::Gamma => "GAMMA",
            ChannelId::Temperature => "TEMPERATURE",
            ChannelId::Tint => "TINT",
        }
    }

    pub fn range(self) -> ChannelRange {
        use ChannelId::*;
        let (min, max, neutral, unit) = match self {
            Vibrance | Saturation => (0, 200, 100, Unit::Percent),
            Hue => (-180, 180, 0, Unit::Degrees),
            Brightness => (-50, 50, 0, Unit::Percent),
            Contrast => (30, 200, 100, Unit::Factor),
            Gamma => (40, 280, 100, Unit::Factor),
            Temperature | Tint => (-100, 100, 0, Unit::Percent),
        };
        ChannelRange { min, max, neutral, step: 1, unit }
    }
}

/// Channel values in UI units: percent, degrees, or factor x100.
/// Integer on purpose — this is what the sliders emit and what the log prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct ColorState {
    pub vibrance: i32,
    pub saturation: i32,
    pub hue: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub gamma: i32,
    pub temperature: i32,
    pub tint: i32,
}

impl ColorState {
    pub fn neutral() -> Self {
        let mut s = ColorState {
            vibrance: 0, saturation: 0, hue: 0,
            brightness: 0, contrast: 0, gamma: 0, temperature: 0, tint: 0,
        };
        for id in ChannelId::ALL {
            *s.slot(id) = id.range().neutral;
        }
        s
    }

    fn slot(&mut self, id: ChannelId) -> &mut i32 {
        match id {
            ChannelId::Vibrance => &mut self.vibrance,
            ChannelId::Saturation => &mut self.saturation,
            ChannelId::Hue => &mut self.hue,
            ChannelId::Brightness => &mut self.brightness,
            ChannelId::Contrast => &mut self.contrast,
            ChannelId::Gamma => &mut self.gamma,
            ChannelId::Temperature => &mut self.temperature,
            ChannelId::Tint => &mut self.tint,
        }
    }

    pub fn get(&self, id: ChannelId) -> i32 {
        let mut copy = *self;
        *copy.slot(id)
    }

    /// Clamps. A value outside the range is a bug upstream, not a reason to
    /// write something the panel cannot show.
    pub fn set(&mut self, id: ChannelId, value: i32) {
        let r = id.range();
        *self.slot(id) = value.clamp(r.min, r.max);
    }

    pub fn is_neutral(&self, id: ChannelId) -> bool {
        self.get(id) == id.range().neutral
    }

    pub fn touched(&self) -> impl Iterator<Item = ChannelId> + '_ {
        ChannelId::ALL.into_iter().filter(move |id| !self.is_neutral(*id))
    }
}

impl Default for ColorState {
    fn default() -> Self { Self::neutral() }
}
```

`src-tauri/crates/azure-color/src/lib.rs`:

```rust
//! Colour maths and capability routing for Azure.
//!
//! Pure: no Windows API, no Tauri, no I/O, no clock. Everything here is a
//! function of its arguments so that the parts of the product that are hard
//! to be sure about — what a backend can express, what actually landed —
//! are decided in code that a test can pin down on any machine.

pub mod channel;

pub use channel::{ChannelId, ChannelRange, ColorState, Unit};
```

- [x] **Step 6: Run the tests**

Run: `cargo test -p azure-color`
Expected: PASS, 4 tests.

- [x] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/crates/azure-color
git commit -m "Add the pure colour crate and the eight-channel model"
```

---

### Task 2: The 5x5 colour matrix

**Files:**
- Create: `src-tauri/crates/azure-color/src/ops.rs`
- Create: `src-tauri/crates/azure-color/src/matrix.rs`
- Modify: `src-tauri/crates/azure-color/src/lib.rs`
- Test: inline `#[cfg(test)]` in `matrix.rs`

**Interfaces:**
- Consumes: `ChannelId`, `ColorState` from Task 1.
- Produces: `OpGroup::{Mix,Affine,Power}`, `ChannelId::group()`, `MixOp{saturation,hue_degrees}`, `AffineOp{brightness,contrast,gains:[f32;3]}`, `PowerOp{gamma}`, each with `from_state(&ColorState)` and `is_identity()`; `Mat5([[f32;5];5])` with `IDENTITY`, `mul`, `apply([f32;3])`, `saturation(f32)`, `hue(f32)`, `affine(&AffineOp)`, `from_mix(&MixOp)`, `is_identity()`, `as_flat() -> [f32;25]`.

- [x] **Step 1: Write the op-group decomposition**

`src-tauri/crates/azure-color/src/ops.rs`:

```rust
use crate::channel::{ChannelId, ColorState};

/// Routing granularity. Never route a single channel: splitting the affine
/// group across both backends inserts a second clamp to [0,1] at the DWM
/// boundary and silently changes the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpGroup {
    /// Cross-channel mixing. Matrix only — a 1D LUT cannot mix channels.
    Mix,
    /// Per-channel gain and offset. Both backends can express it; the LUT is
    /// preferred because it survives exit and exclusive fullscreen.
    Affine,
    /// A power curve. LUT only — the matrix is affine.
    Power,
}

impl ChannelId {
    pub fn group(self) -> OpGroup {
        match self {
            ChannelId::Vibrance | ChannelId::Saturation | ChannelId::Hue => OpGroup::Mix,
            ChannelId::Brightness | ChannelId::Contrast
            | ChannelId::Temperature | ChannelId::Tint => OpGroup::Affine,
            ChannelId::Gamma => OpGroup::Power,
        }
    }
}

/// Saturation gain and hue rotation, collapsed to the two things a matrix
/// can actually carry.
///
/// Vibrance folds into `saturation` as a flat multiplier. That is the whole
/// of the approximation the spec warns about: true vibrance applies more
/// gain to less-saturated pixels, which is saturation-dependent and
/// therefore outside what any affine matrix can express. The routing report
/// says `Approximate` for exactly this reason.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixOp {
    pub saturation: f32,
    pub hue_degrees: f32,
}

impl MixOp {
    pub fn from_state(s: &ColorState) -> Self {
        MixOp {
            saturation: (s.saturation as f32 / 100.0) * (s.vibrance as f32 / 100.0),
            hue_degrees: s.hue as f32,
        }
    }
    pub fn is_identity(&self) -> bool {
        (self.saturation - 1.0).abs() < 1e-6 && self.hue_degrees == 0.0
    }
}

/// Per-channel gain and offset.
///
/// `brightness` is a gain about black, `contrast` a gain about mid-grey,
/// `gains` the per-channel weighting that carries temperature and tint.
/// Composed in that order; see `Mat5::affine` and `Ramp::build`, which must
/// agree because either may carry this group.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineOp {
    pub brightness: f32,
    pub contrast: f32,
    pub gains: [f32; 3],
}

impl AffineOp {
    /// First-order channel weighting, not a Planckian white-point model.
    /// +/-20% at the ends of the slider.
    const TEMP_TINT_DEPTH: f32 = 0.20;

    pub fn from_state(s: &ColorState) -> Self {
        let t = s.temperature as f32 / 100.0;
        let n = s.tint as f32 / 100.0;
        AffineOp {
            brightness: 1.0 + s.brightness as f32 / 100.0,
            contrast: s.contrast as f32 / 100.0,
            gains: [
                1.0 + Self::TEMP_TINT_DEPTH * t,
                1.0 + Self::TEMP_TINT_DEPTH * n,
                1.0 - Self::TEMP_TINT_DEPTH * t,
            ],
        }
    }
    pub fn is_identity(&self) -> bool {
        self.brightness == 1.0 && self.contrast == 1.0 && self.gains == [1.0, 1.0, 1.0]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerOp {
    /// Display gamma. 1.0 is identity; output is input^(1/gamma).
    pub gamma: f32,
}

impl PowerOp {
    pub fn from_state(s: &ColorState) -> Self {
        PowerOp { gamma: s.gamma as f32 / 100.0 }
    }
    pub fn is_identity(&self) -> bool { self.gamma == 1.0 }
}
```

- [x] **Step 2: Write the failing matrix tests**

`src-tauri/crates/azure-color/src/matrix.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::AffineOp;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn identity_leaves_colour_alone() {
        assert!(close(Mat5::IDENTITY.apply([0.2, 0.5, 0.9]), [0.2, 0.5, 0.9]));
    }

    #[test]
    fn saturation_one_is_identity() {
        assert!(Mat5::saturation(1.0).is_identity());
    }

    #[test]
    fn saturation_zero_collapses_to_luma() {
        let out = Mat5::saturation(0.0).apply([1.0, 0.0, 0.0]);
        assert!(close(out, [0.2126, 0.2126, 0.2126]), "{out:?}");
    }

    #[test]
    fn saturation_preserves_luma_at_any_gain() {
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let src = [0.8, 0.3, 0.1];
        for gain in [0.0, 0.5, 1.0, 1.56, 2.0] {
            let out = Mat5::saturation(gain).apply(src);
            assert!((luma(out) - luma(src)).abs() < 1e-4, "gain {gain} moved luma");
        }
    }

    #[test]
    fn hue_zero_is_identity() {
        assert!(Mat5::hue(0.0).is_identity());
    }

    #[test]
    fn hue_leaves_grey_alone() {
        for deg in [0.0, 37.0, 90.0, -120.0, 180.0] {
            let out = Mat5::hue(deg).apply([0.5, 0.5, 0.5]);
            assert!(close(out, [0.5, 0.5, 0.5]), "{deg} deg moved grey to {out:?}");
        }
    }

    #[test]
    fn hue_is_periodic_over_360_degrees() {
        let src = [0.9, 0.4, 0.2];
        assert!(close(Mat5::hue(360.0).apply(src), Mat5::hue(0.0).apply(src)));
    }

    #[test]
    fn composition_applies_left_operand_first() {
        // Desaturate, then halve. Order matters: the reverse is a different
        // picture, and the device order is not ours to choose.
        let desat_then_dim = Mat5::saturation(0.0).mul(Mat5::affine(&AffineOp {
            brightness: 0.5, contrast: 1.0, gains: [1.0, 1.0, 1.0],
        }));
        let out = desat_then_dim.apply([1.0, 0.0, 0.0]);
        assert!(close(out, [0.1063, 0.1063, 0.1063]), "{out:?}");
    }

    #[test]
    fn affine_contrast_pivots_on_mid_grey() {
        let m = Mat5::affine(&AffineOp {
            brightness: 1.0, contrast: 2.0, gains: [1.0, 1.0, 1.0],
        });
        assert!(close(m.apply([0.5, 0.5, 0.5]), [0.5, 0.5, 0.5]));
        assert!(close(m.apply([0.75, 0.75, 0.75]), [1.0, 1.0, 1.0]));
    }

    #[test]
    fn flat_layout_is_row_major_with_a_pinned_last_column() {
        let f = Mat5::IDENTITY.as_flat();
        assert_eq!(f.len(), 25);
        assert_eq!(f[24], 1.0);
        assert_eq!(f[4], 0.0);
    }
}
```

- [x] **Step 3: Run them and watch them fail**

Run: `cargo test -p azure-color --lib matrix`
Expected: FAIL — `cannot find type Mat5 in this scope`.

- [x] **Step 4: Implement the matrix**

`src-tauri/crates/azure-color/src/matrix.rs`, above the tests:

```rust
use crate::ops::{AffineOp, MixOp};

/// Rec.709 luma weights. sRGB primaries, so these are the right ones.
const LR: f32 = 0.2126;
const LG: f32 = 0.7152;
const LB: f32 = 0.0722;

/// A 5x5 colour matrix in the layout `MAGCOLOREFFECT` wants: row-major,
/// row-vector convention, `out = [r g b a 1] * m`. Column 4 is pinned to
/// [0,0,0,0,1]; row 4 is the offset row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat5(pub [[f32; 5]; 5]);

impl Mat5 {
    pub const IDENTITY: Mat5 = Mat5([
        [1.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, 1.0],
    ]);

    /// `self` first, then `rhs`. Row-vector convention, so this is the plain
    /// product self*rhs and the reading order matches the device order.
    pub fn mul(self, rhs: Mat5) -> Mat5 {
        let mut out = [[0.0f32; 5]; 5];
        for i in 0..5 {
            for j in 0..5 {
                let mut acc = 0.0;
                for k in 0..5 {
                    acc += self.0[i][k] * rhs.0[k][j];
                }
                out[i][j] = acc;
            }
        }
        Mat5(out)
    }

    /// Unclamped on purpose: the caller decides where the clamp goes, and
    /// the whole point of keeping the affine group on one backend is to not
    /// clamp twice.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let v = [rgb[0], rgb[1], rgb[2], 1.0, 1.0];
        let mut out = [0.0f32; 3];
        for (j, o) in out.iter_mut().enumerate() {
            *o = (0..5).map(|i| v[i] * self.0[i][j]).sum();
        }
        out
    }

    pub fn is_identity(&self) -> bool {
        self.0.iter().zip(Mat5::IDENTITY.0.iter()).all(|(r, e)| {
            r.iter().zip(e.iter()).all(|(a, b)| (a - b).abs() < 1e-6)
        })
    }

    pub fn as_flat(&self) -> [f32; 25] {
        let mut out = [0.0f32; 25];
        for i in 0..5 {
            out[i * 5..i * 5 + 5].copy_from_slice(&self.0[i]);
        }
        out
    }

    /// Luma-preserving saturation. `s` is a gain: 0 collapses to luma, 1 is
    /// identity, above 1 pushes away from the grey axis.
    pub fn saturation(s: f32) -> Mat5 {
        let (r, g, b) = ((1.0 - s) * LR, (1.0 - s) * LG, (1.0 - s) * LB);
        Mat5([
            [r + s, r, r, 0.0, 0.0],
            [g, g + s, g, 0.0, 0.0],
            [b, b, b + s, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about the grey axis, the matrix from the SVG filter effects
    /// `feColorMatrix type="hueRotate"` definition, transposed into the
    /// row-vector layout. Row sums are exactly 1, so grey is untouched.
    pub fn hue(degrees: f32) -> Mat5 {
        let (s, c) = degrees.to_radians().sin_cos();
        // Column-vector rows from the spec, transposed on write.
        let a = [
            [0.213 + 0.787 * c - 0.213 * s, 0.715 - 0.715 * c - 0.715 * s, 0.072 - 0.072 * c + 0.928 * s],
            [0.213 - 0.213 * c + 0.143 * s, 0.715 + 0.285 * c + 0.140 * s, 0.072 - 0.072 * c - 0.283 * s],
            [0.213 - 0.213 * c - 0.787 * s, 0.715 - 0.715 * c + 0.715 * s, 0.072 + 0.928 * c + 0.072 * s],
        ];
        Mat5([
            [a[0][0], a[1][0], a[2][0], 0.0, 0.0],
            [a[0][1], a[1][1], a[2][1], 0.0, 0.0],
            [a[0][2], a[1][2], a[2][2], 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// The affine group as a matrix, for when no LUT can carry it.
    ///
    /// x1 = x * brightness; x2 = (x1 - 0.5) * contrast + 0.5; x3 = x2 * gain
    /// collapses to gain*contrast*brightness*x + gain*0.5*(1 - contrast).
    /// `Ramp::build` evaluates the same three steps in the same order.
    pub fn affine(op: &AffineOp) -> Mat5 {
        let mut m = Mat5::IDENTITY;
        for c in 0..3 {
            m.0[c][c] = op.gains[c] * op.contrast * op.brightness;
            m.0[4][c] = op.gains[c] * 0.5 * (1.0 - op.contrast);
        }
        m
    }

    /// The whole mix group: saturation (carrying vibrance) then hue.
    pub fn from_mix(op: &MixOp) -> Mat5 {
        Mat5::saturation(op.saturation).mul(Mat5::hue(op.hue_degrees))
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod matrix;
pub mod ops;

pub use matrix::Mat5;
pub use ops::{AffineOp, MixOp, OpGroup, PowerOp};
```

- [x] **Step 5: Run the tests**

Run: `cargo test -p azure-color`
Expected: PASS, 14 tests.

- [x] **Step 6: Commit**

```bash
git add src-tauri/crates/azure-color
git commit -m "Add the colour matrix and op-group decomposition"
```

---

### Task 3: The scanout ramp

**Files:**
- Create: `src-tauri/crates/azure-color/src/ramp.rs`
- Modify: `src-tauri/crates/azure-color/src/lib.rs`
- Test: inline `#[cfg(test)]` in `ramp.rs`

**Interfaces:**
- Consumes: `AffineOp`, `PowerOp` from Task 2.
- Produces: `Ramp(pub [[u16; 256]; 3])` with `identity()`, `build(&AffineOp, &PowerOp)`, `is_identity()`, `max_deviation(&Ramp) -> u16`, `as_gdi() -> [u16; 768]`.
- `max_deviation` is what Task 5 uses to decide whether a write actually landed.

- [x] **Step 1: Write the failing tests**

`src-tauri/crates/azure-color/src/ramp.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: AffineOp = AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.0, 1.0, 1.0] };
    const FLAT: PowerOp = PowerOp { gamma: 1.0 };

    #[test]
    fn neutral_input_builds_the_identity_ramp() {
        assert_eq!(Ramp::build(&UNIT, &FLAT), Ramp::identity());
        assert!(Ramp::build(&UNIT, &FLAT).is_identity());
    }

    #[test]
    fn identity_ramp_spans_the_full_16_bit_range() {
        let r = Ramp::identity();
        assert_eq!(r.0[0][0], 0);
        assert_eq!(r.0[0][255], 65535);
        assert_eq!(r.0[2][255], 65535);
    }

    #[test]
    fn every_ramp_is_monotonic_non_decreasing() {
        let cases = [
            (AffineOp { brightness: 1.3, contrast: 1.4, gains: [1.2, 1.0, 0.8] }, PowerOp { gamma: 2.8 }),
            (AffineOp { brightness: 0.5, contrast: 0.3, gains: [0.8, 1.0, 1.2] }, PowerOp { gamma: 0.4 }),
            (UNIT, PowerOp { gamma: 1.12 }),
        ];
        for (affine, power) in cases {
            let r = Ramp::build(&affine, &power);
            for c in 0..3 {
                for i in 1..256 {
                    assert!(r.0[c][i] >= r.0[c][i - 1], "channel {c} dips at {i}");
                }
            }
        }
    }

    #[test]
    fn gamma_above_one_lifts_the_midtone() {
        let lifted = Ramp::build(&UNIT, &PowerOp { gamma: 2.2 });
        assert!(lifted.0[0][128] > Ramp::identity().0[0][128]);
    }

    #[test]
    fn gamma_below_one_lowers_the_midtone() {
        let crushed = Ramp::build(&UNIT, &PowerOp { gamma: 0.5 });
        assert!(crushed.0[0][128] < Ramp::identity().0[0][128]);
    }

    #[test]
    fn contrast_pivots_on_mid_grey() {
        let r = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 1.5, gains: [1.0, 1.0, 1.0] },
            &FLAT,
        );
        let identity_mid = Ramp::identity().0[0][128];
        assert!((r.0[0][128] as i32 - identity_mid as i32).abs() < 300);
        assert!(r.0[0][200] > Ramp::identity().0[0][200]);
        assert!(r.0[0][40] < Ramp::identity().0[0][40]);
    }

    #[test]
    fn temperature_gain_separates_the_channels() {
        let warm = Ramp::build(
            &AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.2, 1.0, 0.8] },
            &FLAT,
        );
        assert!(warm.0[0][200] > warm.0[1][200]);
        assert!(warm.0[1][200] > warm.0[2][200]);
    }

    #[test]
    fn values_clamp_instead_of_wrapping() {
        let blown = Ramp::build(
            &AffineOp { brightness: 1.5, contrast: 2.0, gains: [1.2, 1.2, 1.2] },
            &FLAT,
        );
        assert_eq!(blown.0[0][255], 65535);
        let crushed = Ramp::build(
            &AffineOp { brightness: 0.1, contrast: 0.3, gains: [1.0, 1.0, 1.0] },
            &PowerOp { gamma: 0.4 },
        );
        assert_eq!(crushed.0[0][0], 0);
    }

    #[test]
    fn deviation_measures_the_worst_entry() {
        let a = Ramp::identity();
        let mut b = Ramp::identity();
        b.0[1][7] = b.0[1][7].saturating_add(900);
        assert_eq!(a.max_deviation(&b), 900);
        assert_eq!(a.max_deviation(&a), 0);
    }

    #[test]
    fn gdi_layout_is_three_contiguous_planes() {
        let r = Ramp::build(&AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.2, 1.0, 0.8] }, &FLAT);
        let flat = r.as_gdi();
        assert_eq!(flat.len(), 768);
        assert_eq!(flat[0], r.0[0][0]);
        assert_eq!(flat[256], r.0[1][0]);
        assert_eq!(flat[512], r.0[2][0]);
    }
}
```

- [x] **Step 2: Run them and watch them fail**

Run: `cargo test -p azure-color --lib ramp`
Expected: FAIL — `cannot find type Ramp in this scope`.

- [x] **Step 3: Implement the ramp**

`src-tauri/crates/azure-color/src/ramp.rs`, above the tests:

```rust
use crate::ops::{AffineOp, PowerOp};

/// The scanout lookup table exactly as `SetDeviceGammaRamp` wants it:
/// three planes of 256 sixteen-bit entries, red first.
///
/// This is the stage that survives process exit and keeps working in
/// exclusive fullscreen, so everything that can be expressed here is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ramp(pub [[u16; 256]; 3]);

impl std::fmt::Debug for Ramp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ramp(r0={} r128={} r255={}, g128={}, b128={})",
            self.0[0][0], self.0[0][128], self.0[0][255], self.0[1][128], self.0[2][128])
    }
}

impl Ramp {
    pub fn identity() -> Ramp {
        let mut out = [[0u16; 256]; 3];
        for i in 0..256 {
            let v = (i as u32 * 65535 / 255) as u16;
            out[0][i] = v;
            out[1][i] = v;
            out[2][i] = v;
        }
        Ramp(out)
    }

    /// Evaluates the affine group then the power group, in that order,
    /// because that is the order of the device path. The three affine steps
    /// are the same three `Mat5::affine` collapses, so a fallback to the
    /// matrix is the same picture minus the gamma.
    pub fn build(affine: &AffineOp, power: &PowerOp) -> Ramp {
        let exponent = 1.0 / power.gamma.max(1e-3);
        let mut out = [[0u16; 256]; 3];
        for c in 0..3 {
            for i in 0..256 {
                let x = i as f32 / 255.0;
                let x = x * affine.brightness;
                let x = (x - 0.5) * affine.contrast + 0.5;
                let x = x * affine.gains[c];
                // Clamp before the power: a negative base with a fractional
                // exponent is NaN, and NaN on a display is a black screen.
                let x = x.clamp(0.0, 1.0).powf(exponent);
                out[c][i] = (x.clamp(0.0, 1.0) * 65535.0).round() as u16;
            }
        }
        Ramp(out)
    }

    pub fn is_identity(&self) -> bool {
        *self == Ramp::identity()
    }

    /// Worst-case distance between two ramps. Used to decide whether a write
    /// landed: `SetDeviceGammaRamp` reports success while silently applying
    /// something else, so the only honest check is reading it back.
    pub fn max_deviation(&self, other: &Ramp) -> u16 {
        let mut worst = 0u16;
        for c in 0..3 {
            for i in 0..256 {
                worst = worst.max(self.0[c][i].abs_diff(other.0[c][i]));
            }
        }
        worst
    }

    pub fn as_gdi(&self) -> [u16; 768] {
        let mut flat = [0u16; 768];
        for c in 0..3 {
            flat[c * 256..c * 256 + 256].copy_from_slice(&self.0[c]);
        }
        flat
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod ramp;

pub use ramp::Ramp;
```

- [x] **Step 4: Run the tests**

Run: `cargo test -p azure-color`
Expected: PASS, 24 tests.

- [x] **Step 5: Commit**

```bash
git add src-tauri/crates/azure-color
git commit -m "Add the scanout ramp and its readback deviation measure"
```

---

### Task 4: Capability routing and the fidelity report

**Files:**
- Create: `src-tauri/crates/azure-color/src/route.rs`
- Modify: `src-tauri/crates/azure-color/src/lib.rs`
- Test: inline `#[cfg(test)]` in `route.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `Environment{matrix_available, lut_available, exclusive_fullscreen, hdr_active, gamma_range_unlocked, color_filters_active}` (+ `Environment::ideal()`), `Stage::{Matrix,Lut}`, `Fidelity::{Exact,Approximate,Clamped,Inert,Unrealised}`, `Routing{mix, affine, power}` each `Option<Stage>`, `route(&Environment) -> Routing`, `ChannelReport{id, key, name, range, stage, fidelity, reachable, note}`, `ApplyPlan{matrix: Option<Mat5>, ramp: Option<Ramp>, reports: Vec<ChannelReport>}`, `plan(&ColorState, &Environment) -> ApplyPlan`.
- `GAMMA_LOCKED_REACHABLE: [i32; 2] = [70, 140]`.

- [x] **Step 1: Write the failing routing tests**

`src-tauri/crates/azure-color/src/route.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::ChannelId::*;

    fn fidelity_of(plan: &ApplyPlan, id: crate::ChannelId) -> Fidelity {
        plan.reports.iter().find(|r| r.id == id).unwrap().fidelity
    }
    fn stage_of(plan: &ApplyPlan, id: crate::ChannelId) -> Option<Stage> {
        plan.reports.iter().find(|r| r.id == id).unwrap().stage
    }

    #[test]
    fn ideal_desktop_routes_mix_to_matrix_and_the_rest_to_the_lut() {
        let r = route(&Environment::ideal());
        assert_eq!(r.mix, Some(Stage::Matrix));
        assert_eq!(r.affine, Some(Stage::Lut));
        assert_eq!(r.power, Some(Stage::Lut));
    }

    #[test]
    fn five_of_eight_channels_sit_in_the_lut_on_an_ideal_desktop() {
        // The claim the product is built on. If this number moves, the
        // README and the surface are both wrong.
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        let in_lut = p.reports.iter().filter(|r| r.stage == Some(Stage::Lut)).count();
        assert_eq!(in_lut, 5);
    }

    #[test]
    fn the_affine_group_never_splits_across_backends() {
        let envs = [
            Environment::ideal(),
            Environment { hdr_active: true, ..Environment::ideal() },
            Environment { lut_available: false, ..Environment::ideal() },
            Environment { matrix_available: false, ..Environment::ideal() },
            Environment { exclusive_fullscreen: true, ..Environment::ideal() },
        ];
        for env in envs {
            let p = plan(&ColorState::neutral(), &env);
            let stages: std::collections::HashSet<_> = [Brightness, Contrast, Temperature, Tint]
                .iter().map(|id| stage_of(&p, *id)).collect();
            assert_eq!(stages.len(), 1, "affine group split in {env:?}");
        }
    }

    #[test]
    fn affine_falls_back_to_the_matrix_when_the_lut_cannot_carry_it() {
        let env = Environment { lut_available: false, ..Environment::ideal() };
        assert_eq!(route(&env).affine, Some(Stage::Matrix));
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(fidelity_of(&p, Brightness), Fidelity::Approximate);
    }

    #[test]
    fn gamma_has_no_path_without_the_lut_because_the_matrix_is_affine() {
        let env = Environment { lut_available: false, ..Environment::ideal() };
        assert_eq!(route(&env).power, None);
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(fidelity_of(&p, Gamma), Fidelity::Unrealised);
        assert_eq!(stage_of(&p, Gamma), None);
    }

    #[test]
    fn vibrance_is_always_approximate() {
        for env in [Environment::ideal(), Environment { hdr_active: true, ..Environment::ideal() }] {
            let p = plan(&ColorState::neutral(), &env);
            assert_eq!(fidelity_of(&p, Vibrance), Fidelity::Approximate);
            assert!(p.reports.iter().find(|r| r.id == Vibrance).unwrap().note.is_some());
        }
    }

    #[test]
    fn exclusive_fullscreen_makes_the_matrix_channels_inert() {
        let env = Environment { exclusive_fullscreen: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        for id in [Vibrance, Saturation, Hue] {
            assert_eq!(fidelity_of(&p, id), Fidelity::Inert, "{id:?}");
        }
        for id in [Brightness, Contrast, Gamma, Temperature, Tint] {
            assert_ne!(fidelity_of(&p, id), Fidelity::Inert, "{id:?} should survive");
        }
    }

    #[test]
    fn hdr_pushes_affine_to_the_matrix_and_leaves_gamma_with_no_path() {
        let env = Environment { hdr_active: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        assert_eq!(stage_of(&p, Brightness), Some(Stage::Matrix));
        assert_eq!(fidelity_of(&p, Gamma), Fidelity::Unrealised);
        assert_eq!(fidelity_of(&p, Saturation), Fidelity::Approximate);
    }

    #[test]
    fn colour_filters_take_the_matrix_slot_and_we_yield() {
        let env = Environment { color_filters_active: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        for id in [Vibrance, Saturation, Hue] {
            assert_eq!(fidelity_of(&p, id), Fidelity::Unrealised, "{id:?}");
        }
        assert!(p.matrix.is_none());
        // The LUT half is untouched by the accessibility setting.
        assert_eq!(stage_of(&p, Gamma), Some(Stage::Lut));
    }

    #[test]
    fn locked_gamma_range_reports_clamped_with_the_reachable_window() {
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        let g = p.reports.iter().find(|r| r.id == Gamma).unwrap();
        assert_eq!(g.fidelity, Fidelity::Clamped);
        assert_eq!(g.reachable, Some(GAMMA_LOCKED_REACHABLE));
    }

    #[test]
    fn unlocking_the_gamma_range_clears_the_clamp() {
        let env = Environment { gamma_range_unlocked: true, ..Environment::ideal() };
        let p = plan(&ColorState::neutral(), &env);
        let g = p.reports.iter().find(|r| r.id == Gamma).unwrap();
        assert_eq!(g.fidelity, Fidelity::Exact);
        assert_eq!(g.reachable, None);
    }

    #[test]
    fn a_neutral_state_builds_no_payload_at_all() {
        let p = plan(&ColorState::neutral(), &Environment::ideal());
        assert!(p.matrix.is_none(), "neutral must not write a matrix");
        assert!(p.ramp.is_none(), "neutral must not write a ramp");
    }

    #[test]
    fn a_moved_channel_builds_only_the_payload_that_carries_it() {
        let mut s = ColorState::neutral();
        s.set(Vibrance, 156);
        let p = plan(&s, &Environment::ideal());
        assert!(p.matrix.is_some());
        assert!(p.ramp.is_none());

        let mut s = ColorState::neutral();
        s.set(Gamma, 112);
        let p = plan(&s, &Environment::ideal());
        assert!(p.matrix.is_none());
        assert!(p.ramp.is_some());
    }

    #[test]
    fn affine_on_the_matrix_carries_the_gain_but_not_the_gamma() {
        let mut s = ColorState::neutral();
        s.set(Brightness, 20);
        s.set(Gamma, 150);
        let env = Environment { lut_available: false, ..Environment::ideal() };
        let p = plan(&s, &env);
        let m = p.matrix.expect("affine should have fallen back to the matrix");
        assert!(!m.is_identity());
        assert!(p.ramp.is_none());
    }
}
```

- [x] **Step 2: Run them and watch them fail**

Run: `cargo test -p azure-color --lib route`
Expected: FAIL — `cannot find function route in this scope`.

- [x] **Step 3: Implement routing**

`src-tauri/crates/azure-color/src/route.rs`, above the tests:

```rust
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::channel::{ChannelId, ChannelRange, ColorState};
use crate::matrix::Mat5;
use crate::ops::{AffineOp, MixOp, OpGroup, PowerOp};
use crate::ramp::Ramp;

/// Gamma factors x100 that survive the GDI range clamp when
/// `GdiIcmGammaRange` has not been set to 256. Conservative: the runtime
/// readback in `azure-display` is what decides the truth for a given write.
pub const GAMMA_LOCKED_REACHABLE: [i32; 2] = [70, 140];

/// What the machine can do right now. Every field is measured, never
/// assumed; `azure-display::probe` fills it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct Environment {
    /// The Magnification API initialised and nothing else owns the slot.
    pub matrix_available: bool,
    /// At least one display accepted a gamma ramp handle.
    pub lut_available: bool,
    /// A game is bypassing the compositor, so the matrix reaches nothing.
    pub exclusive_fullscreen: bool,
    /// Any targeted display is in HDR mode.
    pub hdr_active: bool,
    /// HKLM ICM GdiIcmGammaRange == 256.
    pub gamma_range_unlocked: bool,
    /// Windows Colour Filters owns the fullscreen colour effect. We yield.
    pub color_filters_active: bool,
}

impl Environment {
    /// An SDR desktop with both stages working and the gamma range still
    /// locked — the default Windows machine.
    pub fn ideal() -> Self {
        Environment {
            matrix_available: true,
            lut_available: true,
            exclusive_fullscreen: false,
            hdr_active: false,
            gamma_range_unlocked: false,
            color_filters_active: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum Stage { Matrix, Lut }

/// How faithfully the routed stage can express a channel. Not a boolean:
/// vibrance is genuinely approximate on both vendor-neutral backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum Fidelity { Exact, Approximate, Clamped, Inert, Unrealised }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Routing {
    pub mix: Option<Stage>,
    pub affine: Option<Stage>,
    pub power: Option<Stage>,
}

/// Routes whole op-groups, never channels.
///
/// The LUT is preferred for everything it can carry because it survives
/// process exit and keeps working in exclusive fullscreen. HDR takes the LUT
/// out entirely — Windows ignores the ramp — so the affine group moves to
/// the matrix as one piece rather than being split.
pub fn route(env: &Environment) -> Routing {
    let matrix_usable = env.matrix_available && !env.color_filters_active;
    let lut_usable = env.lut_available && !env.hdr_active;

    Routing {
        mix: matrix_usable.then_some(Stage::Matrix),
        affine: if lut_usable {
            Some(Stage::Lut)
        } else if matrix_usable {
            Some(Stage::Matrix)
        } else {
            None
        },
        power: lut_usable.then_some(Stage::Lut),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct ChannelReport {
    pub id: ChannelId,
    pub key: String,
    pub name: String,
    pub range: ChannelRange,
    pub stage: Option<Stage>,
    pub fidelity: Fidelity,
    /// The sub-range that actually reaches the panel when something outside
    /// Azure is limiting it. `None` means the whole range is reachable.
    pub reachable: Option<[i32; 2]>,
    /// Present whenever fidelity is not `Exact`. States the machine fact.
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ApplyPlan {
    pub matrix: Option<Mat5>,
    pub ramp: Option<Ramp>,
    pub reports: Vec<ChannelReport>,
}

const AFFINE_UNIT: AffineOp = AffineOp { brightness: 1.0, contrast: 1.0, gains: [1.0, 1.0, 1.0] };
const POWER_FLAT: PowerOp = PowerOp { gamma: 1.0 };

/// Builds the payloads and the per-channel truth for one state in one
/// environment. Pure — the backends do the writing, this decides what.
pub fn plan(state: &ColorState, env: &Environment) -> ApplyPlan {
    let routing = route(env);
    let mix = MixOp::from_state(state);
    let affine = AffineOp::from_state(state);
    let power = PowerOp::from_state(state);

    let mut matrix = Mat5::IDENTITY;
    let mut wrote_matrix = false;
    if routing.mix == Some(Stage::Matrix) && !mix.is_identity() {
        matrix = matrix.mul(Mat5::from_mix(&mix));
        wrote_matrix = true;
    }
    if routing.affine == Some(Stage::Matrix) && !affine.is_identity() {
        matrix = matrix.mul(Mat5::affine(&affine));
        wrote_matrix = true;
    }

    let lut_affine = routing.affine == Some(Stage::Lut) && !affine.is_identity();
    let lut_power = routing.power == Some(Stage::Lut) && !power.is_identity();
    let ramp = (lut_affine || lut_power).then(|| {
        Ramp::build(
            if lut_affine { &affine } else { &AFFINE_UNIT },
            if lut_power { &power } else { &POWER_FLAT },
        )
    });

    ApplyPlan {
        matrix: wrote_matrix.then_some(matrix),
        ramp,
        reports: ChannelId::ALL.iter().map(|id| report(*id, &routing, env)).collect(),
    }
}

fn report(id: ChannelId, routing: &Routing, env: &Environment) -> ChannelReport {
    let stage = match id.group() {
        OpGroup::Mix => routing.mix,
        OpGroup::Affine => routing.affine,
        OpGroup::Power => routing.power,
    };

    let (fidelity, reachable, note): (Fidelity, Option<[i32; 2]>, Option<&str>) = match (stage, id) {
        (None, ChannelId::Gamma) => (
            Fidelity::Unrealised, None,
            Some("no vendor-neutral path: the compositor matrix is affine and cannot express a power curve"),
        ),
        (None, _) if env.color_filters_active => (
            Fidelity::Unrealised, None,
            Some("Windows Colour Filters owns the fullscreen colour effect; Azure yields to it"),
        ),
        (None, _) => (
            Fidelity::Unrealised, None,
            Some("no backend on this machine can carry this channel"),
        ),
        (Some(Stage::Matrix), _) if env.exclusive_fullscreen => (
            Fidelity::Inert, None,
            Some("a game is bypassing the compositor; the matrix reaches nothing until it exits"),
        ),
        (Some(Stage::Matrix), ChannelId::Vibrance) => (
            Fidelity::Approximate, None,
            Some("realised as a flat saturation gain: a 5x5 affine matrix cannot express saturation-dependent gain"),
        ),
        (Some(Stage::Matrix), _) if id.group() == OpGroup::Affine => (
            Fidelity::Approximate, None,
            Some("routed to the compositor matrix because the scanout LUT cannot carry it here; lost on exit and in exclusive fullscreen"),
        ),
        (Some(Stage::Matrix), _) if env.hdr_active => (
            Fidelity::Approximate, None,
            Some("HDR: the compositor blends in scRGB, so the matrix lands differently than it does in SDR"),
        ),
        (Some(Stage::Matrix), _) => (Fidelity::Exact, None, None),
        (Some(Stage::Lut), ChannelId::Gamma) if !env.gamma_range_unlocked => (
            Fidelity::Clamped, Some(GAMMA_LOCKED_REACHABLE),
            Some("Windows is clamping this range until GdiIcmGammaRange is unlocked"),
        ),
        (Some(Stage::Lut), _) => (Fidelity::Exact, None, None),
    };

    ChannelReport {
        id,
        key: id.key().to_string(),
        name: id.name().to_string(),
        range: id.range(),
        stage,
        fidelity,
        reachable,
        note: note.map(str::to_string),
    }
}
```

Add to `src/lib.rs`:

```rust
pub mod route;

pub use route::{plan, route, ApplyPlan, ChannelReport, Environment, Fidelity, Stage};
```

- [x] **Step 4: Run the tests**

Run: `cargo test -p azure-color`
Expected: PASS, 38 tests.

- [x] **Step 5: Commit**

```bash
git add src-tauri/crates/azure-color
git commit -m "Route op-groups by capability and report per-channel fidelity"
```

---

### Task 5: Backend traits, mock backend, and the apply path

**Files:**
- Create: `src-tauri/crates/azure-display/Cargo.toml`
- Create: `src-tauri/crates/azure-display/src/lib.rs`
- Create: `src-tauri/crates/azure-display/src/backend.rs`
- Create: `src-tauri/crates/azure-display/src/mock.rs`
- Create: `src-tauri/crates/azure-display/src/engine.rs`
- Test: inline `#[cfg(test)]` in `engine.rs`

**Interfaces:**
- Consumes: `azure_color::{ApplyPlan, ChannelReport, ColorState, Environment, Fidelity, Mat5, Ramp, Stage, plan}`.
- Produces:
  - `DisplayInfo { key: String, name: String, primary: bool, hdr: bool }` (ts-rs exported)
  - `LutTarget { All, One(String) }`
  - `BackendError` (thiserror): `Unavailable(String)`, `Rejected(String)`, `NotApplied { deviation: u16 }`
  - `trait MatrixBackend: Send { fn name(&self) -> &'static str; fn apply(&mut self, m: &Mat5) -> Result<(), BackendError>; fn clear(&mut self) -> Result<(), BackendError>; }`
  - `trait RampBackend: Send { fn name(&self) -> &'static str; fn displays(&self) -> Vec<DisplayInfo>; fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError>; fn clear(&mut self) -> Result<(), BackendError>; }`
  - `RampLanding { display: String, deviation: u16, clamped: bool }`
  - `StageLanding { stage: Stage, backend: String, ok: bool, detail: Option<String> }`
  - `ApplyReport { reports: Vec<ChannelReport>, stages: Vec<StageLanding>, micros: u64, dirty: bool }`
  - `Core::new(Box<dyn MatrixBackend>, Box<dyn RampBackend>)`, `Core::mock()`, `apply(&ColorState, &Environment, &LutTarget) -> ApplyReport`, `restore() -> Result<(), BackendError>`, `is_dirty() -> bool`, `displays() -> Vec<DisplayInfo>`
  - `CLAMP_TOLERANCE: u16 = 512`
- `RampBackend::apply` is responsible for readback verification; `deviation` is `written.max_deviation(&read_back)`.

- [x] **Step 1: Create the crate**

`src-tauri/crates/azure-display/Cargo.toml`:

```toml
[package]
name = "azure-display"
version = "0.1.0"
edition = "2021"
description = "Windows colour backends and the engine that owns display state."

[dependencies]
azure-color = { path = "../azure-color" }
serde = { version = "1", features = ["derive"] }
thiserror = "2"
ts-rs = "12"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = [
  "Win32_Foundation",
  "Win32_Graphics_Gdi",
  "Win32_Graphics_Dwm",
  "Win32_UI_Magnification",
  "Win32_Devices_Display",
  "Win32_System_Registry",
] }
```

- [x] **Step 2: Write the failing engine tests**

`src-tauri/crates/azure-display/src/engine.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::{ChannelId, ColorState, Environment, Fidelity, Stage};

    #[test]
    fn a_neutral_state_writes_nothing_and_leaves_the_display_clean() {
        let mut c = Core::mock();
        let report = c.apply(&ColorState::neutral(), &Environment::ideal(), &LutTarget::All);
        assert!(report.stages.is_empty(), "neutral must not touch a backend");
        assert!(!c.is_dirty());
    }

    #[test]
    fn a_mix_change_writes_the_matrix_only() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].stage, Stage::Matrix);
        assert!(report.stages[0].ok);
        assert!(c.is_dirty());
    }

    #[test]
    fn a_gamma_change_writes_the_ramp_only() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stages[0].stage, Stage::Lut);
    }

    #[test]
    fn a_clamped_ramp_is_reported_as_clamped_not_as_success() {
        let mut c = Core::new(
            Box::new(MockMatrix::default()),
            Box::new(MockRamp::clamping(4000)),
        );
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 240);
        let env = Environment { gamma_range_unlocked: true, ..Environment::ideal() };
        let report = c.apply(&s, &env, &LutTarget::All);
        let gamma = report.reports.iter().find(|r| r.id == ChannelId::Gamma).unwrap();
        assert_eq!(gamma.fidelity, Fidelity::Clamped);
        assert!(gamma.note.as_ref().unwrap().contains("readback"));
    }

    #[test]
    fn restore_clears_both_backends_and_drops_the_dirty_flag() {
        let mut c = Core::mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert!(c.is_dirty());
        c.restore().unwrap();
        assert!(!c.is_dirty());
    }

    #[test]
    fn a_failing_backend_is_reported_and_does_not_stop_the_other_stage() {
        let mut c = Core::new(Box::new(MockMatrix::failing()), Box::new(MockRamp::default()));
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::All);
        assert_eq!(report.stages.len(), 2);
        let matrix = report.stages.iter().find(|s| s.stage == Stage::Matrix).unwrap();
        let lut = report.stages.iter().find(|s| s.stage == Stage::Lut).unwrap();
        assert!(!matrix.ok);
        assert!(lut.ok);
        let sat = report.reports.iter().find(|r| r.id == ChannelId::Saturation).unwrap();
        assert_eq!(sat.fidelity, Fidelity::Unrealised);
    }

    #[test]
    fn targeting_one_display_does_not_touch_the_others() {
        let mut ramp = MockRamp::default();
        ramp.displays = vec![
            DisplayInfo { key: "A".into(), name: "A".into(), primary: true, hdr: false },
            DisplayInfo { key: "B".into(), name: "B".into(), primary: false, hdr: false },
        ];
        let mut c = Core::new(Box::new(MockMatrix::default()), Box::new(ramp));
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        let report = c.apply(&s, &Environment::ideal(), &LutTarget::One("B".into()));
        assert_eq!(report.stages[0].detail.as_deref(), Some("1 display"));
    }
}
```

- [x] **Step 3: Run them and watch them fail**

Run: `cargo test -p azure-display`
Expected: FAIL — `cannot find type Core in this scope`.

- [x] **Step 4: Implement the backend traits**

`src-tauri/crates/azure-display/src/backend.rs`:

```rust
use azure_color::{Mat5, Ramp};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct DisplayInfo {
    /// Stable device key, used to target the LUT.
    pub key: String,
    pub name: String,
    pub primary: bool,
    pub hdr: bool,
}

/// The matrix is desktop-wide; the LUT is the only per-display stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum LutTarget {
    All,
    One(String),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Rejected(String),
    #[error("the write was accepted but readback differs by {deviation} of 65535")]
    NotApplied { deviation: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RampLanding {
    pub display: String,
    pub deviation: u16,
    pub clamped: bool,
}

pub trait MatrixBackend: Send {
    fn name(&self) -> &'static str;
    fn apply(&mut self, m: &Mat5) -> Result<(), BackendError>;
    fn clear(&mut self) -> Result<(), BackendError>;
}

pub trait RampBackend: Send {
    fn name(&self) -> &'static str;
    fn displays(&self) -> Vec<DisplayInfo>;
    /// Implementations MUST read the ramp back and report the deviation.
    /// `SetDeviceGammaRamp` returns TRUE while silently not applying.
    fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError>;
    fn clear(&mut self) -> Result<(), BackendError>;
}

/// Quantisation noise between a written and a read-back ramp is a few
/// counts; anything past this is the driver applying something else.
pub const CLAMP_TOLERANCE: u16 = 512;
```

- [x] **Step 5: Implement the mock backends**

`src-tauri/crates/azure-display/src/mock.rs`:

```rust
use azure_color::{Mat5, Ramp};

use crate::backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, RampLanding, CLAMP_TOLERANCE,
};

#[derive(Default)]
pub struct MockMatrix {
    pub last: Option<Mat5>,
    pub clears: u32,
    pub fail: bool,
}

impl MockMatrix {
    pub fn failing() -> Self {
        MockMatrix { fail: true, ..Default::default() }
    }
}

impl MatrixBackend for MockMatrix {
    fn name(&self) -> &'static str { "mock-matrix" }

    fn apply(&mut self, m: &Mat5) -> Result<(), BackendError> {
        if self.fail {
            return Err(BackendError::Unavailable("mock matrix is unavailable".into()));
        }
        self.last = Some(*m);
        Ok(())
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        self.clears += 1;
        self.last = None;
        Ok(())
    }
}

pub struct MockRamp {
    pub last: Option<Ramp>,
    pub clears: u32,
    pub displays: Vec<DisplayInfo>,
    /// Deviation the simulated readback reports, standing in for the GDI
    /// range clamp that accepts a write and applies something else.
    pub clamp_deviation: u16,
}

impl Default for MockRamp {
    fn default() -> Self {
        MockRamp {
            last: None,
            clears: 0,
            displays: vec![DisplayInfo {
                key: "MOCK0".into(),
                name: "MOCK DISPLAY".into(),
                primary: true,
                hdr: false,
            }],
            clamp_deviation: 0,
        }
    }
}

impl MockRamp {
    pub fn clamping(deviation: u16) -> Self {
        MockRamp { clamp_deviation: deviation, ..Default::default() }
    }
}

impl RampBackend for MockRamp {
    fn name(&self) -> &'static str { "mock-lut" }

    fn displays(&self) -> Vec<DisplayInfo> { self.displays.clone() }

    fn apply(&mut self, target: &LutTarget, r: &Ramp) -> Result<Vec<RampLanding>, BackendError> {
        self.last = Some(*r);
        Ok(self
            .displays
            .iter()
            .filter(|d| match target {
                LutTarget::All => true,
                LutTarget::One(key) => &d.key == key,
            })
            .map(|d| RampLanding {
                display: d.key.clone(),
                deviation: self.clamp_deviation,
                clamped: self.clamp_deviation > CLAMP_TOLERANCE,
            })
            .collect())
    }

    fn clear(&mut self) -> Result<(), BackendError> {
        self.clears += 1;
        self.last = None;
        Ok(())
    }
}
```

- [x] **Step 6: Implement the apply path**

`src-tauri/crates/azure-display/src/engine.rs`, above the tests:

```rust
use std::time::Instant;

use azure_color::{plan, ChannelReport, ColorState, Environment, Fidelity, Stage};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, CLAMP_TOLERANCE,
};
use crate::mock::{MockMatrix, MockRamp};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct StageLanding {
    pub stage: Stage,
    pub backend: String,
    pub ok: bool,
    pub detail: Option<String>,
}

/// What actually happened, which is the only thing the surface is allowed
/// to draw.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub struct ApplyReport {
    pub reports: Vec<ChannelReport>,
    pub stages: Vec<StageLanding>,
    /// Wall time of the writes, microseconds. The field prints it.
    pub micros: u64,
    /// True while display state differs from the panel's own defaults.
    pub dirty: bool,
}

pub struct Core {
    matrix: Box<dyn MatrixBackend>,
    ramp: Box<dyn RampBackend>,
    dirty: bool,
}

impl Core {
    pub fn new(matrix: Box<dyn MatrixBackend>, ramp: Box<dyn RampBackend>) -> Self {
        Core { matrix, ramp, dirty: false }
    }

    /// A mock-backed core, for tests and for any platform without the
    /// Windows backends.
    pub fn mock() -> Self {
        Core::new(Box::new(MockMatrix::default()), Box::new(MockRamp::default()))
    }

    pub fn displays(&self) -> Vec<DisplayInfo> { self.ramp.displays() }

    pub fn is_dirty(&self) -> bool { self.dirty }

    pub fn apply(
        &mut self,
        state: &ColorState,
        env: &Environment,
        target: &LutTarget,
    ) -> ApplyReport {
        let started = Instant::now();
        let p = plan(state, env);
        let mut stages = Vec::new();
        let mut reports = p.reports;

        if let Some(m) = p.matrix {
            match self.matrix.apply(&m) {
                Ok(()) => {
                    self.dirty = true;
                    stages.push(StageLanding {
                        stage: Stage::Matrix,
                        backend: self.matrix.name().to_string(),
                        ok: true,
                        detail: None,
                    });
                }
                Err(e) => {
                    let detail = e.to_string();
                    downgrade(&mut reports, Stage::Matrix, Fidelity::Unrealised, &detail);
                    stages.push(StageLanding {
                        stage: Stage::Matrix,
                        backend: self.matrix.name().to_string(),
                        ok: false,
                        detail: Some(detail),
                    });
                }
            }
        }

        if let Some(r) = p.ramp {
            match self.ramp.apply(target, &r) {
                Ok(landings) => {
                    self.dirty = true;
                    let worst = landings.iter().map(|l| l.deviation).max().unwrap_or(0);
                    if worst > CLAMP_TOLERANCE {
                        downgrade(
                            &mut reports,
                            Stage::Lut,
                            Fidelity::Clamped,
                            &format!(
                                "readback differs by {worst} of 65535: the driver applied a narrower ramp"
                            ),
                        );
                    }
                    stages.push(StageLanding {
                        stage: Stage::Lut,
                        backend: self.ramp.name().to_string(),
                        ok: true,
                        detail: Some(format!(
                            "{} display{}",
                            landings.len(),
                            if landings.len() == 1 { "" } else { "s" }
                        )),
                    });
                }
                Err(e) => {
                    let detail = e.to_string();
                    downgrade(&mut reports, Stage::Lut, Fidelity::Unrealised, &detail);
                    stages.push(StageLanding {
                        stage: Stage::Lut,
                        backend: self.ramp.name().to_string(),
                        ok: false,
                        detail: Some(detail),
                    });
                }
            }
        }

        ApplyReport {
            reports,
            stages,
            micros: started.elapsed().as_micros() as u64,
            dirty: self.dirty,
        }
    }

    /// Puts the display back. Runs on exit, on panic, and on the next launch
    /// after an unclean shutdown.
    pub fn restore(&mut self) -> Result<(), BackendError> {
        let a = self.matrix.clear();
        let b = self.ramp.clear();
        self.dirty = false;
        a.and(b)
    }
}

/// Rewrites the truth for every channel a failed or clamped stage was
/// carrying. A stage that did not land cannot leave `Exact` behind.
fn downgrade(reports: &mut [ChannelReport], stage: Stage, to: Fidelity, note: &str) {
    for r in reports.iter_mut().filter(|r| r.stage == Some(stage)) {
        r.fidelity = to;
        r.note = Some(note.to_string());
    }
}
```

`src-tauri/crates/azure-display/src/lib.rs`:

```rust
//! Windows colour backends and the engine that owns display state.
//!
//! Everything that talks to the OS lives here; everything that decides what
//! to write lives in `azure-color`.

pub mod backend;
pub mod engine;
pub mod mock;

pub use backend::{
    BackendError, DisplayInfo, LutTarget, MatrixBackend, RampBackend, RampLanding, CLAMP_TOLERANCE,
};
pub use engine::{ApplyReport, Core, StageLanding};
```

- [x] **Step 7: Run the tests**

Run: `cargo test -p azure-display`
Expected: PASS, 7 tests.

- [x] **Step 8: Commit**

```bash
git add src-tauri/crates/azure-display src-tauri/Cargo.lock
git commit -m "Add backend traits, mock backends and the verified apply path"
```

---

### Task 6: The Windows backends

**Files:**
- Create: `src-tauri/crates/azure-display/src/win/mod.rs`
- Create: `src-tauri/crates/azure-display/src/win/displays.rs`
- Create: `src-tauri/crates/azure-display/src/win/magnifier.rs`
- Create: `src-tauri/crates/azure-display/src/win/lut.rs`
- Create: `src-tauri/crates/azure-display/src/probe.rs`
- Modify: `src-tauri/crates/azure-display/src/lib.rs`

**Interfaces:**
- Produces: `win::MagnifierMatrix::new() -> Result<MagnifierMatrix, BackendError>`, `win::GdiRamp::new() -> Result<GdiRamp, BackendError>`, `win::enumerate_displays() -> Vec<DisplayInfo>`, `win::color_filters_active() -> bool`, `win::gamma_range_unlocked() -> bool`, `win::unlock_gamma_range() -> GammaRangeOutcome`, `probe::real_core() -> (Core, Vec<String>)`, `probe::environment(&[DisplayInfo], bool, bool) -> Environment`.
- Every Win32 call gets a one-line comment naming the call and why it is safe. `docs/WHY-THIS-IS-SAFE.md` is generated from those comments in M9.

**No unit test asserts a visible change** — that needs a camera. The tests here assert construction and teardown; the maths is already covered.

- [x] **Step 1: Write the smoke test**

`src-tauri/crates/azure-display/src/win/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{LutTarget, RampBackend, CLAMP_TOLERANCE};
    use azure_color::Ramp;

    #[test]
    fn enumerate_finds_at_least_the_primary_display() {
        let found = enumerate_displays();
        assert!(!found.is_empty(), "no displays enumerated");
        assert_eq!(found.iter().filter(|d| d.primary).count(), 1);
        assert!(found.iter().all(|d| !d.key.is_empty()));
    }

    #[test]
    fn a_ramp_backend_round_trips_the_identity_without_stranding_the_display() {
        let mut b = match GdiRamp::new() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping: no writable display handle here ({e})");
                return;
            }
        };
        let landings = b.apply(&LutTarget::All, &Ramp::identity()).expect("identity ramp");
        assert!(!landings.is_empty());
        assert!(landings.iter().all(|l| l.deviation < CLAMP_TOLERANCE));
        b.clear().expect("restore");
    }
}
```

- [x] **Step 2: Run it and watch it fail**

Run: `cargo test -p azure-display --lib win`
Expected: FAIL — `cannot find function enumerate_displays in this scope`.

- [x] **Step 3: Implement display enumeration and the registry probes**

`src-tauri/crates/azure-display/src/win/displays.rs`. Read-only; this module writes nothing except `unlock_gamma_range`.

```rust
/// `EnumDisplayDevicesW` twice — adapter, then its monitor — for the stable
/// device key and the friendly name. Read-only enumeration of the caller's
/// own session.
pub fn enumerate_displays() -> Vec<DisplayInfo>;

/// `QueryDisplayConfig` + `DisplayConfigGetDeviceInfo` with
/// `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO`. Read-only.
fn hdr_active_for(device_name: &str) -> bool;

/// HKCU Software\Microsoft\ColorFiltering\Active == 1. Windows Colour
/// Filters owns the same fullscreen slot; we never take it from an
/// accessibility setting.
pub fn color_filters_active() -> bool;

/// HKLM SOFTWARE\Microsoft\Windows NT\CurrentVersion\ICM\GdiIcmGammaRange
/// == 256.
pub fn gamma_range_unlocked() -> bool;

/// Writes that value. HKLM is not writable without elevation and Azure does
/// not elevate itself behind the user's back, so `NeedsElevation` is a
/// normal outcome, not an error. Windows reads the value at sign-in, so a
/// successful write is not live until the user signs out and back in.
pub fn unlock_gamma_range() -> GammaRangeOutcome;
```

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export, export_to = "../../../src/lib/bindings/")]
pub enum GammaRangeOutcome {
    Unlocked { requires_sign_out: bool },
    NeedsElevation,
    Failed { reason: String },
}
```

- [x] **Step 4: Implement the magnifier backend**

`src-tauri/crates/azure-display/src/win/magnifier.rs`:

- `MagnifierMatrix::new()` calls `MagInitialize`; a failure is `BackendError::Unavailable`.
- `apply` writes `MAGCOLOREFFECT { transform: m.as_flat() }` through `MagSetFullscreenColorEffect`.
- `clear` writes the identity effect; `Drop` calls `MagUninitialize`.
- Both `apply` and `clear` refuse when `color_filters_active()`, returning `BackendError::Unavailable("Windows Colour Filters owns the fullscreen colour effect")`.
- Comment: a documented user-mode API that asks the compositor to post-process its own output. Reads nothing, touches no other process, installs nothing.

- [x] **Step 5: Implement the GDI ramp backend**

`src-tauri/crates/azure-display/src/win/lut.rs`:

- `GdiRamp::new()` creates one `HDC` per display with `CreateDCW`; `Drop` calls `DeleteDC`.
- `apply` calls `SetDeviceGammaRamp`, then **always** `GetDeviceGammaRamp`, and reports `written.max_deviation(&read_back)` per display. `deviation > CLAMP_TOLERANCE` sets `clamped: true` — the API returning TRUE is not evidence.
- `clear` writes `Ramp::identity()` to every display.
- Comment: the same documented call every vendor control panel and calibration tool makes.

- [x] **Step 6: Implement the probe**

`src-tauri/crates/azure-display/src/probe.rs`:

```rust
/// Builds the real core on Windows, the mock core anywhere else. The second
/// return is the list of reasons a backend fell back, for the event log.
pub fn real_core() -> (Core, Vec<String>);

pub fn environment(
    displays: &[DisplayInfo],
    matrix_available: bool,
    lut_available: bool,
) -> Environment;
```

`environment` fills `hdr_active` from `displays.iter().any(|d| d.hdr)`, `color_filters_active` and `gamma_range_unlocked` from the registry reads, and leaves `exclusive_fullscreen` false — the foreground watcher that sets it is M6, and reporting a guess would be a lie. That sentence goes in the code as a comment.

- [x] **Step 7: Run the tests**

Run: `cargo test -p azure-display`
Expected: PASS. The ramp round-trip either runs or prints its skip line.

- [x] **Step 8: Commit**

```bash
git add src-tauri/crates/azure-display src-tauri/Cargo.lock
git commit -m "Add the DWM matrix and scanout LUT backends"
```

---

### Task 7: Engine thread and lifecycle

**Files:**
- Create: `src-tauri/crates/azure-display/src/thread.rs`
- Modify: `src-tauri/crates/azure-display/src/lib.rs`
- Test: inline `#[cfg(test)]` in `thread.rs`

**Interfaces:**
- Produces: `EngineHandle` (`Clone + Send + Sync`) with `spawn()`, `spawn_mock()`, `apply(ColorState, LutTarget) -> Result<ApplyReport, EngineDown>`, `set_enabled(bool) -> Result<ApplyReport, EngineDown>`, `snapshot() -> Result<Snapshot, EngineDown>`, `restore() -> Result<(), EngineDown>`, `shutdown() -> Result<(), EngineDown>`; `Snapshot { channels: Vec<ChannelReport>, displays: Vec<DisplayInfo>, environment: Environment, enabled: bool, state: ColorState, target: LutTarget, notices: Vec<String> }`.
- All display state — the Magnification session, every `HDC` — lives on the worker thread and nowhere else.

- [x] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use azure_color::ChannelId;

    #[test]
    fn the_handle_applies_and_reports_from_another_thread() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Vibrance, 156);
        let report = engine.apply(s, LutTarget::All).unwrap();
        assert_eq!(report.stages.len(), 1);
        assert_eq!(engine.snapshot().unwrap().state.vibrance, 156);
    }

    #[test]
    fn disabling_restores_the_display_but_keeps_the_state() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        let report = engine.set_enabled(false).unwrap();
        assert!(!report.dirty, "disabling must put the display back");
        let snap = engine.snapshot().unwrap();
        assert_eq!(snap.state.gamma, 112, "the preset is not lost by disabling");
        assert!(!snap.enabled);
    }

    #[test]
    fn re_enabling_puts_the_state_back_on_the_screen() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        engine.set_enabled(false).unwrap();
        let report = engine.set_enabled(true).unwrap();
        assert!(report.dirty);
        assert_eq!(report.stages.len(), 1);
    }

    #[test]
    fn shutdown_restores_the_display_and_closes_the_handle() {
        let engine = EngineHandle::spawn_mock();
        let mut s = ColorState::neutral();
        s.set(ChannelId::Gamma, 112);
        engine.apply(s, LutTarget::All).unwrap();
        engine.shutdown().unwrap();
        assert!(engine.snapshot().is_err());
    }
}
```

- [x] **Step 2: Run it and watch it fail**

Run: `cargo test -p azure-display --lib thread`
Expected: FAIL — `cannot find type EngineHandle in this scope`.

- [x] **Step 3: Implement the thread**

A `std::sync::mpsc` channel of `Msg` variants, each carrying a reply `Sender`. The worker owns `Core`, the current `ColorState`, `enabled`, and the `LutTarget`. `set_enabled(false)` calls `Core::restore` without clearing the stored state; `set_enabled(true)` re-applies it. On `Msg::Shutdown`, and on loop exit for any reason, the worker calls `Core::restore` before dropping its backends.

- [x] **Step 4: Run the tests**

Run: `cargo test -p azure-display`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add src-tauri/crates/azure-display
git commit -m "Give the colour core its own thread and a restore on every exit path"
```

---

### Task 8: Tauri commands and generated bindings

**Files:**
- Modify: `src-tauri/Cargo.toml` (add the two path dependencies)
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/bindings/index.ts` (hand-written re-exports of the generated files)

Generated bindings **are committed**, so the frontend builds without a Rust toolchain.

**Interfaces:**
- Commands: `get_snapshot() -> Snapshot`, `apply_state(state: ColorState, target: LutTarget) -> ApplyReport`, `set_enabled(enabled: bool) -> ApplyReport`, `set_lut_target(target: LutTarget) -> ApplyReport`, `restore_display() -> ()`, `unlock_gamma_range() -> GammaRangeOutcome`.
- Tauri converts snake_case parameters to camelCase on the JS side; the TS wrappers in Task 9 pass `{ state, target }` and `{ enabled }`.

- [x] **Step 1: Delete the scaffold command**

Remove `greet` from `src-tauri/src/lib.rs`. Nothing calls it, and "Hello, you've been greeted from Rust!" inside a product that promises never to lie about what landed is exactly the kind of thing that stays for two years.

- [x] **Step 2: Wire the engine into Tauri state**

```rust
pub fn run() {
    let engine = EngineHandle::spawn();
    let on_exit = engine.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::apply_state,
            commands::set_enabled,
            commands::set_lut_target,
            commands::restore_display,
            commands::unlock_gamma_range,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Azure")
        .run(move |_app, event| {
            // Closing the window is not exiting — the effect has to hold
            // with no window open. Only a real exit restores.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let _ = on_exit.shutdown();
            }
        });
}
```

- [x] **Step 3: Write the commands**

Each command is three lines: take `State<EngineHandle>`, call the handle, map the channel error to a `String`. No logic — the logic is tested in the crates below it.

- [x] **Step 4: Generate the bindings**

Run: `cargo test -p azure-color -p azure-display`
Expected: `src/lib/bindings/*.ts` written by ts-rs.
Then write `src/lib/bindings/index.ts` re-exporting every generated type, and verify with `bun run build`.

- [x] **Step 5: Commit**

```bash
git add src-tauri src/lib/bindings
git commit -m "Expose the colour core over typed Tauri commands"
```

---

### Task 9: Wire the surface to the core

**Files:**
- Create: `src/lib/ipc.ts`
- Modify: `src/lib/model.ts` (keep the UI-only helpers; delete the invented capability table)
- Modify: `src/App.tsx`, and the prop types of `ChannelRow`, `Monument`, `SignalChain`, `DisplayRows`, `StatusStrip`
- Delete: `src/lib/demo.ts`
- Modify: `README.md` (status line)

**Interfaces:**
- Consumes: the generated types and the six commands.
- Produces: `isTauri()`, `loadSnapshot()`, `applyState(state, target)`, `setEnabled(enabled)`, `setLutTarget(target)`, `unlockGammaRange()`.

- [x] **Step 1: Write the IPC layer with an honest browser fallback**

`bun run dev` in a browser has no Rust core. Rather than fake one, `isTauri()` is false there and the field renders with every channel `Unrealised` and the note "no colour core in a browser session; run `bun tauri dev`". The surface must never draw a capability it does not have, and that includes during development.

- [x] **Step 2: Replace the module-level channel table**

`CHANNELS` and `channelsFor()` in `src/lib/model.ts` are the invented half. Delete both. `App` holds `channels: ChannelReport[]` from the snapshot and passes them down. `formatValue`, `FIDELITY_LABEL` and `isInert` stay — they are presentation, not capability. Adjust each component's import to the generated types; change the imports, not the markup.

- [x] **Step 3: Replace the log with real events**

Every `applyState` returns an `ApplyReport`. The log entry is built from it — `micros` for `TOOK`, the changed channel keys for `SUBJECT`, `stages[].backend` plus the channel's fidelity for `DETAIL`, `kind: "warn"` when any report came back `Clamped`, `Inert` or `Unrealised`. Delete the `Math.random()` latency.

- [x] **Step 4: Make the gamma warning row tell the truth**

`UNLOCK FULL RANGE` calls `unlockGammaRange()`. On `NeedsElevation` the row reads "Unlocking the gamma range writes an HKLM registry value and needs an elevated Azure. Restart as administrator to change it." On `Unlocked { requiresSignOut: true }` it reads "Written. Windows reads this at sign-in, so the full range is available after you sign out and back in." Neither pretends the slider changed.

- [x] **Step 5: Verify against the real app**

Run: `bun tauri dev`
Check, on the real desktop: moving `SAT` changes the screen; moving `GAM` changes the screen; `TOOK` shows a measured microsecond figure; closing the app restores the display; the event log names the backend that carried each change.

- [x] **Step 6: Update the README status line**

Replace "the Rust colour core is in progress" with what is true after this task.

- [x] **Step 7: Commit**

```bash
git add src src-tauri README.md
git commit -m "Draw what the core reports instead of synthetic data"
```

---

## Self-Review

**Spec coverage.** Mechanism table → Tasks 2, 3, 4, 6. Op-group routing → Task 4, with the never-split property as its own test. Five-way `Fidelity` → Task 4. Device order → encoded in `Mat5::mul` semantics and `Ramp::build`'s step order, tested in Task 2. Readback verification → Tasks 5 and 6. No injection → Task 6 uses only documented user-mode calls, one comment each. HDR → Task 4 routing, Task 6 detection. Colour Filters → Tasks 4 and 6. Never strand the display → Task 7. "The interface cannot show a colour preview" → unchanged; the hold-to-bypass primitive already in `App.tsx` is untouched by this plan.

**Out of scope, by design.** UIPI and elevated-window hotkeys, the `exclusive_fullscreen` probe, launcher scanning, preset persistence. `Environment.exclusive_fullscreen` exists and routes correctly but is only ever set by M6; Task 6 Step 6 says so in a comment rather than guessing.

**Known risk, resolved before writing this plan.** `tauri-specta` v2 does not exist on crates.io — only the Tauri-v1-era 1.0.2 — so the promise in `src/lib/model.ts` that bindings will be "generated by tauri-specta" cannot be kept. This plan uses `ts-rs` 12 instead, which generates types but not command wrappers; Task 9 hand-writes six thin wrappers. The stale comment dies in Task 9 Step 2.

---

## What actually happened

Executed 2026-09-18 on branch `m1-colour-core`. All nine tasks landed; 74
tests pass (`cargo test --workspace`), `bun run build` is clean, and
`bun tauri dev` runs. Where the work diverged from the plan:

- **`Ramp::from_gdi`** was added to Task 3. The plan gave the LUT backend
  `as_gdi` for writing but nothing to turn a readback into a `Ramp` that
  `max_deviation` could measure.
- **`backend::Unavailable`** was added in Task 6. The plan said a stage
  that will not initialise falls back to a mock; a mock reports success,
  which is the one thing a missing stage must never do. `Unavailable`
  carries the reason and returns it from every call.
- **ts-rs resolves `export_to` from the workspace root and strips leading
  `..`**, so the export directory is set once in `src-tauri/.cargo/config.toml`
  via `TS_RS_EXPORT_DIR` instead of per-type paths.
- **`ApplyReport.micros` is `u32`, not `u64`.** ts-rs maps `u64` to
  `bigint`, but serde sends it as a JSON number, so the generated type was
  wrong at runtime.
- **`EngineHandle` holds its sender behind a `Mutex`.** `mpsc::Sender` is
  `Send` but not `Sync`, and Tauri's managed state needs both.
- **The release profile no longer sets `panic = "abort"`.** Azure holds
  display state that only its own `Drop` impls put back, so a panic has to
  unwind through them.
- **The browser fallback is a generated fixture, not a hand-written
  table.** Task 9 Step 1 planned a TypeScript stand-in with every channel
  `Unrealised`. Generating `src/lib/preview-snapshot.json` from the real
  router in a Rust test keeps one implementation of the routing rules and
  leaves the field representative for design review; the banner does the
  honesty work instead.
- **Two claims in the existing surface were removed** as part of Step 2,
  beyond what the plan listed: the footer advertised three global hotkeys
  that do not exist until M7, and the preset bar offered a SCAN LIBRARIES
  button with no scanner behind it.
- **A real end-to-end test was added** (`the_real_engine_applies_and_restores_on_this_machine`).
  It drives the actual backends, writes a ramp one percent off neutral,
  and restores — the only test that touches the display on purpose.

Carried forward, and written into the code as comments rather than left
implied: recovery after a hard kill needs a dirty flag on disk and belongs
with preset persistence in M4, and `Environment.exclusive_fullscreen` is
routed correctly but never set until the M6 watcher can tell.
