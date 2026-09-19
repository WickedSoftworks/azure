import { cn } from "cn";
import type { DisplayInfo, Environment, Preset } from "@/lib/model";

interface Props {
  environment: Environment;
  displays: DisplayInfo[];
  enabled: boolean;
  preset: Preset | undefined;
  /** True when the watcher chose this preset rather than the person. */
  automatic: boolean;
  /** False in a browser session, where nothing reaches a display. */
  live: boolean;
  onToggle: () => void;
}

export function StatusStrip({
  environment,
  displays,
  enabled,
  preset,
  automatic,
  live,
  onToggle,
}: Props) {
  const hdr = environment.hdrActive;

  return (
    <header className="ng-rule-b flex items-center gap-4 bg-field px-4 h-9 shrink-0">
      <button
        type="button"
        onClick={onToggle}
        aria-pressed={enabled}
        disabled={!live}
        className={cn(
          "px-2 py-0.5 tracking-[var(--ng-track)] uppercase font-bold transition-none",
          enabled && live ? "ng-reverse" : "bg-plate text-dim",
        )}
      >
        {!live ? "NO CORE" : enabled ? "ACTIVE" : "OFF"}
      </button>

      <span className="ng-value truncate">{preset?.name ?? "—"}</span>

      <span className="ng-label hidden sm:inline">
        {automatic ? "activated by focus" : "selected by hand"}
        {enabled && live ? " · applied" : " · not applied"}
      </span>

      <div className="ml-auto flex items-center gap-4">
        {environment.colorFiltersActive && (
          <span className="ng-label text-alert">COLOUR FILTERS</span>
        )}
        {environment.exclusiveFullscreen && (
          <span className="ng-label text-alert">EXCLUSIVE FULLSCREEN</span>
        )}
        <span className={cn("ng-label", hdr && "text-warn")}>{hdr ? "HDR" : "SDR"}</span>
        <span className="ng-label hidden sm:inline">
          {displays.length} {displays.length === 1 ? "DISPLAY" : "DISPLAYS"}
        </span>
      </div>
    </header>
  );
}
