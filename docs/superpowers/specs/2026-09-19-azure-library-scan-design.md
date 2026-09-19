# Azure M5 — Scanning the library

The last milestone. Presets match a foreground executable; today the only
ways to get one are to browse for an .exe or to capture the next window.
Both work, and both ask a player who has two hundred games to do clerical
work Azure could do for them.

## Problem

`PRODUCT.md` promises auto-detection across seven launchers. The button
that said `SCAN LIBRARIES` was deleted in M1 for advertising a scanner
that did not exist. This is that scanner.

The job is not "find games". It is **produce an executable name that will
still be the foreground process when the game is running**, which is a
different and harder question. A launcher knows where it installed a
title; it does not always know which of the eleven .exe files in that
directory is the one that ends up on screen.

## What each launcher actually gives

Checked against a real machine while writing this, not from memory.

| Launcher | Source | What it yields |
|---|---|---|
| **Epic** | `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`, JSON | `DisplayName`, `InstallLocation`, **`LaunchExecutable`** — the exact binary, no guessing |
| **Steam** | `HKCU\Software\Valve\Steam\SteamPath` → `steamapps\libraryfolders.vdf` → each library's `appmanifest_*.acf` | `name`, `installdir`; the executable must be found in the directory |
| **Ubisoft** | `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>\InstallDir` | a directory; no name, no executable |
| **GOG** | `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<id>` | `gameName`, `path`, `exe` — name and binary both |
| **EA** | `HKLM\SOFTWARE\WOW6432Node\Electronic Arts\<title>\Install Dir` | a directory |
| **Battle.net** | `%APPDATA%\Battle.net\Battle.net.config`, JSON | per-game install paths |
| **Xbox / MS Store** | — | see **The Xbox gap** |

Three shapes, then: one launcher hands over the executable, four hand over
a directory, and one hands over nothing Azure is allowed to read.

## Ownership

`azure-scan` is new. The parsing and the choosing are pure and take paths
and directory listings as arguments, so the tests feed fixtures rather
than needing seven launchers installed. Only `azure-scan::win::roots`
reads the registry.

This matters more here than elsewhere: most of this code can never be
exercised on the machine that writes it. Six launchers cannot all be
installed, so the logic has to be testable without any of them.

## Choosing the executable

The interesting half. A game directory holds the game, its crash handler,
its anti-cheat bootstrapper, two redistributable installers and an
uninstaller. Picking wrong means a preset that never activates.

`choose(listing, game_name) -> Option<Chosen>`, pure, where `Chosen`
carries a confidence that is reported rather than hidden:

- **`Exact`** — the launcher named it. Epic and GOG only.
- **`Likely`** — one candidate survived after the noise was removed, or
  one matches the game's name.
- **`Guess`** — several survived; the largest wins and the interface says
  it is a guess.

Rules, in order:

