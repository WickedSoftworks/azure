# Azure — design spec

**Date:** 2026-09-17 · **Status:** approved · **Platform:** Windows 10/11, x64

## Problem

Competitive FPS players raise digital vibrance so enemy models separate from the
environment. The established tool for that, vibranceGUI, is NVIDIA-only. The
vendor-neutral alternative, GameHaff/DigitalVibrance, is built on a single API —
`MagSetFullscreenColorEffect`, a 5×5 colour matrix applied by the desktop
compositor — and that API has two hard limits:

- **It cannot express gamma.** The matrix is affine; gamma is a power curve. No
  parameterisation produces one.
- **It cannot survive the process exiting.** Windows drops the effect
  deliberately, so a crash can never strand a display.

Nobody ships per-game colour control that is both vendor-neutral and complete.

## Mechanism

Two complementary vendor-agnostic APIs, routed by capability.

| | DWM matrix (`MagSetFullscreenColorEffect`) | Scanout LUT (`SetDeviceGammaRamp`) |
|---|---|---|
| Saturation · vibrance · hue | yes | no — 1D LUT cannot mix channels |
| Brightness · contrast · temp · tint | yes | yes — **preferred** |
| Gamma | no — affine only | yes — **only option** |
| Survives process exit | no | **yes** |
| Works in exclusive fullscreen | no — DWM bypassed | **yes** |
| Scope | whole desktop | **per monitor** |

Each covers the other's blind spot. Routing everything the LUT can carry to the
LUT means **five of eight channels keep working in exclusive fullscreen and
survive a full exit**, which neither prior tool offers.

**The device order is fixed by hardware, not chosen:** framebuffer → matrix →
LUT → panel. No design may assume gamma before saturation.

## Architecture

The colour engine lives in **Rust**, because the effect must hold while no
window exists. TypeScript/React owns the entire UI. One implementation of the
maths, surfaced through typed Tauri commands. See the approved plan for the full
module layout, backend trait, capability routing, launcher scanners, watcher and
lifecycle design.

Routing happens at **op-group** granularity — `Mix`, `Affine`, `Power` — never
per channel. Splitting the affine group across both backends would insert a
second clamp to [0,1] at the DWM boundary and silently change the result.

Capability is `Fidelity::{Exact, Approximate, Clamped, Inert, Unrealised}`, not a
boolean. Vibrance is genuinely approximate on both vendor-neutral backends: a
5×5 affine matrix cannot express saturation-dependent gain, and the only
vendor-neutral route to true nonlinear vibrance is a 3D LUT inside `dwm.exe`,
which is ruled out by the no-injection constraint.

## Constraints

- **No injection of any kind.** No DLL injection, no in-process hooks, no
  overlay, no driver, no `PROCESS_VM_READ`. Load-bearing for anti-cheat
  coexistence, and the reason true 3D-LUT vibrance is off the table.
- HDR makes LUT behaviour undefined and matrix behaviour unreliable. Detect and
  report; do not pretend.
- Global hotkeys do not fire while an elevated window has focus unless Azure is
  also elevated (Windows UIPI). Common with kernel anti-cheat.
- `SetDeviceGammaRamp` returns TRUE while silently not applying under the GDI
  range clamp. Every write is verified by readback.

## Visual direction

**Netgraph** — the surface reports your screen the way `net_graph` reports your
connection. Seed `1812d23f`, candidate 5 of 7, locked by the user against six
catalog challengers.

One type size across the whole field; rank carried by weight, case, reversal and
rule, with exactly one monumental exception — the value being dragged. Near-black
ground derived from the use scene (a player alt-tabbing out of a dark game at
11pm). Restrained palette derived from the subject: saturated chrome would bias
the colour judgement the user is making, so amber is the only accent and the
three state hues appear only on state marks. Iosevka throughout, self-hosted and
subsetted. Diagonal hatch is the literal mark of an inert or clamped channel.

**The interface cannot show a colour preview.** The compositor applies the effect
to the whole desktop including Azure's own window, so an in-app "after" would be
double-transformed and therefore a lie. The honest primitive is hold-to-bypass
against the real screen, and it is bound by default.

## Open

- Product name collides with Microsoft Azure in search. Settle before packaging.
- Whether DXGI Desktop Duplication captures the Mag effect, which decides if
  automated visual regression is possible at all. Resolve in M1.
- Whether DWM applies the matrix in sRGB-encoded or linear space. Resolve in M1;
  a four-line change either way, but only if learned early.
