# Azure M7 — Residency: tray, hotkeys, and recovery

Azure's success condition, stated in `PRODUCT.md`, is that it is invisible
and instant. Today it is neither: the watcher only runs while the window is
open, so closing the window ends the automatic preset switching that the
presets milestone just built. M7 is the milestone where Azure stops being an
application you use and becomes one that is simply running.

## Problem

Three gaps, all of them lifecycle:

- **Closing the window exits.** The presets spec named this honestly as a
  known gap. Making close hide the window was worse while there was no tray
  to bring it back; now there will be one.
- **There are no global hotkeys.** The surface advertised three
  (`ALT+SHIFT+V`, `ALT+SHIFT+←/→`, `CTRL+ALT+SHIFT+R`) and Task 9 of M1
  removed the footer rather than keep claiming them. The player's primary
  interface is a key pressed mid-match with the game in front.
- **A run that dies strands the display.** The marker on disk exists and is
  kept in step, but nothing reads it as evidence, so nothing recovers.

Two items carried forward from M1 land here because both only start
mattering once Azure runs with no window: `Environment.exclusive_fullscreen`
is routed correctly and never set, and the dirty marker has no recovery path.

## Ownership

`azure-settings` is new and pure, in the manner of `azure-color` and
`azure-presets`: chords, actions, bindings and their conflict rules, plus
the settings store. No Windows API, no Tauri, no knowledge of what directory
it writes to until one is handed to it. Bindings do not belong in
`azure-presets` — that crate is the preset store, and settings in it would
blur a boundary that is currently clean.

`azure-display::win::hotkey` registers them, on its own message-loop thread,
modelled on `win::watch`. Registration is where the OS gets a say, so it is
the only place that can refuse.

`src-tauri` gains the tray and the window lifecycle. That is Tauri's
territory and it stays there.

## Settings

```
Settings {
  version: u32,
  bindings: BindingSet,
  autostart: bool,
  told_about_tray: bool,
}
```

Stored in `settings.json` beside `presets.json`, with the same discipline:
write to a temporary file and rename over the target, keep an unreadable
file rather than overwrite it, and never fail a load — report instead.

`told_about_tray` exists so the "Azure is still running" notification on
first hide happens once ever, not once per session. A player who has learned
where the app went does not need telling again.

## Chords

```
Chord { mods: Modifiers, key: Key }
Action { ToggleEnabled, CycleNext, CyclePrev, RestoreDisplay, HoldBypass }
BindingSet: Action -> Option<Chord>
```

A chord must carry at least one modifier. A bare `V` registered globally
would eat the letter V everywhere in Windows, and the validation rejecting
it is a rule worth a test rather than a comment.

Defaults are the three the removed footer advertised, so the interface's old
promise becomes true rather than being quietly dropped:

| Action | Default |
|---|---|
| `ToggleEnabled` | `ALT+SHIFT+V` |
| `CycleNext` | `ALT+SHIFT+RIGHT` |
| `CyclePrev` | `ALT+SHIFT+LEFT` |
| `RestoreDisplay` | `CTRL+ALT+SHIFT+R` |
| `HoldBypass` | unset |

`HoldBypass` ships unset deliberately. A key you hold down mid-match is a
personal choice about what your hand is already near, and guessing wrong
means binding something a game needs.

`BindingSet::conflicts()` reports two actions sharing a chord. This is a
distinct failure from Windows refusing a registration, and the two are
reported separately because the fixes differ: one is Azure's fault, the
other is another program already holding the key.

## Registration

`RegisterHotKey` on a dedicated message-loop thread. Each bound action gets
an outcome, and the outcome is per-action rather than a single success flag:

```
Registration { action: Action, chord: Chord, outcome: Registered | Refused(String) }
```

Windows refuses a chord another process already holds, and it does not say
which process. The interface reports the refusal against the chord that
earned it, so a player who has bound the same key in Discord can see which
of their bindings did not take. Reporting a global "hotkeys failed" would be
the same lie as a boolean fidelity.

**Hold-to-bypass costs a poll.** `WM_HOTKEY` fires on press; Windows gives
no release event. On press the thread polls `GetAsyncKeyState` for that
chord's key every 16ms until it goes up, then fires the release. The poll
runs only while the key is held and stops the moment it is not.

**No keyboard hook, ever.** `SetWindowsHookEx(WH_KEYBOARD_LL)` would give
releases for free and is the obvious shortcut. It is forbidden here for the
same reason injection is: a tool sitting beside kernel anti-cheat with a
global keyboard hook is indistinguishable, to anyone auditing it, from the
thing anti-cheat exists to stop. This constraint joins the existing list in
`docs/superpowers/specs/2026-09-17-azure-design.md`.

## When hotkeys cannot fire

Windows UIPI blocks hotkeys registered by an unelevated process while an
elevated window has focus. Azure runs unelevated by default and says so when
it matters rather than failing silently.

The signal is already in the codebase. `win::watch` calls `OpenProcess` with
`PROCESS_QUERY_LIMITED_INFORMATION`, which an elevated foreground process
refuses; `PresetSet::match_foreground` already documents the missing path as
"the normal case for an elevated game". That refusal becomes
`Foreground::elevated`, and the interface says *hotkeys are blocked while an
elevated window has focus* at the moment it is true.

A **Restart elevated** action appears in that moment, not permanently in
settings. It relaunches via `ShellExecuteW` with the `runas` verb and exits.
Offering it only when it would help keeps Azure from asking for admin as a
matter of course, which next to anti-cheat is the worst first impression
available.