1. Discard by name: anything matching `*crashhandler*`, `*crashreport*`,
   `unins*`, `*vcredist*`, `*directx*`, `dxsetup`, `*dotnet*`,
   `*prereq*`, `*_setup`, `*redist*`, `*EasyAntiCheat_Setup*`,
   `*touchup*`, `*helper*`, `*subprocess*` (Unreal's CEF child).
2. Discard by directory: `_CommonRedist`, `Redist`, `DirectX`,
   `DotNet`, `EasyAntiCheat`, `BattlEye`, `Engine\Binaries\ThirdParty`.
3. Prefer a stem resembling the game's name, compared with punctuation
   and spaces removed — `Tom Clancy's Rainbow Six Siege` against
   `RainbowSix.exe` is the case that must work.
4. Prefer shallower paths, then larger files.

**Anti-cheat variants are kept, not filtered.** A title may ship
`RainbowSix.exe`, `RainbowSix_BE.exe` and `RainbowSix_Vulkan.exe`, and
which one reaches the foreground depends on how the player launches it.
All of them are offered; binding the wrong one costs a preset that does
not fire, and the capture-foreground button already exists to settle it.

The walk is bounded — four directories deep, and it stops after a
thousand files in one game — because a library on a slow disk is not
worth an unbounded scan.

## Filtering what is not a game

Steam lists `Steamworks Common Redistributables`, Proton and the Linux
runtimes as ordinary apps; they have appmanifests exactly like a game.
They are dropped by name (`Steamworks Common`, `Proton`, `Steam Linux
Runtime`, `Steam Controller`) rather than by an appid list, because an
appid list would need maintaining and a name match will not.

A directory that yields no executable at all is not reported as a game
with no executable. It is not reported.

## The Xbox gap

Xbox and Microsoft Store games install under
`C:\Program Files\WindowsApps`, which is ACL-locked: Azure, running
unelevated and deliberately so, cannot list it. The package registry can
be read, but distinguishing 42 store packages into games and not-games
from their identity names alone is guesswork.

So Azure does not scan it, and says exactly that — the scan reports Xbox
as a source it cannot read, and points at capture-foreground, which works
perfectly for these titles because the watcher sees the foreground
process regardless of where it lives. Reporting `0 games found` would be
a lie; reporting the reason is not.

## Reporting

Every source answers, and the answers are per source:

```
SourceReport { launcher, outcome: Scanned { found } | NotInstalled | Unreadable { reason } }
```

`NotInstalled` is the ordinary case for most players and is not a
failure. A player with Steam and Epic should see two sources scanned and
five quietly absent, not five errors.

## What a scan does not do

It does not create presets. It produces candidates, the interface shows
them, and the player chooses. A scanner that made two hundred presets
would be a scanner that made two hundred things to delete — and the
desktop preset plus a handful of games is what this product is for.

Candidates already bound to a preset are marked as such, so a second scan
is useful rather than a list of things already done.

## Testing

The parsers get fixtures taken from the real files: a two-library
`libraryfolders.vdf`, an `appmanifest` including the redistributables
entry that must be dropped, an Epic `.item` with a forward-slashed
`LaunchExecutable`, a Battle.net config, GOG and EA registry shapes as
the values they would return.

`choose` gets the case that matters as a named test: a Rainbow Six
listing containing the game, its BattlEye bootstrapper, a crash handler
and a redistributable installer, asserting the game comes first and the
crash handler is gone.

The end-to-end scan runs against whatever this machine has — Steam, Epic
and Ubisoft are installed here — and asserts it answers for all seven
sources without panicking, rather than asserting a game count that would
differ on any other machine.

## As built

Implemented 2026-09-19. 207 tests pass across the five crates, clippy is
clean and `bun run build` is clean. Where the work differs from the
design above:

- **Acronym matching was added after the first real run, because that
  run was wrong about the most important game in the product.** Against
  the real Steam library, `Counter-Strike 2` resolved to
  `vconsole2.exe` — the Source 2 developer console, which sits in the
  same folder and is larger than the game. The rule in the spec matched
  runs of words (`Rainbow Six`) but not acronyms (`cs2`), so nothing
  scored and size decided. Titles are now split on punctuation as well
  as spaces, numbers are kept whole, and `Counter-Strike 2` → `cs2` is a
  named test.
- **Confidence was being computed from the final score**, which included
  the depth penalty. `cs2.exe` lives at `game/bin/win64/`, so a perfect
  name match came back labelled `Guess`. Whether the name matched is now
  tracked separately from how the candidate scored; position and size
  move the score without saying anything about whether the binary is
  right.
- **An unterminated string in a VDF file is no longer a value.** The
  truncated-file test caught the parser handing back `G:\Steam` from a
  file cut off mid-write, which may be the first half of
  `G:\SteamLibrary` — a path that points somewhere real and wrong is
  worse than no path.
- **`Outcome` was renamed `SourceOutcome`.** `azure-display::hotkeys`
  already exports an `Outcome`, and ts-rs writes one file per type name
  into one directory, so the two silently overwrote each other depending
  on which crate's tests ran last. `SourceReport.ts` was importing a
  type describing hotkey registrations.
- **Adding a scanned game does not select it.** Selecting applies the
  preset, and adding four games in a row would have changed the display
  four times under someone who was only filling in a list.

Verified against the real library on this machine: 93 candidates in
146ms across Steam (87), Epic (5) and Ubisoft (1), with GOG, EA and
Battle.net correctly reporting not-installed and Xbox reporting why it
cannot be read. `Counter-Strike 2` resolves to `cs2.exe` and
`Tom Clancy's Rainbow Six Siege` to `RainbowSix.exe`, both `Likely`. The
entries still marked `Guess` are the honest ones — `gmod.exe`,
`isaac-ng.exe`, `Battles-Win.exe` — where the executable shares nothing
with the title and the label says so.

Four launchers could not be exercised here because they are not
installed. Their parsing is covered by fixtures taken from the real file
formats; their registry and filesystem plumbing is not covered by
anything, and the first player who has GOG installed is the test.
