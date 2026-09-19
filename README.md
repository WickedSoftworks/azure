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

Presets work too: a colour state per game, kept on disk, bound to an
executable you pick or capture, and applied on their own when that game
takes the foreground. Nothing is injected to do it — Windows delivers the
focus event to Azure rather than Azure reaching into the game.

Not built yet: the launcher scanners that would find your games for you
(M5), and global hotkeys and tray residency (M7). Until those land you add
games one at a time, and the watcher only runs while Azure's window is
open.

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
