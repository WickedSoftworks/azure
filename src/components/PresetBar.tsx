import { cn } from "cn";
import type { Preset } from "@/lib/model";

interface Props {
  presets: Preset[];
  activeId: string;
  onSelect: (id: string) => void;
  onScan: () => void;
}

function basename(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export function PresetBar({ presets, activeId, onSelect, onScan }: Props) {
  const active = presets.find((p) => p.id === activeId);

  if (presets.length === 0) {
    return (
      <section className="ng-rule-t flex items-center gap-4 bg-field px-4 py-3 shrink-0">
        <span className="ng-label">NO PRESETS</span>
        <p className="text-dim">
          Scan your libraries, or drop a game executable onto this window.
        </p>
        <button
          type="button"
          onClick={onScan}
          className="ml-auto bg-plate px-3 py-1 ng-label text-text hover:bg-rule"
        >
          SCAN LIBRARIES
        </button>
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
        <button
          type="button"
          onClick={onScan}
          className="ml-auto ng-label text-dim hover:text-text"
        >
          + SCAN LIBRARIES
        </button>
      </div>

      <div className="mt-2 flex w-full min-w-0 items-baseline gap-3">
        <span className="ng-label shrink-0">BOUND</span>
        {active?.exe ? (
          <>
            <span className="ng-value min-w-0 truncate">{basename(active.exe)}</span>
            <span
              className="hidden min-w-0 flex-1 truncate text-dim sm:block"
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
