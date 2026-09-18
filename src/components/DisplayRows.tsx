import { cn } from "cn";
import type { DisplayInfo } from "@/lib/model";

interface Props {
  displays: DisplayInfo[];
  /** Matrix is desktop-wide; the LUT is the only per-display stage. */
  lutTarget: string | "all";
  onTarget: (key: string | "all") => void;
}

/**
 * The asymmetry users trip over: the compositor matrix covers every display
 * at once, while the scanout LUT is per-monitor. Hiding that produces bug
 * reports; stating it produces understanding.
 */
export function DisplayRows({ displays, lutTarget, onTarget }: Props) {
  return (
    <section className="ng-rule-t">
      <div className="ng-rule-b grid grid-cols-[1fr_6ch] sm:grid-cols-[1fr_6ch_7ch_7ch] items-center gap-x-3 px-4 py-1.5">
        <span className="ng-label">DISPLAY</span>
        <span className="ng-label text-right">SIGNAL</span>
        <span className="ng-label hidden sm:block">MATRIX</span>
        <span className="ng-label hidden sm:block">LUT</span>
      </div>

      {displays.map((d) => {
        const lutOn = lutTarget === "all" || lutTarget === d.key;
        return (
          <button
            key={d.key}
            type="button"
            onClick={() => onTarget(lutTarget === d.key ? "all" : d.key)}
            className={cn(
              "grid w-full grid-cols-[1fr_6ch] sm:grid-cols-[1fr_6ch_7ch_7ch] items-center gap-x-3",
              "px-4 h-[var(--ng-cell)] text-left hover:bg-plate/60",
            )}
          >
            <span className="flex min-w-0 items-baseline gap-2">
              <span className="ng-value truncate">{d.name}</span>
              {d.primary && <span className="ng-label shrink-0">PRIMARY</span>}
            </span>

            <span
              className={cn("ng-label text-right", d.hdr ? "text-warn" : "text-ok")}
            >
              {d.hdr ? "HDR" : "SDR"}
            </span>

            <span className="ng-label hidden sm:block text-ok">ALL</span>

            <span
              className={cn(
                "ng-label hidden sm:block px-1",
                lutOn ? "text-ok" : "text-dim ng-hatch",
              )}
            >
              {lutOn ? "ON" : "OFF"}
            </span>
          </button>
        );
      })}

      <p className="px-4 pb-3 pt-1 text-dim">
        {lutTarget === "all"
          ? "LUT channels apply to every display. Click one to target it alone."
          : "LUT channels target one display. Click it again to apply to all."}
      </p>
    </section>
  );
}