## Exclusive fullscreen

`SHQueryUserNotificationState()` returning `QUNS_RUNNING_D3D_FULL_SCREEN`,
probed on each foreground settle and fed into `Environment`. Read-only,
no injection, and documented by Windows for exactly this question.

Its semantics are right for free: *borderless* fullscreen does not report
D3D fullscreen, and borderless is precisely the case where DWM still
composites and the matrix channels still work. A window-rectangle heuristic
would get that backwards and mark the matrix inert when it is not.

## The tray

`TrayIconBuilder`, which needs the `tray-icon` feature on `tauri`.

- Left click shows the window and focuses it.
- Menu: **Azure enabled** (checkbox) · **Presets** (radio submenu, the
  active one checked) · **Restore display** · **Open Azure** · **Quit**.
- Tooltip is the active preset's name, so hovering answers the only
  question a hidden app gets asked.

The menu is a control surface, not a second interface. Anything that needs
explaining belongs in the window.

## Lifecycle

- `CloseRequested` hides the window and prevents the close. The engine and
  the watcher keep running; this is the entire point of the milestone.
- The first hide raises a tray notification saying Azure is still running
  and where to find it, once ever, recorded by `told_about_tray`.
- **Quit** is the only real exit, and it restores the display on the way
  out.
- Autostart is opt-in, default off: a `Run` key under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` holding
  `"<exe>" --hidden`. Written directly — the registry code for the ICM
  gamma unlock already lives in `azure-display`, and a plugin for one value
  would be a dependency for nothing.
- `--hidden` starts without showing the window.

## Recovery

Already built, in the presets milestone, and left off its "As built" list.
`EngineHandle::start` reads the marker at spawn, restores both backends
before anything else runs, clears it and pushes a notice
(`thread.rs:214`); `a_session_that_died_dirty_is_cleaned_up_at_the_next_start`
covers it.

What M7 contributes is not code but the marker's meaning. Because quit
restores, a marker can only have been left by a run that died — there is no
second path that leaves one behind deliberately. That single meaning is what
makes recovering automatically, without asking, the safe thing to do.

## Interface

- A settings panel for bindings: a capture widget per action, the current
  chord, and any refusal reported against it.
- The blocked-hotkey notice, with **Restart elevated** beside it, shown only
  while an elevated window holds focus.
- An autostart toggle.
- The existing footer regains its hotkey strip, now reading real bindings
  rather than three hardcoded strings.

## Testing

`azure-settings` is pure and carries the rules, so it carries the tests:
chord parse and render round-trip, rejection of a modifier-less chord,
conflict detection, defaults, and the store's load/save/corrupt-file
behaviour mirroring the preset store's.

Recovery is testable without a display: a temporary directory, a marker
written by hand, mock backends, and an assertion that the neutralise
happened and the notice is present.

Registration is inherently the OS, so it gets the treatment the real engine
got in M1 — one test that registers an improbable chord, asserts it took,
registers it again and asserts the second is refused.

The tray and the window lifecycle cannot be unit tested. They are verified
by running the app, and what was verified gets written into an "As built"
section here, as the watcher's was.

## As built

Implemented 2026-09-19. 161 tests pass across the four crates, clippy is
clean and `bun run build` is clean. Where the work differs from the design
above:

- **Dirty recovery was already built.** It was written in the presets
  milestone and left off that milestone's "As built" list, which is what
  made it look outstanding. Nothing was added here; the Recovery section
  above was rewritten to say so.
- **`Action::label()` was removed rather than added.** Putting the labels
  in Rust would have been a second copy of strings that already live in
  `src/lib/model.ts` with every other label the interface prints.
- **The first-hide notice is a message box, not a tray balloon.** Tauri's
  tray exposes no balloon, and `tauri-plugin-dialog` is already a
  dependency. A modal also cannot be missed, which for a window that has
  just vanished is the point.
- **Cycling visits the desktop preset.** It was tempting to skip it, but a
  cycle key that cannot reach the state where Azure leaves the display
  alone is a key that cannot undo itself mid-match.
- **`ToggleEnabled` is its own engine message** rather than a read
  followed by a write. Two presses landing either side of a read would
  otherwise leave the display disagreeing with the tray checkbox.
- **A third diagnostic example**, `cargo run -p azure-display --example
  keys`, prints the registrations and every press. It is how the delivery
  path below was verified, and it binds `ALT+PAUSE` so the release half
  can be watched.

Verified on the machine it was written on. Azure was started with
`--hidden`: it ran with no window at 40MB, and a second process asking
Windows for `ALT+SHIFT+V` was refused with error 1409 — Azure held the
chord system-wide with nothing on screen. Driving real key events through
`keybd_event` against the diagnostic produced `ToggleEnabled Pressed`,
`CycleNext Pressed`, and — for a held `ALT+PAUSE` — `HoldBypass Pressed`
followed by `HoldBypass Released`, which is the poll standing in for the
release Windows does not send.

One note for whoever runs the suite in a sandbox: the autostart test
writes a real `Run` value and reads it back. A sandbox that virtualises
registry writes reports success and then hands back nothing, and the test
fails there while passing on the machine. It was left strict rather than
softened into a skip, because a skip would also hide a genuinely wrong
key path.

## Known gap

An unelevated Azure cannot see the executable path of an elevated game, so
such a game matches by executable name only — already true before this
milestone, and `MatchKind` already says which match happened. Restarting
elevated fixes it for players who care.
