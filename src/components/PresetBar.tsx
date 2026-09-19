import { cn } from "cn";
import type { ColorState } from "@/lib/model";

/**
 * Presets land in M4 with persistence and in M5 with the scanners. The
 * shape lives here, beside the only thing that draws it, until there is a
 * core type to replace it.
 */
export interface Preset {
  id: string;
  name: string;
  /** null for the desktop default, which binds to nothing. */
  exe: string | null;
  state: ColorState;
}

interface Props {
  presets: Preset[];
  activeId: string;
  onSelect: (id: string) => void;
  /** Absent until the library scanner exists. A button that does nothing
   *  is worse than no button. */
  onScan?: () => void;
}

function basename(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export function PresetBar({ presets, activeId, onSelect, onScan }: Props) {
  const active = presets.find((p) => p.id === activeId);

  if (presets.length === 0) {
    return (
      <section className="ng-rule-t flex flex-wrap items-baseline gap-x-4 gap-y-1 bg-field px-4 py-3 shrink-0">
        <span className="ng-label">NO PRESETS</span>
        <p className="text-dim">
          Per-game presets arrive with the library scanner. Until then Azure
          holds one state and applies it to the desktop.
        </p>
        {onScan && (
          <button
            type="button"
            onClick={onScan}
            className="ml-auto bg-plate px-3 py-1 ng-label text-text hover:bg-rule"
          >
            SCAN LIBRARIES
          </button>
        )}
      </section>
    );
  }

  return (
    <section className="ng-rule-t bg-field px-4 py-3 shrink-0">
      <div className="flex flex-wrap items-center gap-2">
        <span className="ng-label mr-1">PRESETS</span>
        {presets.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => onSelect(p.id)}
            aria-current={p.id === activeId}
            className={cn(
              "px-2 py-0.5 uppercase tracking-[var(--ng-track)] transition-none",
              p.id === activeId
                ? "ng-reverse"
                : "bg-plate text-dim hover:text-text hover:bg-rule",
            )}
          >
            {p.name}
          </button>
        ))}
        {onScan && (
          <button
            type="button"
            onClick={onScan}
            className="ml-auto ng-label text-dim hover:text-text"
          >
            + SCAN LIBRARIES
          </button>
        )}
      </div>

      <div className="mt-2 flex w-full min-w-0 items-baseline gap-3">
        <span className="ng-label shrink-0">BOUND</span>
        {active?.exe ? (
          <>
            <span className="ng-selectable ng-value min-w-0 truncate">{basename(active.exe)}</span>
            <span
              className="ng-selectable hidden min-w-0 flex-1 truncate text-dim sm:block"
              title={active.exe}
            >
              {active.exe}
            </span>
          </>
        ) : (
          <span className="text-dim">
            Nothing — this preset applies when no bound game is running.
          </span>
        )}
      </div>
    </section>
  );
}
