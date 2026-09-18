import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChannelRow } from "@/components/ChannelRow";
import { DisplayRows } from "@/components/DisplayRows";
import { EventLog, type LogEntry } from "@/components/EventLog";
import { FooterBar } from "@/components/FooterBar";
import { Monument } from "@/components/Monument";
import { PresetBar } from "@/components/PresetBar";
import { SignalChain } from "@/components/SignalChain";
import { StatusStrip } from "@/components/StatusStrip";
import { WarningRow } from "@/components/WarningRow";
import { DEMO_PRESETS, DEMO_SESSION } from "@/lib/demo";
import { CHANNELS, type ChannelId, type ColorState, type Stage } from "@/lib/model";

export default function App() {
  const [session, setSession] = useState(DEMO_SESSION);
  const [presets, setPresets] = useState(DEMO_PRESETS);
  const [focused, setFocused] = useState<ChannelId>("vibrance");
  const [bypassed, setBypassed] = useState(false);
  const [pulse, setPulse] = useState(0);
  const [pulseStage, setPulseStage] = useState<Stage | null>(null);
  const [lutTarget, setLutTarget] = useState<string | "all">("all");
  const [log, setLog] = useState<LogEntry[]>([
    { id: 3, time: "21:47:04", kind: "activate", subject: "VALORANT", detail: "focused · matched by full path", latencyUs: 1900 },
    { id: 2, time: "21:47:04", kind: "warn", subject: "GAM", detail: "ramp clamped by GdiIcmGammaRange", latencyUs: 420 },
    { id: 1, time: "21:47:04", kind: "apply", subject: "VIB SAT CON GAM", detail: "matrix + lut · 2 displays", latencyUs: 2300 },
  ]);
  const logId = useRef(4);
  const pulseTimer = useRef<number | undefined>(undefined);

  const active = useMemo(
    () => presets.find((p) => p.id === session.activePresetId) ?? presets[0],
    [presets, session.activePresetId],
  );

  const focusedChannel = useMemo(
    () => CHANNELS.find((c) => c.id === focused)!,
    [focused],
  );

  const setChannel = useCallback(
    (id: ChannelId, v: number) => {
      const stage = CHANNELS.find((c) => c.id === id)!.stage;
      setPresets((prev) =>
        prev.map((p) =>
          p.id === session.activePresetId
            ? { ...p, state: { ...p.state, [id]: v } as ColorState }
            : p,
        ),
      );
      setPulseStage(stage);
      setPulse((n) => n + 1);
      const ch = CHANNELS.find((c) => c.id === id)!;
      setLog((prev) =>
        [
          {
            id: logId.current++,
            time: new Date().toTimeString().slice(0, 8),
            kind: ch.fidelity === "clamped" ? ("warn" as const) : ("apply" as const),
            subject: ch.key,
            detail:
              ch.fidelity === "clamped"
                ? "ramp clamped by GdiIcmGammaRange"
                : `${stage === "matrix" ? "matrix" : "lut"} · ${ch.fidelity}`,
            latencyUs: 600 + Math.round(Math.random() * 1800),
          },
          ...prev,
        ].slice(0, 60),
      );
      window.clearTimeout(pulseTimer.current);
      pulseTimer.current = window.setTimeout(() => setPulseStage(null), 640);
    },
    [session.activePresetId],
  );

  // Hold to see the original. The effect covers this window too, so an
  // in-app "after" preview would be double-transformed and therefore a
  // lie; the honest comparison is against the real screen.
  useEffect(() => {
    const interactive = (t: EventTarget | null) =>
      t instanceof HTMLElement && (t.tagName === "BUTTON" || t.tagName === "INPUT");

    const down = (e: KeyboardEvent) => {
      if (e.code !== "Space" || e.repeat || interactive(e.target)) return;
      e.preventDefault();
      setBypassed(true);
    };
    const up = (e: KeyboardEvent) => {
      if (e.code === "Space") setBypassed(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", () => setBypassed(false));
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, []);

  useEffect(() => () => window.clearTimeout(pulseTimer.current), []);

  const gammaClamped = !session.gammaRangeUnlocked;

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-void">
      <StatusStrip
        session={session}
        preset={active}
        onToggle={() => setSession((s) => ({ ...s, enabled: !s.enabled }))}
      />

      {gammaClamped && (
        <WarningRow
          tone="warn"
          code="GAMMA CLAMPED"
          message="Windows limits gamma ramps until GdiIcmGammaRange is set to 256. Azure works without it at reduced range."
          actionLabel="UNLOCK FULL RANGE"
          onAction={() =>
            setSession((s) => ({ ...s, gammaRangeUnlocked: true }))
          }
        />
      )}

      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:grid lg:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] lg:overflow-hidden">
        <div className="flex min-w-0 flex-col lg:min-h-0 lg:overflow-hidden lg:border-r lg:border-r-rule">
          <div className="ng-rule-b grid grid-cols-[3ch_1fr_6ch] sm:grid-cols-[3ch_1fr_6ch_7ch_7ch] items-center gap-x-3 px-4 py-1.5">
            <span className="ng-label">CH</span>
            <span className="ng-label">LEVEL</span>
            <span className="ng-label text-right">VALUE</span>
            <span className="ng-label hidden sm:block">STAGE</span>
            <span className="ng-label hidden sm:block">FIDELITY</span>
          </div>

          {CHANNELS.map((c) => (
            <ChannelRow
              key={c.id}
              channel={c}
              value={active.state[c.id]}
              bypassed={bypassed}
              onChange={(v) => setChannel(c.id, v)}
              onFocus={() => setFocused(c.id)}
            />
          ))}

          <DisplayRows
            displays={session.displays}
            lutTarget={lutTarget}
            onTarget={setLutTarget}
          />

          <EventLog entries={log} />

          <div className="ng-rule-t flex flex-wrap gap-x-5 gap-y-1 px-4 py-2">
            <button
              type="button"
              onClick={() =>
                setPresets((prev) =>
                  prev.map((p) =>
                    p.id === active.id
                      ? { ...p, state: { ...CHANNELS.reduce((a, c) => ({ ...a, [c.id]: c.neutral }), {}) } as ColorState }
                      : p,
                  ),
                )
              }
              className="ng-label text-dim hover:text-text"
            >
              RESET TO NEUTRAL
            </button>
          </div>
        </div>

        <aside className="ng-rule-t flex min-w-0 flex-col bg-field lg:border-t-0 lg:min-h-0 lg:overflow-y-auto">
          <Monument
            channel={focusedChannel}
            value={active.state[focused]}
            bypassed={bypassed}
          />
          <div className="ng-rule-t">
            <SignalChain
              state={active.state}
              pulse={pulse}
              pulseStage={pulseStage}
              exclusiveFullscreen={session.exclusiveFullscreen}
            />
          </div>
        </aside>
      </main>

      <PresetBar
        presets={presets}
        activeId={active.id}
        onSelect={(id) => setSession((s) => ({ ...s, activePresetId: id }))}
        onScan={() => {}}
      />

      <FooterBar bypassed={bypassed} />
    </div>
  );
}
