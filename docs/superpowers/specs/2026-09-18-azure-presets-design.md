# Azure presets — design spec

**Date:** 2026-09-18 · **Status:** approved · **Milestone:** M4 + the focus half of M6

## Problem

M1 gave Azure one colour state applied to the desktop. The product is
per-game colour: a state per title, applied when that title takes focus,
without the player opening a window. This spec covers presets, their
persistence, and the foreground watcher that activates them. It does not
cover library scanning (M5), hotkeys or tray residency (M7).

## Ownership

The engine thread already owns the display and is the single point where
writes serialise. It gains the preset collection and the active id rather
than a second component owning them, because any other arrangement lets a
watcher activation race a user edit for control of the same screen.

Three layers, matching M1:

| Crate | Holds | Knows about the OS |
|---|---|---|
| `azure-presets` | `Preset`, `PresetSet`, matching, the store | no |
| `azure-display` | the foreground hook, path resolution | yes |
| `src-tauri` | commands, the config directory | via Tauri |

`azure-presets` takes its directory as an argument, so its tests run
against a temporary one.

## Data

```rust
Preset { id, name, exe: Option<String>, mode: ActivationMode, state: ColorState }
PresetSet { version, next, presets, active_id }
```

`ActivationMode` is `Focused | Manual`. `Running` — a preset that applies
while its process exists whether or not it is focused — is deliberately
absent: it needs continuous process enumeration, which is the kind of
background work this product's users are right to be suspicious of. The
field exists so adding it later is not a migration.

A **desktop preset** with `exe: None` is created on first run and cannot be
deleted. It is what applies when nothing matches.

## Persistence

`presets.json` in the app config directory, written atomically: temp file,
then rename over. A file that will not parse is moved aside to
`presets.json.corrupt-<timestamp>` and reported in the event log — never
silently reset. Losing someone's tuning without telling them is the same
class of failure as a fidelity badge that lies.

**The dirty marker** closes the gap M1 left open. An empty `display-dirty`
file is created when display state first diverges from the panel's own
defaults and removed when it is restored. At startup its presence means
the previous run died without restoring, so Azure clears the ramps before
doing anything else and says so. This is the "recovery on unclean
shutdown" the product principles call non-negotiable.

## The watcher

`SetWinEventHook(EVENT_SYSTEM_FOREGROUND, …, WINEVENT_OUTOFCONTEXT |
WINEVENT_SKIPOWNPROCESS)` on a dedicated thread with a message pump.

- **`OUTOFCONTEXT`** has Windows deliver events to Azure's own process.
  Nothing of Azure's is loaded into the game. This is the whole difference
  between a watcher and the thing kernel anti-cheat exists to stop.
- **`SKIPOWNPROCESS`** means Azure taking focus is not a focus change, so
  alt-tabbing in to adjust vibrance does not pull the game's preset out
  from under the value being adjusted.

Path resolution is `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` plus
`QueryFullProcessImageNameW`. Elevated games deny that, so the fallback is
the executable name from a toolhelp snapshot, reported as
`MatchKind::ExeName` rather than dressed up as a full-path match.
`PROCESS_VM_READ` is never requested — the spec forbids it by name.

A 300 ms settle delay keeps an alt-tab storm from strobing the display.

## Activation

Full-path match wins over executable-name match; no match activates the
desktop preset. While Azure is disabled the watcher still tracks which
preset is current but nothing is written. An activation arriving while
hold-to-bypass is down is deferred until the key is released, because the
point of the bypass is a stable comparison.

## Interface

`PresetBar` becomes functional: select, add, rename, delete, bind. Adding
is a file picker or "capture the next window that takes focus". The status
strip regains the preset name, its mode, and how it matched. Moving a
channel edits the active preset and persists on a debounce.

## Known gap

Closing the window still exits Azure, so the watcher only runs while the
window is open. Tray residency is M7. Making close hide the window instead
would leave no way to bring it back, which is worse than the honest gap.
