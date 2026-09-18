/**
 * SYNTHETIC DATA — replace with real IPC in M1/M5/M6.
 *
 * Every preset, executable path and display name below is invented so the
 * surface can be built and reviewed before the Rust core exists. None of it
 * is a claim: no benchmark, no user count, no capability the product does
 * not have. The routing and fidelity values ARE truthful — they are what
 * the two vendor-neutral backends actually produce on an SDR desktop.
 */

import { NEUTRAL, type ColorState, type Preset, type SessionState } from "./model";

const withState = (over: Partial<ColorState>): ColorState => ({ ...NEUTRAL, ...over });

export const DEMO_PRESETS: Preset[] = [
  {
    id: "valorant",
    name: "VALORANT",
    exe: "D:\\Riot Games\\VALORANT\\live\\ShooterGame\\Binaries\\Win64\\VALORANT-Win64-Shipping.exe",
    mode: "focused",
    state: withState({ vibrance: 156, contrast: 108, gamma: 112 }),
  },
  {
    id: "cs2",
    name: "CS2",
    exe: "D:\\SteamLibrary\\steamapps\\common\\Counter-Strike Global Offensive\\game\\bin\\win64\\cs2.exe",
    mode: "focused",
    state: withState({ vibrance: 178, saturation: 106, gamma: 118 }),
  },
  {
    id: "apex",
    name: "APEX LEGENDS",
    exe: "D:\\SteamLibrary\\steamapps\\common\\Apex Legends\\r5apex.exe",
    mode: "running",
    state: withState({ vibrance: 138, brightness: 6, gamma: 124 }),
  },
  {
    id: "desktop",
    name: "DESKTOP",
    exe: null,
    mode: "manual",
    state: withState({ temperature: -14 }),
  },
];

export const DEMO_SESSION: SessionState = {
  enabled: true,
  activePresetId: "valorant",
  matchedBy: "full path",
  exclusiveFullscreen: false,
  gammaRangeUnlocked: false,
  displays: [
    { key: "DISPLAY#AUS27A1#5&1f2c", name: "ASUS PG27AQN", primary: true, hdr: false },
    { key: "DISPLAY#DEL2722#4&0b91", name: "DELL S2722DGM", primary: false, hdr: false },
  ],
};
