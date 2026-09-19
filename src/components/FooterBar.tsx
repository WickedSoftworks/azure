import { cn } from "cn";

interface Props {
  bypassed: boolean;
}

/**
 * Hold-to-bypass does not sit at reminder rank. Azure ships no in-app
 * preview on purpose — the compositor applies the effect to this window
 * too, so an "after" swatch would be double-transformed and therefore a
 * lie — which makes this the only way to make the comparison the user
 * opened the window to make. It takes the reversal lever; everything
 * beside it stays dim.
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

      {/* The three global hotkeys this bar used to advertise are M7. A
          reminder for a binding that does not exist is the same kind of
          lie as a fidelity badge for a stage that never landed. */}
      <span className="ml-auto text-dim">
        Global hotkeys arrive with tray residency.
      </span>
    </footer>
  );
}
