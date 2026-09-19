import { cn } from "cn";
import { isInert, type ChannelReport, type ColorState, type Stage } from "@/lib/model";

interface Props {
  channels: ChannelReport[];
  state: ColorState;
  /** Increments on every committed change; drives the propagation pulse. */
  pulse: number;
  pulseStage: Stage | null;
}

/**
 * The device path, drawn literally, because the whole product is the claim
 * that two APIs cover each other's blind spots. The order is not a design
 * choice — the compositor runs before scanout, always.
 *
 * Which channels each stage carries is the core's answer, not this file's:
 * on an HDR display the affine group moves to the matrix, and the drawing
 * has to move with it.
 */
export function SignalChain({ channels, state, pulse, pulseStage }: Props) {
  const stages: { stage: Stage; name: string; api: string; ops: string }[] = [
    {
      stage: "matrix",
      name: "MATRIX",
      api: "MagSetFullscreenColorEffect",
      ops: "mix",
    },
    {
      stage: "lut",
      name: "LUT",
      api: "SetDeviceGammaRamp",
      ops: "affine · power",
    },
  ];

  const inLut = channels.filter((c) => c.stage === "lut" && !isInert(c.fidelity)).length;
  const stranded = channels.filter((c) => c.stage === null);

  return (
    <div className="flex flex-col gap-0 px-6 py-5">
      <span className="ng-label">SIGNAL CHAIN</span>

      <div className="mt-3 flex flex-col">
        <span className="ng-label text-text">FRAMEBUFFER</span>

        {stages.map(({ stage, name, api, ops }) => {
          const here = channels.filter((c) => c.stage === stage);
          const carried = here.filter((c) => state[c.id] !== c.range.neutral);
          // A stage with nothing routed to it, or whose channels all came
          // back inert, is not carrying anything on this machine.
          const inert = here.length === 0 || here.every((c) => isInert(c.fidelity));
          const inertNote = here.find((c) => isInert(c.fidelity))?.note;

          return (
            <div key={stage} className="relative pl-4 py-2">
              <span
                aria-hidden
                className="absolute left-0 top-0 bottom-0 w-px bg-rule"
              />
              <span aria-hidden className="absolute left-0 top-1/2 h-px w-3 bg-rule" />

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
                <span
                  className={cn(
                    "ng-label",
                    carried.length > 0 && !inert && "text-signal",
                  )}
                >
                  {carried.length > 0 ? carried.map((c) => c.key).join(" ") : "—"}
                </span>
              </div>
              <div className="ng-selectable px-2 text-dim">
                {inert && inertNote ? inertNote : api}
              </div>
            </div>
          );
        })}

        <span className="ng-label text-text">PANEL</span>
      </div>

      <p className="mt-4 max-w-[52ch] text-dim">
        {inLut > 0 ? (
          <>
            {inLut} of {channels.length} channels live in the scanout LUT, so they
            survive both exclusive fullscreen and a full exit.
          </>
        ) : (
          <>
            Nothing is in the scanout LUT on this machine, so no channel survives
            exclusive fullscreen or a full exit.
          </>
        )}
        {stranded.length > 0 && (
          <>
            {" "}
            {stranded.map((c) => c.key).join(", ")}{" "}
            {stranded.length === 1 ? "has" : "have"} no path here at all.
          </>
        )}
      </p>
    </div>
  );
}
