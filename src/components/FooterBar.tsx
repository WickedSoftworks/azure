import { cn } from "cn";

interface Props {
  bypassed: boolean;
}

const BINDINGS: { keys: string; action: string }[] = [
  { keys: "ALT+SHIFT+V", action: "TOGGLE" },
  { keys: "ALT+SHIFT+←/→", action: "CYCLE PRESET" },
  { keys: "CTRL+ALT+SHIFT+R", action: "RESTORE DISPLAY" },
];

/**
 * Hold-to-bypass does not sit at reminder rank. Azure ships no in-app
 * preview on purpose — the compositor applies the effect to this window
 * too, so an "after" swatch would be double-transformed and therefore a
 * lie — which makes this the only way to make the comparison the user
 * opened the window to make. It takes the reversal lever; the three
 * reminders beside it stay dim.
 */
export function FooterBar({ bypassed }: Props) {
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
          {bypassed ? "SHOWING ORIGINAL" : "HOLD SPACE"}
        </span>
        <span className={cn(bypassed ? "text-void/70" : "text-dim")}>
          {bypassed ? "RELEASE TO RETURN" : "TO SEE THE ORIGINAL SCREEN"}
        </span>
      </span>

      <div className="ml-auto flex flex-wrap items-center gap-x-5 gap-y-1">
        {BINDINGS.map((b) => (
          <span key={b.keys} className="flex items-baseline gap-2">
            <span className="text-dim">{b.keys}</span>
            <span className="ng-label">{b.action}</span>
          </span>
        ))}
      </div>
    </footer>
  );
}
