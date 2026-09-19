import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ChannelRow } from "@/components/ChannelRow";
import { DisplayRows } from "@/components/DisplayRows";
import { EventLog, type LogEntry } from "@/components/EventLog";
import { FooterBar } from "@/components/FooterBar";
import { HotkeyPanel } from "@/components/HotkeyPanel";
import { Monument } from "@/components/Monument";
import { PresetBar } from "@/components/PresetBar";
import { SignalChain } from "@/components/SignalChain";
import { StatusStrip } from "@/components/StatusStrip";
import { WarningRow } from "@/components/WarningRow";
import {
  addPreset,
  applyState,
  bindPreset,
  isTauri,
  loadResidency,
  loadSnapshot,
  onActivated,
  onSnapshot,
  removePreset,
  renamePreset,
  resetBindings,
  restartElevated,
  selectPreset,
  setAutostart,
  setBinding,
  setBypass as setBypassIpc,
  setEnabled as setEnabledIpc,
  setLutTarget,
  unlockGammaRange,
} from "@/lib/ipc";
import type {
  Action,
  ApplyReport,
  BindingSet,
  ChannelId,
  ChannelReport,
  ColorState,
  LutTarget,
  MatchKind,
  Preset,
  Registration,
  ResidencyView,
  Snapshot,
  Stage,
} from "@/lib/model";

/**
 * What the footer advertises before the bindings have loaded: nothing.
 * A reminder that appears and then changes is worse than one that
 * arrives a frame late.
 */
const EMPTY_BINDINGS: BindingSet = {
  toggleEnabled: null,
  cycleNext: null,
  cyclePrev: null,
  restoreDisplay: null,
  holdBypass: null,
};

/** How long after the last drag event a change is considered settled. */
const SETTLE_MS = 350;

const LIVE = isTauri();

