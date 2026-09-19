import { useCallback, useEffect, useState } from "react";
import { cn } from "cn";
import type { Action, Conflict, Registration } from "@/lib/bindings";
import type { BindingSet } from "@/lib/bindings";

interface Props {
  bindings: BindingSet;
  registrations: Registration[];
  conflicts: Conflict[];
  autostart: boolean;
  live: boolean;
  onBind: (action: Action, chord: string | null) => void;
  onReset: () => void;
  onAutostart: (enabled: boolean) => void;
}

/** The order the actions are listed in, and what each is called. */
const ROWS: { action: Action; label: string; note?: string }[] = [
  { action: "toggleEnabled", label: "TOGGLE AZURE" },
  { action: "cycleNext", label: "NEXT PRESET" },
  { action: "cyclePrev", label: "PREVIOUS PRESET" },
  { action: "restoreDisplay", label: "RESTORE DISPLAY" },
  {
    action: "holdBypass",
    label: "HOLD TO BYPASS",
    note: "held, not pressed — unbound until you choose a key your hand is already near",
  },
];

const FIELD: Record<Action, keyof BindingSet> = {
  toggleEnabled: "toggleEnabled",
  cycleNext: "cycleNext",
  cyclePrev: "cyclePrev",
  restoreDisplay: "restoreDisplay",
  holdBypass: "holdBypass",
};

/**
 * Turns a key event into the text the core parses.
 *
 * Returns null while only modifiers are held: a chord is not finished
 * until a real key joins them, and showing `CTRL+` as though it were a
 * binding would be showing something that cannot be registered.
 */
function chordFrom(e: KeyboardEvent): string | null {
  const key = keyName(e.code);
  if (!key) return null;

  const parts: string[] = [];
  if (e.ctrlKey) parts.push("CTRL");
  if (e.altKey) parts.push("ALT");
  if (e.shiftKey) parts.push("SHIFT");
  if (e.metaKey) parts.push("WIN");
  if (parts.length === 0) return null;

  parts.push(key);
  return parts.join("+");
}

function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;

  const named: Record<string, string> = {
    ArrowLeft: "LEFT",
    ArrowRight: "RIGHT",
    ArrowUp: "UP",
    ArrowDown: "DOWN",
    Space: "SPACE",
    Tab: "TAB",
    Enter: "ENTER",
    Backspace: "BACKSPACE",
    Insert: "INSERT",
    Delete: "DELETE",
    Home: "HOME",
    End: "END",
    PageUp: "PAGEUP",
    PageDown: "PAGEDOWN",
    Pause: "PAUSE",
  };
  return named[code] ?? null;
}

/**
 * Bindings, and what Windows said about them.
 *
 * A refusal is shown against the chord that earned it rather than as a
 * general failure: four of five may have registered perfectly, and a
 * player who has bound the same key in Discord needs to know which one to
 * change.
 */
export function HotkeyPanel({
  bindings,
  registrations,
  conflicts,
  autostart,
  live,
  onBind,
  onReset,
  onAutostart,
}: Props) {
  const [capturing, setCapturing] = useState<Action | null>(null);

  const stop = useCallback(() => setCapturing(null), []);

  useEffect(() => {
    if (!capturing) return;

    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        stop();
        return;
      }
      const chord = chordFrom(e);
      if (!chord) return;
      onBind(capturing, chord);
      stop();
    };

    window.addEventListener("keydown", down, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("blur", stop);
    };
  }, [capturing, onBind, stop]);

  return (
    <section className="ng-rule-t flex flex-col">
      <div className="ng-rule-b flex items-baseline gap-x-3 px-4 py-1.5">
        <span className="ng-label">HOTKEYS</span>
        <span className="text-dim">
          work with the window closed · Azure stays in the notification area
        </span>
        <button
          type="button"
          onClick={onReset}
          disabled={!live}
          className="ml-auto ng-label text-dim hover:text-text disabled:opacity-40"
        >
          RESET TO DEFAULTS
        </button>
      </div>

      {ROWS.map(({ action, label, note }) => {
        const chord = bindings[FIELD[action]];
        const registration = registrations.find((r) => r.action === action);
        const refused =
          registration && registration.outcome.kind === "refused"
            ? registration.outcome.reason
            : null;
        const clashes = conflicts.some((c) => c.actions.includes(action));

        return (
          <div
            key={action}
            className="ng-rule-b grid grid-cols-[1fr_auto] items-baseline gap-x-3 gap-y-1 px-4 py-2"
          >
            <span className="ng-label text-text">{label}</span>

            <div className="flex items-baseline gap-2">
              <button
                type="button"
                onClick={() => setCapturing(capturing === action ? null : action)}
                disabled={!live}
                className={cn(
                  "px-2 py-0.5 uppercase tracking-[var(--ng-track)] border disabled:opacity-40",
                  capturing === action
                    ? "ng-reverse border-signal"
                    : refused || clashes
                      ? "border-warn/60 bg-plate text-warn"
                      : chord
                        ? "border-signal/60 bg-plate text-bright"
                        : "border-rule bg-plate text-dim",
                )}
              >
                {capturing === action ? "PRESS A COMBINATION" : (chord ?? "UNBOUND")}
              </button>
              {chord && (
                <button
                  type="button"
                  onClick={() => onBind(action, null)}
                  disabled={!live}
                  className="ng-label text-dim hover:text-text disabled:opacity-40"
                >
                  CLEAR
                </button>
              )}
            </div>

            {(note || refused || clashes) && (
              <p className="col-span-2 ng-selectable text-dim">
                {clashes && (
                  <span className="text-warn">
                    Bound to more than one action here.{" "}
                  </span>
                )}
                {refused && <span className="text-warn">Windows refused it: {refused}. </span>}
                {note}
              </p>
            )}
          </div>
        );
      })}

      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-4 py-2">
        <button
          type="button"
          onClick={() => onAutostart(!autostart)}
          disabled={!live}
          className={cn(
            "ng-label disabled:opacity-40",
            autostart ? "text-signal hover:text-bright" : "text-dim hover:text-text",
          )}
        >
          {autostart ? "◼ STARTS WITH WINDOWS" : "◻ START WITH WINDOWS"}
        </button>
        <span className="text-dim">
          launches hidden, so the preset is already right when the game opens
        </span>
      </div>
    </section>
  );
}
