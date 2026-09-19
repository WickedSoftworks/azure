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
import {
  applyState,
  isTauri,
  loadSnapshot,
  restoreDisplay,
  setEnabled as setEnabledIpc,
  setLutTarget,
  unlockGammaRange,
} from "@/lib/ipc";
import type {
  ApplyReport,
  ChannelId,
  ChannelReport,
  ColorState,
  LutTarget,
  Snapshot,
  Stage,
} from "@/lib/model";

/** How long after the last drag event a change is considered settled. */
const SETTLE_MS = 350;

const LIVE = isTauri();

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [channels, setChannels] = useState<ChannelReport[]>([]);
  const [state, setState] = useState<ColorState | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [target, setTarget] = useState<LutTarget>("all");
  const [focused, setFocused] = useState<ChannelId>("vibrance");
  const [bypassed, setBypassed] = useState(false);
  const [pulse, setPulse] = useState(0);
  const [pulseStage, setPulseStage] = useState<Stage | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);

  const logId = useRef(1);
  const pulseTimer = useRef<number | undefined>(undefined);
  const settleTimer = useRef<number | undefined>(undefined);
  const inFlight = useRef(false);
  const queued = useRef<{ state: ColorState; target: LutTarget } | null>(null);
  const lastReport = useRef<ApplyReport | null>(null);
  // The bypass listeners are bound once, so they read the live values
  // through refs rather than closing over the first render's.
  const bypassedRef = useRef(false);
  const stateRef = useRef<ColorState | null>(null);
  const targetRef = useRef<LutTarget>("all");

  const write = useCallback((entry: Omit<LogEntry, "id" | "time">) => {
    setLog((prev) =>
      [
        { id: logId.current++, time: new Date().toTimeString().slice(0, 8), ...entry },
        ...prev,
      ].slice(0, 60),
    );
  }, []);

  useEffect(() => {
    loadSnapshot().then((snap) => {
      setSnapshot(snap);
      setChannels(snap.channels);
      setState(snap.state);
      setEnabled(snap.enabled);
      setTarget(snap.target);
      for (const notice of snap.notices) {
        write({ kind: "warn", subject: "CORE", detail: notice });
      }
    });
  }, [write]);

  /**
   * Sends the newest state and drops anything stale behind it.
   *
   * A slider drag fires far faster than a write to two backends across
   * however many displays completes, and the engine is one thread. Queuing
   * every tick would make the screen lag the thumb by whatever the queue
   * had built up; the only value worth writing is the latest one.
   */
  const push = useCallback((next: ColorState, nextTarget: LutTarget) => {
    if (inFlight.current) {
      queued.current = { state: next, target: nextTarget };
      return;
    }
    inFlight.current = true;
    applyState(next, nextTarget)
      .then((report) => {
        setChannels(report.reports);
        lastReport.current = report;
      })
      .catch((e: unknown) => {
        write({ kind: "warn", subject: "CORE", detail: String(e) });
      })
      .finally(() => {
        inFlight.current = false;
        const pending = queued.current;
        queued.current = null;
        if (pending) push(pending.state, pending.target);
      });
  }, [write]);

  const setChannel = useCallback(
    (id: ChannelId, value: number) => {
      if (!state) return;
      const next = { ...state, [id]: value };
      setState(next);

      const channel = channels.find((c) => c.id === id);
      if (channel?.stage) {
        setPulseStage(channel.stage);
        setPulse((n) => n + 1);
        window.clearTimeout(pulseTimer.current);
        pulseTimer.current = window.setTimeout(() => setPulseStage(null), 640);
      }

      push(next, target);

      // One log line per settled change rather than one per drag event.
      window.clearTimeout(settleTimer.current);
      settleTimer.current = window.setTimeout(() => {
        const report = lastReport.current;
        const landed = channels.find((c) => c.id === id);
        const current = report?.reports.find((c) => c.id === id) ?? landed;
        if (!current) return;
        const stage =
          report?.stages.find((s) => s.stage === current.stage)?.backend ??
          (current.stage ?? "no stage");
        write({
          kind: current.fidelity === "exact" ? "apply" : "warn",
          subject: current.key,
          detail: current.note ?? `${stage} · ${current.fidelity}`,
          latencyUs: report?.micros,
        });
      }, SETTLE_MS);
    },
    [channels, push, state, target, write],
  );

  const applyReport = useCallback(
    (report: ApplyReport) => {
      setChannels(report.reports);
      lastReport.current = report;
    },
    [],
  );

  const toggle = useCallback(() => {
    const next = !enabled;
    setEnabled(next);
    setEnabledIpc(next).then((report) => {
      applyReport(report);
      write({
        kind: next ? "apply" : "restore",
        subject: next ? "ON" : "OFF",
        detail: next
          ? "state re-applied to the display"
          : "display restored, state kept",
        latencyUs: report.micros,
      });
    });
  }, [applyReport, enabled, write]);

  const retarget = useCallback(
    (key: string | "all") => {
      const next: LutTarget = key === "all" ? "all" : { one: key };
      setTarget(next);
      setLutTarget(next).then((report) => {
        applyReport(report);
        write({
          kind: "apply",
          subject: "LUT",
          detail:
            key === "all"
              ? "targeting every display"
              : `targeting ${snapshot?.displays.find((d) => d.key === key)?.name ?? key}`,
          latencyUs: report.micros,
        });
      });
    },
    [applyReport, snapshot, write],
  );

  // Hold to see the original. The effect covers this window too, so an
  // in-app "after" preview would be double-transformed and therefore a
  // lie; the honest comparison is the real screen with Azure stood down.
  useEffect(() => {
    const interactive = (t: EventTarget | null) =>
      t instanceof HTMLElement && (t.tagName === "BUTTON" || t.tagName === "INPUT");

    const down = (e: KeyboardEvent) => {
      if (e.code !== "Space" || e.repeat || interactive(e.target)) return;
      e.preventDefault();
      bypassedRef.current = true;
      setBypassed(true);
      // Stand down for real. Dimming the interface would be theatre: the
      // comparison being made is against the desktop behind it.
      restoreDisplay();
    };
    const release = () => {
      if (!bypassedRef.current) return;
      bypassedRef.current = false;
      setBypassed(false);
      if (stateRef.current) push(stateRef.current, targetRef.current);
    };
    const up = (e: KeyboardEvent) => {
      if (e.code === "Space") release();
    };

    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", release);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", release);
    };
  }, [push]);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useEffect(() => {
    targetRef.current = target;
  }, [target]);

  useEffect(() => () => {
    window.clearTimeout(pulseTimer.current);
    window.clearTimeout(settleTimer.current);
  }, []);

  const focusedChannel = useMemo(
    () => channels.find((c) => c.id === focused) ?? channels[0],
    [channels, focused],
  );

  const gamma = channels.find((c) => c.id === "gamma");
  const [gammaNotice, setGammaNotice] = useState<string | null>(null);

  if (!snapshot || !state || !focusedChannel) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-void">
        <span className="ng-label">READING THE DISPLAY…</span>
      </div>
    );
  }

  const lutTargetKey: string | "all" = target === "all" ? "all" : target.one;

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-void">
      <StatusStrip
        environment={snapshot.environment}
        displays={snapshot.displays}
        enabled={enabled}
        live={LIVE}
        onToggle={toggle}
      />

      {!LIVE && (
        <WarningRow
          tone="alert"
          code="NO COLOUR CORE"
          message="This is the interface running in a browser. Nothing here is reaching a display. Run bun tauri dev for the real thing."
        />
      )}

      {gamma?.fidelity === "clamped" && (
        <WarningRow
          tone="warn"
          code="GAMMA CLAMPED"
          message={
            gammaNotice ??
            "Windows limits gamma ramps until GdiIcmGammaRange is set to 256. Azure works without it at reduced range."
          }
          actionLabel={gammaNotice ? undefined : "UNLOCK FULL RANGE"}
          onAction={() => {
            unlockGammaRange().then((outcome) => {
              const message =
                outcome.kind === "unlocked"
                  ? "Written. Windows reads this at sign-in, so the full range is available after you sign out and back in."
                  : outcome.kind === "needsElevation"
                    ? "Unlocking the gamma range writes an HKLM registry value and needs an elevated Azure. Restart as administrator to change it."
                    : outcome.reason;
              setGammaNotice(message);
              write({
                kind: outcome.kind === "unlocked" ? "apply" : "warn",
                subject: "GAM",
                detail: message,
              });
            });
          }}
        />
      )}

      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto win:grid win:grid-cols-[minmax(0,1fr)_minmax(0,19rem)] win:overflow-hidden lg:grid-cols-[minmax(0,1fr)_minmax(0,26rem)]">
        <div className="flex min-w-0 flex-col win:min-h-0 win:overflow-y-auto win:border-r win:border-r-rule">
          <div className="ng-rule-b grid grid-cols-[3ch_1fr_6ch] sm:grid-cols-[3ch_1fr_6ch_7ch_8ch] items-center gap-x-3 px-4 py-1.5">
            <span className="ng-label">CH</span>
            <span className="ng-label">LEVEL</span>
            <span className="ng-label text-right">VALUE</span>
            <span className="ng-label hidden sm:block">STAGE</span>
            <span className="ng-label hidden sm:block">FIDELITY</span>
          </div>

          {channels.map((c) => (
            <ChannelRow
              key={c.id}
              channel={c}
              value={state[c.id]}
              bypassed={bypassed}
              onChange={(v) => setChannel(c.id, v)}
              onFocus={() => setFocused(c.id)}
            />
          ))}

          <DisplayRows
            displays={snapshot.displays}
            lutTarget={lutTargetKey}
            onTarget={retarget}
          />

          <EventLog entries={log} />

          <div className="ng-rule-t flex flex-wrap gap-x-5 gap-y-1 px-4 py-2">
            <button
              type="button"
              onClick={() => {
                const neutral = channels.reduce(
                  (acc, c) => ({ ...acc, [c.id]: c.range.neutral }),
                  {} as ColorState,
                );
                setState(neutral);
                push(neutral, target);
                write({
                  kind: "restore",
                  subject: "ALL",
                  detail: "every channel back to neutral",
                });
              }}
              className="ng-label text-dim hover:text-text"
            >
              RESET TO NEUTRAL
            </button>
          </div>
        </div>

        <aside className="ng-rule-t flex min-w-0 flex-col bg-field win:border-t-0 win:min-h-0 win:overflow-y-auto">
          <Monument
            channel={focusedChannel}
            value={state[focusedChannel.id]}
            bypassed={bypassed}
          />
          <div className="ng-rule-t">
            <SignalChain
              channels={channels}
              state={state}
              pulse={pulse}
              pulseStage={pulseStage}
            />
          </div>
        </aside>
      </main>

      <PresetBar presets={[]} activeId="" onSelect={() => {}} />

      <FooterBar bypassed={bypassed} />
    </div>
  );
}
