# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri v2 (Rust core) + React + Vite + shadcn/ui on Tailwind v4, TypeScript. **bun** is the package manager and script runner; only `bun.lock` is committed. Windows-only target, NSIS installer. Chosen by the user over Electron explicitly for small bundle size and low idle memory, accepting that the native layer is Rust rather than TypeScript.

## Users

**Primary: competitive FPS players on Windows** — CS2, Valorant, Apex, Rainbow Six. They raise digital vibrance to make enemy player models separate from the environment. This is an established habit with an established tool (vibranceGUI), which is NVIDIA-only.

Their situation defines the product: they configure it once, then **never want to see it again**. The app's success condition is that it is invisible and instant — correct preset on game launch, hotkey toggle mid-match, no window, no interruption, no overlay. Time spent inside the app's UI is a cost, not engagement.

Secondary, unserved by existing tools and reached for free: anyone on AMD or Intel graphics who has never had a per-game vibrance tool at all.

## Product Purpose

Per-game display colour control — vibrance, saturation, brightness, contrast, gamma, hue — that works on **any GPU vendor**, applies itself automatically when a game launches, and can be toggled with a global hotkey from inside the game.

Success: a player installs it, scans their library, sets vibrance once for their main game, and never opens the window again — and it works identically whether they are on NVIDIA, AMD or Intel.

## Positioning

The two neighbouring tools each have a gap this one closes:

- **vibranceGUI** — the category leader for competitive players, but NVIDIA-only. AMD and Intel users have no equivalent.
- **GameHaff/DigitalVibrance** — vendor-neutral, but built on a single API (`MagSetFullscreenColorEffect`) that is mathematically incapable of gamma and is dropped by Windows the instant the process exits.

Azure's mechanism is the part neither has: **two complementary vendor-agnostic APIs routed by capability**. The DWM colour matrix handles channel mixing (saturation, hue, vibrance); the GPU hardware LUT handles the tone curve (gamma, brightness, contrast, temperature) — and the LUT half persists after exit and keeps working in exclusive-fullscreen games. Neither competitor can claim both vendor-neutrality and gamma.

A consequence worth stating as positioning: because four of six controls live in the scanout LUT, four of six keep working in exclusive fullscreen, where the compositor-based competitor does nothing at all.

## Operating Context

- Used on a gaming desktop, usually mid-session, often with the game already fullscreen. The hotkey is the primary interface; the window is the setup interface.
- Launchers in play: Steam, Epic, GOG, EA, Ubisoft Connect, Battle.net, Xbox/MS Store. Libraries commonly run to hundreds of titles across several drives.
- Runs alongside kernel anti-cheat (EAC, BattlEye, Vanguard). Perceived safety is a hard requirement, not a nicety.
- Coexists with software competing for the same display state: Night Light, f.lux, ICC profile loaders, vendor control panels, Windows Colour Filters.
- Monitors are increasingly HDR, which degrades both backends and must be surfaced rather than hidden.

## Capabilities and Constraints

**Capabilities:** six colour controls plus temperature/tint; per-game presets; library auto-detection across seven launchers with manual add and capture-foreground fallbacks; automatic preset switching on focus or process lifetime; global hotkeys including master toggle and hold-to-bypass; tray residency; bake-on-exit persistence; per-monitor targeting for LUT channels.

**Hard constraints, all consequences of the chosen mechanism:**
- The DWM matrix cannot express gamma (it is affine) and does not survive process exit.
- The hardware LUT cannot express saturation or hue (no channel mixing).
- Device order is fixed: framebuffer → matrix → LUT → panel. No design may assume gamma before saturation.
- True nonlinear vibrance is unreachable without a 3D LUT; v1 vibrance is honestly a saturation curve and must be labelled as approximate.
- Exclusive fullscreen bypasses DWM, so matrix channels are inert there.
- HDR makes LUT behaviour undefined and matrix behaviour unreliable.
- Global hotkeys do not fire while an elevated window has focus unless the app is also elevated (Windows UIPI).
- **No injection of any kind** — no DLL injection, no in-process hooks, no overlay, no driver, no process memory reads. This is load-bearing for anti-cheat coexistence and rules out otherwise-attractive implementations.

**Critical UI constraint:** the effect is applied to the entire desktop, *including Azure's own window*. Any in-app "after" preview would be double-transformed and therefore a lie. Honest comparison must be press-and-hold-to-see-original against the real screen.

## Brand Commitments

Name: **Azure**, confirmed by the user over the alternative. Noted for a public release: it collides with Microsoft Azure in search and conversation — a distribution consideration, not a product one.

## Evidence on Hand

None yet — greenfield repository with no commits. There are no users, benchmarks, testimonials, reviews, download counts or press to cite, and none may be fabricated. Competitor behaviour cited above is from their public documentation and source. Performance figures (bundle size, idle memory) are targets, not measurements, until M9.

## Product Principles

1. **Invisible by default.** The window is setup; the hotkey is the product. Every design decision that adds a step between launching a game and playing it is wrong.
2. **Never lie about what landed.** Vendor-agnostic colour control is full of silent failures — clamped ramps, inert matrices in fullscreen, HDR degradation. The app reports what actually applied, per channel, always.
3. **Never strand the user's display.** The worst outcome is an unusable screen and no visible way to fix it. Safety assessment, arm-and-confirm, an always-bound emergency restore hotkey, and recovery on unclean shutdown are non-negotiable.
4. **Detection failure must always be survivable.** Every scanner miss has a two-second manual escape hatch.
5. **Earn anti-cheat trust explicitly.** Document every Win32 call; never do anything that looks like tampering even when it would be convenient.

## Accessibility & Inclusion

- Windows Colour Filters occupies the identical fullscreen colour-effect slot. Azure must detect it and yield, never silently disable someone's accessibility setting.
- Colour is the product's subject matter, so it can never be the sole carrier of state in the interface: routing, fidelity and warning states need text or shape as well as hue.
- Full keyboard operation, including hotkey capture. Public release, so English-only at launch but strings centralised for later translation — the closest competitor ships 16 languages.