/** `D:\games\r5apex.exe` becomes `R5APEX`. */
function nameFor(pathOrExe: string): string {
  const i = Math.max(pathOrExe.lastIndexOf("\\"), pathOrExe.lastIndexOf("/"));
  const base = i >= 0 ? pathOrExe.slice(i + 1) : pathOrExe;
  return base.replace(/\.exe$/i, "").toUpperCase() || "GAME";
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [channels, setChannels] = useState<ChannelReport[]>([]);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [activeId, setActiveId] = useState("");
  const [matchedBy, setMatchedBy] = useState<MatchKind | null>(null);
  const [state, setState] = useState<ColorState | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [target, setTarget] = useState<LutTarget>("all");
  const [focused, setFocused] = useState<ChannelId>("vibrance");
  const [bypassed, setBypassed] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [pulse, setPulse] = useState(0);
  const [pulseStage, setPulseStage] = useState<Stage | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [gammaNotice, setGammaNotice] = useState<string | null>(null);
  const [residency, setResidency] = useState<ResidencyView | null>(null);
  // Set while an elevated window holds the foreground, which is when
  // Windows UIPI silently refuses to deliver Azure's hotkeys.
  const [blockedBy, setBlockedBy] = useState<string | null>(null);

  const logId = useRef(1);
  const pulseTimer = useRef<number | undefined>(undefined);
  const settleTimer = useRef<number | undefined>(undefined);
  const inFlight = useRef(false);
  const queued = useRef<{ state: ColorState; target: LutTarget } | null>(null);
  const lastReport = useRef<ApplyReport | null>(null);
  // The bypass and capture listeners are bound once, so they read the live
  // values through refs rather than closing over the first render's.
  const bypassedRef = useRef(false);
  const capturingRef = useRef(false);

  const write = useCallback((entry: Omit<LogEntry, "id" | "time">) => {
    setLog((prev) =>
      [
        { id: logId.current++, time: new Date().toTimeString().slice(0, 8), ...entry },
        ...prev,
      ].slice(0, 60),
    );
  }, []);

  const absorb = useCallback((snap: Snapshot) => {
    setSnapshot(snap);
    setChannels(snap.channels);
    setPresets(snap.presets);
    setActiveId(snap.activeId);
    setMatchedBy(snap.matchedBy);
    setState(snap.state);
    setEnabled(snap.enabled);
    setTarget(snap.target);
  }, []);

  const refresh = useCallback(() => loadSnapshot().then(absorb), [absorb]);

  const complain = useCallback(
    (subject: string) => (e: unknown) => {
      write({ kind: "warn", subject, detail: e instanceof Error ? e.message : String(e) });
    },
    [write],
  );

  useEffect(() => {
    loadSnapshot().then((snap) => {
      absorb(snap);
      for (const notice of snap.notices) {
        write({ kind: "warn", subject: "CORE", detail: notice });
      }
    });
  }, [absorb, write]);

  useEffect(() => {
    loadResidency().then((view) => {
      setResidency(view);
      for (const notice of view.notices) {
        write({ kind: "warn", subject: "KEYS", detail: notice });
      }
      for (const registration of view.registrations) {
        if (registration.outcome.kind === "refused") {
          write({
            kind: "warn",
            subject: "KEYS",
            detail: `${registration.chord} did not register: ${registration.outcome.reason}`,
          });
        }
      }
    });
  }, [write]);

  /**
   * Applies a change to the bindings and keeps what Windows said about
   * it. The registrations come back from the same call, so a refusal is
   * visible without a second round trip.
   */
  const absorbRegistrations = useCallback(
    (registrations: Registration[]) => {
      setResidency((prev: ResidencyView | null) => (prev ? { ...prev, registrations } : prev));
      void loadResidency().then(setResidency);
      for (const registration of registrations) {
        if (registration.outcome.kind === "refused") {
          write({
            kind: "warn",
            subject: "KEYS",
            detail: `${registration.chord} did not register: ${registration.outcome.reason}`,
          });
        }
      }
    },
    [write],
  );

  const bind = useCallback(
    (action: Action, chord: string | null) => {
      setBinding(action, chord)
        .then(absorbRegistrations)
        .catch(complain("KEYS"));
    },
    [absorbRegistrations, complain],
  );

  /**
   * Something outside this window changed the display: a global hotkey,
   * or the tray menu. The field follows, because the screen already has.
   */
  useEffect(() => onSnapshot(absorb), [absorb]);

  /**
   * Sends the newest state and drops anything stale behind it.
   *
   * A slider drag fires far faster than a write to two backends across
   * however many displays completes, and the engine is one thread. Queuing
   * every tick would make the screen lag the thumb by whatever the queue
   * had built up; the only value worth writing is the latest one.
   */
  const push = useCallback(
    (next: ColorState, nextTarget: LutTarget) => {
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
        .catch(complain("CORE"))
        .finally(() => {
          inFlight.current = false;
          const pending = queued.current;
          queued.current = null;
          if (pending) push(pending.state, pending.target);
        });
    },
    [complain],
  );

  const setChannel = useCallback(
    (id: ChannelId, value: number) => {
      if (!state) return;
      const next = { ...state, [id]: value };
      setState(next);
      // The core writes this into whichever preset is active; the copy
      // here is only what the sliders are drawn from.
      setPresets((prev) => prev.map((p) => (p.id === activeId ? { ...p, state: next } : p)));

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
        const current = report?.reports.find((c) => c.id === id) ?? channel;
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
    [activeId, channels, push, state, target, write],
  );

  const absorbReport = useCallback((report: ApplyReport) => {
    setChannels(report.reports);
    lastReport.current = report;
  }, []);

  const toggle = useCallback(() => {
    const next = !enabled;
    setEnabled(next);
    setEnabledIpc(next)
      .then((report) => {
        absorbReport(report);
        write({
          kind: next ? "apply" : "restore",
          subject: next ? "ON" : "OFF",
          detail: next ? "state re-applied to the display" : "display restored, state kept",
          latencyUs: report.micros,
        });
      })
      .catch(complain("CORE"));
  }, [absorbReport, complain, enabled, write]);

  const retarget = useCallback(
    (key: string | "all") => {
      const next: LutTarget = key === "all" ? "all" : { one: key };
      setTarget(next);
      setLutTarget(next)
        .then((report) => {
          absorbReport(report);
          write({
            kind: "apply",
            subject: "LUT",
            detail:
              key === "all"
                ? "targeting every display"
                : `targeting ${snapshot?.displays.find((d) => d.key === key)?.name ?? key}`,
            latencyUs: report.micros,
          });
        })
        .catch(complain("CORE"));
    },
    [absorbReport, complain, snapshot, write],
  );

  // ── presets ───────────────────────────────────────────────────────────

  const choose = useCallback(
    (id: string) => {
      selectPreset(id)
        .then((report) => {
          absorbReport(report);
          return refresh();
        })
        .catch(complain("PRESET"));
    },
    [absorbReport, complain, refresh],
  );

  const create = useCallback(
    (exe: string, how: string) => {
      const name = nameFor(exe);
      addPreset(name, exe)
        .then((id) => {
          write({ kind: "activate", subject: name, detail: `bound to ${exe} · ${how}` });
          return selectPreset(id).then(() => refresh());
        })
        .catch(complain("PRESET"));
    },
    [complain, refresh, write],
  );

  const browse = useCallback(() => {
    open({
      multiple: false,
      directory: false,
      filters: [{ name: "Programs", extensions: ["exe"] }],
    })
      .then((picked) => {
        if (typeof picked === "string") create(picked, "picked");
      })
      .catch(complain("PRESET"));
  }, [complain, create]);

  const capture = useCallback(() => {
    capturingRef.current = true;
    setCapturing(true);
    write({
      kind: "activate",
      subject: "CAPTURE",
      detail: "waiting for the next window to take focus",
    });
  }, [write]);

  const cancelCapture = useCallback(() => {
    capturingRef.current = false;
    setCapturing(false);
  }, []);

  /**
   * The watcher changed the preset, or a capture is waiting for a window.
   * Either way the field has to follow: the display already has.
   */
  useEffect(
    () =>
      onActivated(({ foreground, snapshot: snap }) => {
        if (capturingRef.current) {
          capturingRef.current = false;
          setCapturing(false);
          create(foreground.path ?? foreground.exe, "captured from the foreground");
          return;
        }
        absorb(snap);
        // Windows will not deliver an unelevated process's hotkeys while
        // an elevated window has focus. The watcher is the only thing
        // that can tell, so it is what says so.
        setBlockedBy(foreground.elevated ? foreground.exe : null);
        const preset = snap.presets.find((p) => p.id === snap.activeId);
        write({
          kind: "activate",
          subject: preset?.name ?? "DESKTOP",
          detail: snap.matchedBy
            ? `${foreground.exe} · matched by ${snap.matchedBy === "fullPath" ? "full path" : "exe name"}`
            : `${foreground.exe} · nothing bound, fell back to the desktop`,
        });
      }),
    [absorb, create, write],
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
      void setBypassIpc(true);
    };
    const release = () => {
      if (!bypassedRef.current) return;
      bypassedRef.current = false;
      setBypassed(false);
      setBypassIpc(false)
        .then(absorbReport)
        .catch(() => {});
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
  }, [absorbReport]);

  useEffect(
    () => () => {
      window.clearTimeout(pulseTimer.current);
      window.clearTimeout(settleTimer.current);
    },
    [],
  );

  const focusedChannel = useMemo(
    () => channels.find((c) => c.id === focused) ?? channels[0],
    [channels, focused],
  );

  const activePreset = useMemo(
    () => presets.find((p) => p.id === activeId),
    [presets, activeId],
  );

  const gamma = channels.find((c) => c.id === "gamma");

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
        preset={activePreset}
        automatic={matchedBy !== null}
        live={LIVE}
        onToggle={toggle}
      />

      {!LIVE && (
        <WarningRow
          tone="alert"
          code="NO COLOUR CORE"
          message="This is the interface running in a browser. Nothing here is reaching a display, and presets cannot be changed. Run bun tauri dev for the real thing."
        />
      )}

      {blockedBy && residency && !residency.elevated && (
        <WarningRow
          tone="warn"
          code="HOTKEYS BLOCKED"
          message={`${blockedBy} is running as administrator, and Windows will not deliver Azure's hotkeys while it has focus. Preset switching still works.`}
          actionLabel="RESTART AS ADMINISTRATOR"
          onAction={() => {
            restartElevated().catch(complain("KEYS"));
          }}
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

          {residency && (
            <HotkeyPanel
              bindings={residency.bindings}
              registrations={residency.registrations}
              conflicts={residency.conflicts}
              autostart={residency.autostart}
              live={LIVE}
              onBind={bind}
              onReset={() => {
                resetBindings().then(absorbRegistrations).catch(complain("KEYS"));
              }}
              onAutostart={(next) => {
                setAutostart(next)
                  .then((now) => {
                    setResidency((prev: ResidencyView | null) =>
                      prev ? { ...prev, autostart: now } : prev,
                    );
                    write({
                      kind: now ? "apply" : "restore",
                      subject: "START",
                      detail: now
                        ? "Azure will start hidden with Windows"
                        : "Azure will no longer start with Windows",
                    });
                  })
                  .catch(complain("START"));
              }}
            />
          )}

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
                  detail: `every channel in ${activePreset?.name ?? "this preset"} back to neutral`,
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

      <PresetBar
        presets={presets}
        activeId={activeId}
        matchedBy={matchedBy}
        capturing={capturing}
        live={LIVE}
        onSelect={choose}
        onBrowse={browse}
        onCapture={capture}
        onCancelCapture={cancelCapture}
        onRename={(id, name) => renamePreset(id, name).then(refresh).catch(complain("PRESET"))}
        onUnbind={(id) => bindPreset(id, null).then(refresh).catch(complain("PRESET"))}
        onDelete={(id) =>
          removePreset(id)
            .then((report) => {
              absorbReport(report);
              return refresh();
            })
            .catch(complain("PRESET"))
        }
      />

      <FooterBar
        bypassed={bypassed}
        bindings={residency?.bindings ?? EMPTY_BINDINGS}
      />
    </div>
  );
}
