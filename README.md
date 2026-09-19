# Azure

Per-game display colour control for Windows that works on **any GPU vendor** —
NVIDIA, AMD or Intel.

Vibrance, saturation, hue, brightness, contrast, gamma, temperature and tint.
Per-game presets that apply themselves when a game takes focus. A global hotkey
to toggle mid-match. No injection, no overlay, no driver.

## Why it exists

Competitive players raise digital vibrance so enemy models separate from the
environment. The established tool for that is NVIDIA-only. The vendor-neutral
alternative is built on a single compositor API that mathematically cannot do
gamma and is dropped the moment its process exits.

Azure routes across two complementary Windows APIs instead — the DWM colour
matrix for channel mixing, the GPU scanout LUT for the tone curve. Five of eight
channels therefore keep working in exclusive-fullscreen games and survive a full
exit, on every vendor.

It also tells you, per channel, exactly which stage carried your change and
whether it landed exactly, approximately, or not at all.

## Safe alongside anti-cheat

Azure never injects a DLL, never hooks in-process, never reads another process's
memory, and installs no driver. It changes display state through documented OS
APIs — the same class of call the NVIDIA Control Panel makes. See
`docs/WHY-THIS-IS-SAFE.md` (written in M9) for the complete list of Win32 calls.

## Status

Early, and specific about it. The colour core is built and the surface is
wired to it: both backends, capability routing at op-group granularity,
every ramp write verified by readback, and a field that draws what the core
reports rather than a mock-up of it.

Not built yet: preset persistence (M4), the launcher scanners (M5), the
focus watcher that switches presets for you (M6), global hotkeys and tray
residency (M7). Until those land Azure holds one state and applies it to the
desktop while its window is open — the setup half of the product without the
invisible half.

See `docs/superpowers/specs/` for the design spec and
`docs/superpowers/plans/` for the milestone plans.

## Develop

```sh
bun install
bun tauri dev
```

`bun tauri build` produces the NSIS installer.

Fonts are checked in pre-subsetted. After adding a glyph the interface draws,
regenerate them:

```sh
python scripts/subset-fonts.py
```
