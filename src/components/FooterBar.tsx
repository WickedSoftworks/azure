import { cn } from "cn";

interface Props {
  bypassed: boolean;
}

const BINDINGS: { keys: string; action: string }[] = [
  { keys: "ALT+SHIFT+V", action: "TOGGLE" },
  { keys: "ALT+SHIFT+←/→", action: "CYCLE PRESET" },
  { keys: "CTRL+ALT+SHIFT+R", action: "RESTORE DISPLAY" },
];

export function FooterBar({ bypassed }: Props) {
  return (
    <footer className="ng-rule-t flex flex-wrap items-center gap-x-5 gap-y-1 bg-field px-4 py-2 shrink-0">
      <span
        className={cn(
          "px-2 py-0.5 uppercase tracking-[var(--ng-track)]",
          bypassed ? "ng-reverse" : "bg-plate text-dim",
        )}
      >
        {bypassed ? "SHOWING ORIGINAL" : "HOLD SPACE"}
      </span>
      <span className="ng-label hidden sm:inline">
        {bypassed ? "RELEASE TO RETURN" : "TO COMPARE"}
      </span>

      <div className="ml-auto flex flex-wrap items-center gap-x-5 gap-y-1">
        {BINDINGS.map((b) => (
          <span key={b.keys} className="flex items-baseline gap-2">
            <span className="ng-value">{b.keys}</span>
            <span className="ng-label">{b.action}</span>
          </span>
        ))}
      </div>
    </footer>
  );
}
