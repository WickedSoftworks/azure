import { cn } from "cn";
import type { Candidate, Confidence, Launcher, ScanReport } from "@/lib/bindings";

interface Props {
  report: ScanReport;
  /** Executables already bound to a preset, lowercased. */
  bound: Set<string>;
  busy: boolean;
  onAdd: (candidate: Candidate) => void;
  onRescan: () => void;
  onDismiss: () => void;
}

const LAUNCHER_LABEL: Record<Launcher, string> = {
  steam: "STEAM",
  epic: "EPIC",
  gog: "GOG",
  ea: "EA",
  ubisoft: "UBISOFT",
  battleNet: "BATTLE.NET",
  xbox: "XBOX",
};

/**
 * What a confidence means, in the words a person would use. `Exact` earns
 * no note: the launcher named the file, and saying so would be noise on
 * every row that is simply correct.
 */
const CONFIDENCE_NOTE: Record<Confidence, string | null> = {
  exact: null,
  likely: null,
  guess: "best guess — check it, or capture the window instead",
};

/**
 * The results of a library scan.
 *
 * It creates nothing by itself. A scanner that made two hundred presets
 * would be a scanner that made two hundred things to delete, and the
 * product is a desktop preset plus the handful of games you actually
 * tune.
 */
export function ScanPanel({ report, bound, busy, onAdd, onRescan, onDismiss }: Props) {
  const unbound = report.candidates.filter((c) => !bound.has(c.exe.toLowerCase()));
  const already = report.candidates.length - unbound.length;

  return (
    <section className="ng-rule-t flex flex-col">
      <div className="ng-rule-b flex flex-wrap items-baseline gap-x-3 gap-y-1 px-4 py-1.5">
        <span className="ng-label">LIBRARY SCAN</span>
        <span className="text-dim">
          {busy
            ? "reading every launcher…"
            : `${unbound.length} to add${already > 0 ? ` · ${already} already bound` : ""}`}
        </span>
        <button
          type="button"
          onClick={onRescan}
          disabled={busy}
          className="ml-auto ng-label text-dim hover:text-text disabled:opacity-40"
        >
          SCAN AGAIN
        </button>
        <button
          type="button"
          onClick={onDismiss}
          className="ng-label text-dim hover:text-text"
        >
          CLOSE
        </button>
      </div>

      {/* Every launcher answers, including the ones that are not
          installed. Five quiet absences must not read as five failures. */}
      <div className="ng-rule-b flex flex-wrap gap-x-4 gap-y-1 px-4 py-1.5">
        {report.sources.map((source) => (
          <span key={source.launcher} className="flex items-baseline gap-1.5">
            <span className="ng-label text-dim">{LAUNCHER_LABEL[source.launcher]}</span>
            <span
              className={cn(
                source.outcome.kind === "scanned" && source.outcome.found > 0
                  ? "text-text"
                  : source.outcome.kind === "unreadable"
                    ? "text-warn"
                    : "text-dim",
              )}
              title={source.outcome.kind === "unreadable" ? source.outcome.reason : undefined}
            >
              {source.outcome.kind === "scanned"
                ? source.outcome.found
                : source.outcome.kind === "notInstalled"
                  ? "—"
                  : "can't read"}
            </span>
          </span>
        ))}
      </div>

      {report.sources
        .filter((s) => s.outcome.kind === "unreadable")
        .map((s) => (
          <p key={s.launcher} className="ng-rule-b ng-selectable px-4 py-1.5 text-dim">
            <span className="ng-label text-warn">{LAUNCHER_LABEL[s.launcher]}</span>{" "}
            {s.outcome.kind === "unreadable" && s.outcome.reason}
          </p>
        ))}

      <div className="max-h-72 overflow-y-auto">
        {unbound.length === 0 && !busy && (
          <p className="px-4 py-2 text-dim">
            Nothing new. Games already bound to a preset are not listed again.
          </p>
        )}

        {unbound.map((candidate) => (
          <div
            key={candidate.exe}
            className="ng-rule-b grid grid-cols-[1fr_auto] items-baseline gap-x-3 px-4 py-1.5"
          >
            <div className="min-w-0">
              <span className="text-text">{candidate.name}</span>{" "}
              <span className="ng-label text-dim">{LAUNCHER_LABEL[candidate.launcher]}</span>
              {CONFIDENCE_NOTE[candidate.confidence] && (
                <span className="text-warn"> · {CONFIDENCE_NOTE[candidate.confidence]}</span>
              )}
              <p className="ng-selectable truncate text-dim" title={candidate.exe}>
                {candidate.exe}
              </p>
            </div>
            <button
              type="button"
              onClick={() => onAdd(candidate)}
              className="ng-label text-signal hover:text-bright"
            >
              + ADD
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
