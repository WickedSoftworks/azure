import { cn } from "cn";
import type { DisplayInfo, Environment } from "@/lib/model";

interface Props {
  environment: Environment;
  displays: DisplayInfo[];
  enabled: boolean;
  /** False in a browser session, where nothing reaches a display. */
  live: boolean;
  onToggle: () => void;
}

export function StatusStrip({ environment, displays, enabled, live, onToggle }: Props) {
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

      <span className="ng-value truncate">DESKTOP</span>

      <span className="ng-label hidden sm:inline">
        {enabled && live ? "applied live" : "not applied"}
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
