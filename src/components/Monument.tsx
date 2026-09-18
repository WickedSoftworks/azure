import { cn } from "cn";
import { FIDELITY_LABEL, formatValue, isInert, type Channel } from "@/lib/model";

interface Props {
  channel: Channel;
  value: number;
  bypassed: boolean;
}

/**
 * The single exception to the one-size rule. The number you are currently
 * moving is the reason the window is open, so it is structural rather than
 * a label beside a slider.
 */
export function Monument({ channel, value, bypassed }: Props) {
  const inert = isInert(channel.fidelity);
  const offNeutral = value !== channel.neutral;

  return (
    <div className="flex flex-col justify-center px-6 py-5">
      <div
        className={cn(
          "font-extrabold leading-[0.85] tracking-[-0.04em] tabular-nums",
          bypassed ? "text-dim" : offNeutral && !inert ? "text-signal" : "text-bright",
        )}
        style={{ fontSize: "var(--ng-monument)" }}
      >
        {formatValue(channel, value)}
      </div>

      <div className="mt-4 flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="ng-label text-text">{channel.name}</span>
        <span className="ng-label">
          {channel.stage === "matrix" ? "VIA MATRIX" : "VIA LUT"}
        </span>
        <span
          className={cn(
            "ng-label",
            channel.fidelity === "exact" && "text-ok",
            (channel.fidelity === "approximate" || channel.fidelity === "clamped") &&
              "text-warn",
            inert && "text-alert",
          )}
        >
          {FIDELITY_LABEL[channel.fidelity]}
        </span>
      </div>

      {channel.note && (
        <p className="mt-2 max-w-[46ch] text-dim">{channel.note}</p>
      )}
    </div>
  );
}
