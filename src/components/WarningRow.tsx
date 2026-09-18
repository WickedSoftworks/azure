import { cn } from "cn";

export type WarningTone = "warn" | "alert";

interface Props {
  tone: WarningTone;
  code: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

/**
 * A warning is a row in the field, not a floating card. It states the
 * machine fact and names the recovery; it never says "something went
 * wrong".
 */
export function WarningRow({ tone, code, message, actionLabel, onAction }: Props) {
  return (
    <div className="ng-rule-b flex flex-wrap items-baseline gap-x-3 gap-y-1 bg-plate px-4 py-2">
      <span
        className={cn(
          "ng-label font-bold",
          tone === "warn" ? "text-warn" : "text-alert",
        )}
      >
        {code}
      </span>
      <p className="text-text">{message}</p>
      {actionLabel && (
        <button
          type="button"
          onClick={onAction}
          className="ml-auto ng-label text-signal underline underline-offset-4 hover:text-bright"
        >
          {actionLabel}
        </button>
      )}
    </div>
  );
}
