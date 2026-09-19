import { useEffect, useRef, useState } from "react";
import { cn } from "cn";
import type { MatchKind, Preset } from "@/lib/model";

interface Props {
  presets: Preset[];
  activeId: string;
  /** How the watcher reached the active preset, if it did. */
  matchedBy: MatchKind | null;
  /** True while waiting for the next window to take focus. */
  capturing: boolean;
  /** False in a browser session, where none of this can do anything. */
  live: boolean;
  onSelect: (id: string) => void;
  onBrowse: () => void;
  onCapture: () => void;
  onScan: () => void;
  scanning: boolean;
  onCancelCapture: () => void;
  onRename: (id: string, name: string) => void;
  onUnbind: (id: string) => void;
  onDelete: (id: string) => void;
}

function basename(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

const MATCH_LABEL: Record<MatchKind, string> = {
  fullPath: "matched by full path",
  exeName: "matched by exe name",
};

/**
 * The preset row, and the only place a preset is created, named or
 * unbound. Renaming and deleting happen in place rather than in a dialog:
 * the field is a table, and a modal over a table is a worse table.
 */
export function PresetBar({
  presets,
  activeId,
  matchedBy,
  capturing,
  live,
  onSelect,
  onBrowse,
  onCapture,
  onScan,
  scanning,
  onCancelCapture,
  onRename,
  onUnbind,
  onDelete,
}: Props) {
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [confirming, setConfirming] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const active = presets.find((p) => p.id === activeId);
  const isDesktop = active?.exe === null;

  useEffect(() => {
    if (renaming) input.current?.select();
  }, [renaming]);

  // A half-finished rename or an armed delete should not follow you to
  // another preset.
  useEffect(() => {
    setRenaming(null);
    setConfirming(null);
  }, [activeId]);

  const commitRename = () => {
    const name = draft.trim();
    if (renaming && name) onRename(renaming, name);
    setRenaming(null);
  };

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

        <div className="ml-auto flex items-center gap-4">
          {capturing ? (
            <>
              <span className="ng-label text-signal">
                FOCUS THE GAME — ITS NEXT WINDOW IS CAPTURED
              </span>
              <button
                type="button"
                onClick={onCancelCapture}
                className="ng-label text-dim hover:text-text"
              >
                CANCEL
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                onClick={onCapture}
                disabled={!live}
                className="ng-label text-dim hover:text-text disabled:hover:text-dim"
              >
                + CAPTURE WINDOW
              </button>
              <button
                type="button"
                onClick={onBrowse}
                disabled={!live}
                className="ng-label text-dim hover:text-text disabled:hover:text-dim"
              >
                + ADD GAME
              </button>
              {/* Back, and now with a scanner behind it. M1 removed this
                  button rather than keep offering something that did
                  nothing. */}
              <button
                type="button"
                onClick={onScan}
                disabled={!live || scanning}
                className="ng-label text-dim hover:text-text disabled:hover:text-dim disabled:opacity-40"
              >
                {scanning ? "SCANNING…" : "SCAN LIBRARIES"}
              </button>
            </>
          )}
        </div>
      </div>

      <div className="mt-2 flex w-full min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="ng-label shrink-0">BOUND</span>

        {active?.exe ? (
          <>
            <span className="ng-selectable ng-value min-w-0 truncate">
              {basename(active.exe)}
            </span>
            <span
              className="ng-selectable hidden min-w-0 flex-1 truncate text-dim sm:block"
              title={active.exe}
            >
              {active.exe}
            </span>
            {matchedBy && (
              <span className="ng-label shrink-0 text-ok">{MATCH_LABEL[matchedBy]}</span>
            )}
          </>
        ) : (
          <span className="flex-1 text-dim">
            {isDesktop
              ? "Nothing — this is what applies when no bound game is in front."
              : "Nothing yet. Add a game to bind this preset to it."}
          </span>
        )}

        {active && !isDesktop && (
          <div className="flex shrink-0 items-baseline gap-3">
            {renaming === active.id ? (
              <>
                <input
                  ref={input}
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename();
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  className="ng-value w-[18ch] border-b border-signal bg-transparent px-1 uppercase outline-none"
                  aria-label="Preset name"
                />
                <button type="button" onClick={commitRename} className="ng-label text-signal">
                  SAVE
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setDraft(active.name);
                  setRenaming(active.id);
                }}
                className="ng-label text-dim hover:text-text"
              >
                RENAME
              </button>
            )}

            {active.exe && (
              <button
                type="button"
                onClick={() => onUnbind(active.id)}
                className="ng-label text-dim hover:text-text"
              >
                UNBIND
              </button>
            )}

            {confirming === active.id ? (
              <>
                <span className="ng-label text-alert">DELETE?</span>
                <button
                  type="button"
                  onClick={() => {
                    onDelete(active.id);
                    setConfirming(null);
                  }}
                  className="ng-label text-alert hover:text-bright"
                >
                  YES
                </button>
                <button
                  type="button"
                  onClick={() => setConfirming(null)}
                  className="ng-label text-dim hover:text-text"
                >
                  NO
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={() => setConfirming(active.id)}
                className="ng-label text-dim hover:text-alert"
              >
                DELETE
              </button>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
