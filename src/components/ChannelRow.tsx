import { Slider as SliderPrimitive } from "radix-ui";
import { cn } from "cn";
import {
  FIDELITY_LABEL,
  formatValue,
  isInert,
  type Channel,
} from "@/lib/model";

const SEGMENTS = 40;

interface Props {
  channel: Channel;
  value: number;
  bypassed: boolean;
  onChange: (v: number) => void;
  onFocus: () => void;
}

/**
 * One telemetry line. Fixed columns, one type size, rank carried by weight
 * and case. The track is a segment meter rather than a pill because this
 * row is a readout first and a control second.
 */
export function ChannelRow({ channel, value, bypassed, onChange, onFocus }: Props) {
  const span = channel.max - channel.min;
  const ratio = (value - channel.min) / span;
  const neutralRatio = (channel.neutral - channel.min) / span;
  const inert = isInert(channel.fidelity) || bypassed;
  const offNeutral = value !== channel.neutral;

  const filledFrom = Math.min(ratio, neutralRatio);
  const filledTo = Math.max(ratio, neutralRatio);

  return (
    <div
      className={cn(
        "group grid items-center gap-x-3 px-4 h-[var(--ng-cell)]",
        "grid-cols-[3ch_1fr_6ch] sm:grid-cols-[3ch_1fr_6ch_7ch_7ch]",
        "hover:bg-plate/60 focus-within:bg-plate/60",
      )}
      onPointerEnter={onFocus}
    >
      <span
        className={cn(
          "ng-label tracking-[var(--ng-track)]",
          offNeutral && !inert && "text-signal",
        )}
      >
        {channel.key}
      </span>

      <SliderPrimitive.Root
        className="relative flex h-[var(--ng-cell)] w-full touch-none select-none items-center"
        min={channel.min}
        max={channel.max}
        step={channel.step}
        value={[value]}
        aria-label={channel.name}
        onValueChange={([v]) => onChange(v)}
        onFocus={onFocus}
      >
        <SliderPrimitive.Track
          className={cn(
            "relative flex h-[11px] w-full items-stretch gap-px overflow-hidden",
            inert && "ng-hatch opacity-45",
          )}
        >
          {Array.from({ length: SEGMENTS }, (_, i) => {
            const at = (i + 0.5) / SEGMENTS;
            const lit = at >= filledFrom && at <= filledTo;
            const isNeutralMark = Math.abs(at - neutralRatio) < 0.5 / SEGMENTS;
            return (
              <span
                key={i}
                className={cn(
                  "flex-1",
                  lit
                    ? inert
                      ? "bg-dim"
                      : "bg-signal"
                    : isNeutralMark
                      ? "bg-rule-bright"
                      : "bg-rule",
                )}
              />
            );
          })}
        </SliderPrimitive.Track>

        <SliderPrimitive.Thumb
          className={cn(
            "block h-[19px] w-[3px] bg-bright outline-none",
            "focus-visible:outline focus-visible:outline-1 focus-visible:outline-signal focus-visible:outline-offset-2",
            inert && "bg-dim",
          )}
        />
      </SliderPrimitive.Root>

      <span
        className={cn(
          "ng-value text-right",
          inert && "text-dim",
          offNeutral && !inert && "text-signal",
        )}
      >
        {formatValue(channel, value)}
      </span>

      <span className="ng-label hidden sm:block">
        {channel.stage === "matrix" ? "MATRIX" : "LUT"}
      </span>

      <span
        className={cn(
          "ng-label hidden sm:block",
          channel.fidelity === "exact" && "text-ok",
          channel.fidelity === "approximate" && "text-warn",
          channel.fidelity === "clamped" && "text-warn",
          isInert(channel.fidelity) && "text-alert",
        )}
        title={channel.note}
      >
        {bypassed && !isInert(channel.fidelity)
          ? "BYPASS"
          : FIDELITY_LABEL[channel.fidelity]}
      </span>
    </div>
  );
}
