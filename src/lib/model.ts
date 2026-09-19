/**
 * Presentation only.
 *
 * The channel table used to live here as a hand-written constant. It now
 * comes from the Rust core, because what a channel costs, which stage
 * carries it and how faithfully it lands are answers only the core can
 * give truthfully — they change with the machine. What is left is how to
 * print a number and what to call a state.
 */

import type { ChannelRange, Fidelity, Stage } from "./bindings";

export type {
  ApplyReport,
  ChannelId,
  ChannelRange,
  ChannelReport,
  ColorState,
  DisplayInfo,
  Environment,
  Fidelity,
  GammaRangeOutcome,
  LutTarget,
  Snapshot,
  Stage,
  StageLanding,
} from "./bindings";

export function formatValue(range: ChannelRange, raw: number): string {
  switch (range.unit) {
    case "degrees":
      return `${raw > 0 ? "+" : ""}${raw}°`;
    case "factor":
      return (raw / 100).toFixed(2);
    case "percent":
      return range.neutral === 0 ? `${raw > 0 ? "+" : ""}${raw}%` : `${raw}%`;
  }
}

/** True when a channel is doing nothing to the screen right now. */
export function isInert(fidelity: Fidelity): boolean {
  return fidelity === "inert" || fidelity === "unrealised";
}

export const FIDELITY_LABEL: Record<Fidelity, string> = {
  exact: "EXACT",
  approximate: "APPROX",
  clamped: "CLAMPED",
  inert: "INERT",
  unrealised: "NO PATH",
};

/** `null` means no backend on this machine can carry the channel. */
export function stageLabel(stage: Stage | null): string {
  if (stage === null) return "—";
  return stage === "matrix" ? "MATRIX" : "LUT";
}
