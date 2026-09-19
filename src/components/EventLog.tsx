import { cn } from "cn";

export interface LogEntry {
  id: number;
  time: string;
  kind: "apply" | "activate" | "warn" | "restore";
  subject: string;
  detail: string;
  latencyUs?: number;
}

interface Props {
  entries: LogEntry[];
}

const KIND_LABEL: Record<LogEntry["kind"], string> = {
  apply: "APPLY",
  activate: "ACTIVE",
  warn: "WARN",
  restore: "RESTORE",
};

/**
 * What the core actually did, in order. This is the product's second
 * principle made continuous: every apply reports the stages it reached and
 * how long it took, so a clamped ramp or an inert matrix is visible as it
 * happens rather than discovered later.
 */
export function EventLog({ entries }: Props) {
  return (
    <section className="ng-rule-t flex shrink-0 flex-col">
      <div className="ng-rule-b grid grid-cols-[8ch_7ch_1fr_7ch] items-center gap-x-3 px-4 py-1.5">
        <span className="ng-label">TIME</span>
        <span className="ng-label">EVENT</span>
        <span className="ng-label">DETAIL</span>
        <span className="ng-label text-right">TOOK</span>
      </div>

      <div className="max-h-[18rem] overflow-y-auto">
        {entries.length === 0 ? (
          <p className="px-4 py-3 text-dim">
            Nothing applied yet. Move a channel and the core reports what it did.
          </p>
        ) : (
          entries.map((e) => (
            <div
              key={e.id}
              className="grid grid-cols-[8ch_7ch_1fr_7ch] items-baseline gap-x-3 px-4 py-1"
            >
              <span className="text-dim tabular-nums">{e.time}</span>
              <span
                className={cn(
                  "ng-label",
                  e.kind === "warn" && "text-warn",
                  e.kind === "restore" && "text-alert",
                  e.kind === "activate" && "text-signal",
                )}
              >
                {KIND_LABEL[e.kind]}
              </span>
              <span className="min-w-0 truncate">
                <span className="ng-value">{e.subject}</span>{" "}
                <span className="text-dim">{e.detail}</span>
              </span>
              <span className="text-right text-dim tabular-nums">
                {e.latencyUs !== undefined ? `${(e.latencyUs / 1000).toFixed(1)}ms` : "—"}
              </span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
