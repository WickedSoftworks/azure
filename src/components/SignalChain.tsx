import { cn } from "cn";
import { CHANNELS, isInert, type ColorState, type Stage } from "@/lib/model";

interface Props {
  state: ColorState;
  /** Increments on every committed change; drives the propagation pulse. */
  pulse: number;
  pulseStage: Stage | null;
  exclusiveFullscreen: boolean;
}

/**
 * The device path, drawn literally, because the whole product is the claim
 * that two APIs cover each other's blind spots. The order is not a design
 * choice — the compositor runs before scanout, always.
 */
export function SignalChain({ state, pulse, pulseStage, exclusiveFullscreen }: Props) {
  const stages: {
    stage: Stage;
    name: string;
    api: string;
    ops: string;
    inert: boolean;
    inertNote?: string;
  }[] = [
    {
      stage: "matrix",
      name: "MATRIX",
      api: "MagSetFullscreenColorEffect",
      ops: "mix",
      inert: exclusiveFullscreen,
      inertNote: "DWM bypassed in exclusive fullscreen",
    },
    {
      stage: "lut",
      name: "LUT",
      api: "SetDeviceGammaRamp",
      ops: "affine · power",
      inert: false,
    },
  ];

  return (
    <div className="flex flex-col gap-0 px-6 py-5">
      <span className="ng-label">SIGNAL CHAIN</span>

      <div className="mt-3 flex flex-col">
        <span className="ng-label text-text">FRAMEBUFFER</span>

        {stages.map(({ stage, name, api, ops, inert, inertNote }) => {
          const carried = CHANNELS.filter(
            (c) => c.stage === stage && state[c.id] !== c.neutral,
          );
          return (
            <div key={stage} className="relative pl-4 py-2">
              <span
                aria-hidden
                className="absolute left-0 top-0 bottom-0 w-px bg-rule"
              />
              <span
                aria-hidden
                className="absolute left-0 top-1/2 h-px w-3 bg-rule"
              />

              {/* The authored moment: the change travelling to the panel. */}
              {pulseStage === stage && (
                <span
                  key={pulse}
                  aria-hidden
                  className="absolute left-0 top-1/2 h-px w-full origin-left bg-signal"
                  style={{ animation: "ng-propagate 620ms cubic-bezier(0.16,1,0.3,1)" }}
                />
              )}

              <div
                className={cn(
                  "flex flex-wrap items-baseline gap-x-3 px-2 py-1",
                  inert && "ng-hatch",
                )}
              >
                <span className={cn("ng-value", inert && "text-dim")}>{name}</span>
                <span className="ng-label">{ops}</span>
                <span className={cn("ng-label", carried.length > 0 && !inert && "text-signal")}>
                  {carried.length > 0
                    ? carried.map((c) => c.key).join(" ")
                    : "—"}
                </span>
              </div>
              <div className="px-2 text-dim">{inert ? inertNote : api}</div>
            </div>
          );
        })}

        <span className="ng-label text-text">PANEL</span>
      </div>

      <p className="mt-4 max-w-[52ch] text-dim">
        {CHANNELS.filter((c) => c.stage === "lut" && !isInert(c.fidelity)).length} of{" "}
        {CHANNELS.length} channels live in the scanout LUT, so they survive both
        exclusive fullscreen and a full exit.
      </p>
    </div>
  );
}
