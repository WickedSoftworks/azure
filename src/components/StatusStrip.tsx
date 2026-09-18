import { cn } from "cn";
import type { Preset, SessionState } from "@/lib/model";

interface Props {
  session: SessionState;
  preset: Preset;
  onToggle: () => void;
}

export function StatusStrip({ session, preset, onToggle }: Props) {
  const { enabled, matchedBy, exclusiveFullscreen, displays } = session;
  const hdr = displays.some((d) => d.hdr);

  return (
    <header className="ng-rule-b flex items-center gap-4 bg-field px-4 h-9 shrink-0">
      <button
        type="button"
        onClick={onToggle}
        aria-pressed={enabled}
        className={cn(
          "px-2 py-0.5 tracking-[var(--ng-track)] uppercase font-bold transition-none",
          enabled ? "ng-reverse" : "bg-plate text-dim",
        )}
      >
        {enabled ? "ACTIVE" : "OFF"}
      </button>

      <span className="ng-value truncate">{preset.name}</span>

      {matchedBy && (
        <span className="ng-label hidden sm:inline">
          {preset.mode} · matched by {matchedBy}
        </span>
      )}

      <div className="ml-auto flex items-center gap-4">
        {exclusiveFullscreen && (
          <span className="ng-label text-alert">EXCLUSIVE FULLSCREEN</span>
        )}
        <span className={cn("ng-label", hdr && "text-warn")}>
          {hdr ? "HDR" : "SDR"}
        </span>
        <span className="ng-label hidden sm:inline">
          {displays.length} {displays.length === 1 ? "DISPLAY" : "DISPLAYS"}
        </span>
      </div>
    </header>
  );
}
