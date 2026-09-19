import { cn } from "cn";
import type { BindingSet } from "@/lib/bindings";

interface Props {
  bypassed: boolean;
  bindings: BindingSet;
}

/**
 * Hold-to-bypass does not sit at reminder rank. Azure ships no in-app
 * preview on purpose — the compositor applies the effect to this window
 * too, so an "after" swatch would be double-transformed and therefore a
 * lie — which makes this the only way to make the comparison the user
 * opened the window to make. It takes the reversal lever; everything
 * beside it stays dim.
 */
export function FooterBar({ bypassed, bindings }: Props) {
  // Only bound chords are advertised. M1 deleted this strip rather than
  // keep naming three keys that did nothing, and a reminder for an
  // unbound action would be the same lie in a smaller font.
  const reminders = [
    { keys: bindings.toggleEnabled, action: "TOGGLE" },
    { keys: cycleKeys(bindings), action: "CYCLE PRESET" },
    { keys: bindings.restoreDisplay, action: "RESTORE DISPLAY" },
  ].filter((r): r is { keys: string; action: string } => r.keys !== null);

  return (
    <footer className="ng-rule-t flex flex-wrap items-center gap-x-5 gap-y-2 bg-field px-4 py-2 shrink-0">
      <span
        className={cn(
          "flex items-baseline gap-2 px-2 py-0.5 uppercase tracking-[var(--ng-track)]",
          bypassed
            ? "ng-reverse"
            : "border border-signal/60 bg-plate text-bright",
        )}
      >
        <span className={cn("font-bold", !bypassed && "text-signal")}>
          {bypassed ? "SHOWING ORIGINAL" : holdLabel(bindings)}
        </span>
        <span className={cn(bypassed ? "text-void/70" : "text-dim")}>
          {bypassed ? "RELEASE TO RETURN" : "TO SEE THE ORIGINAL SCREEN"}
        </span>
      </span>

      <span className="ml-auto flex flex-wrap items-baseline gap-x-4 gap-y-1">
        {reminders.map((r) => (
          <span key={r.action} className="flex items-baseline gap-2">
            <span className="ng-label text-text">{r.keys}</span>
            <span className="text-dim">{r.action}</span>
          </span>
        ))}
      </span>
    </footer>
  );
}

/**
 * The two cycle bindings read as one reminder when they are a mirrored
 * pair, which is what the defaults are. Bound to unrelated keys, they are
 * not a pair and the shared half would be a fiction, so only the forward
 * one is shown.
 */
function cycleKeys(bindings: BindingSet): string | null {
  const { cycleNext, cyclePrev } = bindings;
  if (!cycleNext) return cyclePrev;
  if (!cyclePrev) return cycleNext;

  const [nextHead, nextKey] = split(cycleNext);
  const [prevHead, prevKey] = split(cyclePrev);
  if (nextHead === prevHead) return `${nextHead}${prevKey}/${nextKey}`;
  return cycleNext;
}

/** `ALT+SHIFT+RIGHT` becomes `["ALT+SHIFT+", "RIGHT"]`. */
function split(chord: string): [string, string] {
  const at = chord.lastIndexOf("+");
  return at < 0 ? ["", chord] : [chord.slice(0, at + 1), chord.slice(at + 1)];
}

/**
 * Space always works while this window has focus — that is a local key
 * handler, not a registration. A bound global chord is named instead
 * because it is the one that also works with the game in front.
 */
function holdLabel(bindings: BindingSet): string {
  return bindings.holdBypass ? `HOLD ${bindings.holdBypass}` : "HOLD SPACE";
}
