//! Asking Windows for keys, and noticing when it says no.
//!
//! `RegisterHotKey` binds a chord to *this* thread's message queue —
//! nothing is installed anywhere else, no keyboard hook exists, and no
//! keystroke that is not one of Azure's own chords is ever seen by this
//! process. That distinction is the same one `watch.rs` makes, and it
//! matters for the same reason: a global keyboard hook sitting beside
//! kernel anti-cheat is indistinguishable, to anyone auditing the machine,
//! from the thing anti-cheat exists to stop.
//!
//! The cost of refusing the hook is that hold-to-bypass has to watch for
//! its own key release. `WM_HOTKEY` fires on press and Windows sends
//! nothing on release, so while that one key is down this thread polls
//! `GetAsyncKeyState` for it. The poll exists only between press and
//! release, and only for a chord the player deliberately bound.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::thread::{self, JoinHandle};

use azure_settings::{Action, BindingSet, Chord, Key};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PostThreadMessageW, SetTimer, TranslateMessage, MSG,
    WM_HOTKEY, WM_QUIT, WM_TIMER,
};

use crate::hotkeys::{HotkeyEvent, Outcome, Phase, Registration};

/// How often the held key is checked while hold-to-bypass is down. Fast
/// enough that letting go feels immediate, and it runs only while a key
/// is actually held.
const POLL_MS: u32 = 16;

/// Windows' own code for a chord another program already owns. Worth
/// naming because it is the refusal players will actually hit.
const ERROR_HOTKEY_ALREADY_REGISTERED: i32 = 1409;

type Sink = Box<dyn Fn(HotkeyEvent)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    /// Hotkey id to the action it stands for.
    static BOUND: RefCell<HashMap<i32, Action>> = RefCell::new(HashMap::new());
    /// The virtual key currently held for hold-to-bypass, and the timer
    /// watching it. Zero when nothing is held.
    static HELD_VK: Cell<i32> = const { Cell::new(0) };
    static POLL_TIMER: Cell<usize> = const { Cell::new(0) };
    /// The virtual key of the bypass chord, recorded when it registered.
    /// Zero when hold-to-bypass is unbound, which is the default.
    static BYPASS_VK: Cell<i32> = const { Cell::new(0) };
}

/// The thread that owns the registrations. Dropping it unregisters every
/// chord, which is what makes rebinding a matter of replacing the whole
/// thing rather than diffing it.
pub struct HotkeyThread {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl HotkeyThread {
    pub fn spawn(
        bindings: &BindingSet,
        sink: impl Fn(HotkeyEvent) + Send + 'static,
    ) -> (HotkeyThread, Vec<Registration>) {
        let wanted = bindings.bound();
        let (tx_id, rx_id) = std::sync::mpsc::channel::<u32>();
        let (tx_reg, rx_reg) = std::sync::mpsc::channel::<Vec<Registration>>();

        let handle = thread::Builder::new()
            .name("azure-hotkeys".into())
            .spawn(move || {
                SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
                // SAFETY: the id is this thread's own, used only to post
                // it a quit message.
                let _ = tx_id.send(unsafe { GetCurrentThreadId() });

                let registered = register_all(&wanted);
                let taken: Vec<i32> = registered
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| matches!(r.outcome, Outcome::Registered))
                    .map(|(i, _)| i as i32)
                    .collect();
                let _ = tx_reg.send(registered);

                pump();

                for id in taken {
                    // SAFETY: unregistering an id this thread registered,
                    // on the thread that registered it, as required.
                    unsafe {
                        let _ = UnregisterHotKey(None, id);
                    }
                }
            })
            .expect("the hotkey thread could not start");

        let thread_id = rx_id.recv().unwrap_or(0);
        let registrations = rx_reg.recv().unwrap_or_default();
        (HotkeyThread { thread_id, handle: Some(handle) }, registrations)
    }
}

impl Drop for HotkeyThread {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            // SAFETY: posts WM_QUIT to our own hotkey thread, which ends
            // its message loop and unregisters on the way out.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Registers each wanted chord, in order, and answers for each one
/// separately. The id is the index, so the caller's list and the ids Windows
/// reports back stay in step.
fn register_all(wanted: &[(Action, Chord)]) -> Vec<Registration> {
    let mut out = Vec::with_capacity(wanted.len());

    for (index, &(action, chord)) in wanted.iter().enumerate() {
        let id = index as i32;
        let Some(vk) = virtual_key(chord.key) else {
            out.push(Registration::refused(
                action,
                chord,
                format!("{} is not a key Windows can bind", chord.key),
            ));
            continue;
        };

        // SAFETY: binds the chord to this thread's message queue. A null
        // window means WM_HOTKEY is posted to the thread, which is what
        // `pump` below reads.
        let result = unsafe { RegisterHotKey(None, id, modifiers(chord), vk as u32) };

        match result {
            Ok(()) => {
                BOUND.with(|b| b.borrow_mut().insert(id, action));
                if action == Action::HoldBypass {
                    BYPASS_VK.with(|v| v.set(vk as i32));
                }
                out.push(Registration { action, chord, outcome: Outcome::Registered });
            }
            Err(e) => {
                let reason = if e.code().0 & 0xFFFF == ERROR_HOTKEY_ALREADY_REGISTERED {
                    "another program already holds this key".to_string()
                } else {
                    e.message().trim_end_matches(['.', ' ']).to_string()
                };
                out.push(Registration::refused(action, chord, reason));
            }
        }
    }

    out
}

fn modifiers(chord: Chord) -> HOT_KEY_MODIFIERS {
    let mut flags = MOD_NOREPEAT;
    if chord.mods.ctrl {
        flags |= MOD_CONTROL;
    }
    if chord.mods.alt {
        flags |= MOD_ALT;
    }
    if chord.mods.shift {
        flags |= MOD_SHIFT;
    }
    if chord.mods.win {
        flags |= MOD_WIN;
    }
    flags
}

/// The Windows virtual-key code for a bindable key.
///
/// This mapping lives here rather than in `azure-settings` on purpose:
/// that crate is pure, and a virtual-key code is a Windows fact.
fn virtual_key(key: Key) -> Option<u16> {
    Some(match key {
        // Letters and digits share their ASCII value as a virtual key,
        // which is the one place Windows made this easy.
        Key::Char(c) if c.is_ascii_alphanumeric() => c.to_ascii_uppercase() as u16,
        Key::Char(_) => return None,
        Key::F(n) if (1..=24).contains(&n) => 0x70 + (n as u16 - 1),
        Key::F(_) => return None,
        Key::Left => 0x25,
        Key::Up => 0x26,
        Key::Right => 0x27,
        Key::Down => 0x28,
        Key::Space => 0x20,
        Key::Tab => 0x09,
        Key::Enter => 0x0D,
        Key::Backspace => 0x08,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::Pause => 0x13,
    })
}

fn pump() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: standard message loop on this thread's own queue.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            // 0 is WM_QUIT, -1 is an error; either way we are done.
            stop_watching_held_key();
            return;
        }
        match msg.message {
            WM_HOTKEY => fired(msg.wParam.0 as i32),
            WM_TIMER => check_held_key(),
            _ => {
                // SAFETY: the message came from GetMessageW above.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

fn fired(id: i32) {
    let Some(action) = BOUND.with(|b| b.borrow().get(&id).copied()) else {
        return;
    };

    emit(HotkeyEvent { action, phase: Phase::Pressed });

    // Every other action happens on the press and is over. Only the
    // bypass has a second half, and only that one starts a poll.
    if action == Action::HoldBypass {
        start_watching_held_key();
    }
}

fn start_watching_held_key() {
    // The key recorded when the bypass chord was registered, not one
    // re-derived from settings that may have changed since.
    let vk = BYPASS_VK.with(|v| v.get());
    if vk == 0 {
        return;
    }
    stop_watching_held_key();
    HELD_VK.with(|h| h.set(vk));
    // SAFETY: a thread timer (null window), delivered to this queue.
    POLL_TIMER.with(|t| t.set(unsafe { SetTimer(None, 0, POLL_MS, None) }));
}

fn check_held_key() {
    let vk = HELD_VK.with(|h| h.get());
    if vk == 0 {
        return;
    }
    // SAFETY: reads the current state of one virtual key. The high bit is
    // set while the key is down.
    let down = unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
    if down {
        return;
    }
    stop_watching_held_key();
    emit(HotkeyEvent { action: Action::HoldBypass, phase: Phase::Released });
}

fn stop_watching_held_key() {
    HELD_VK.with(|h| h.set(0));
    POLL_TIMER.with(|t| {
        let id = t.get();
        if id != 0 {
            // SAFETY: a thread timer this thread created.
            let _ = unsafe { KillTimer(None, id) };
            t.set(0);
        }
    });
}

fn emit(event: HotkeyEvent) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(event);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_settings::Action;

    #[test]
    fn every_bindable_key_has_a_virtual_key() {
        // If a key can be parsed it must be registrable; a chord that
        // parses and then cannot be bound would be a refusal with nobody
        // to blame.
        for text in [
            "ALT+A", "ALT+Z", "ALT+0", "ALT+9", "ALT+F1", "ALT+F24", "ALT+LEFT", "ALT+RIGHT",
            "ALT+UP", "ALT+DOWN", "ALT+SPACE", "ALT+TAB", "ALT+ENTER", "ALT+BACKSPACE",
            "ALT+INSERT", "ALT+DELETE", "ALT+HOME", "ALT+END", "ALT+PAGEUP", "ALT+PAGEDOWN",
            "ALT+PAUSE",
        ] {
            let chord = Chord::parse(text).expect(text);
            assert!(virtual_key(chord.key).is_some(), "{text} has no virtual key");
        }
    }

    #[test]
    fn modifiers_carry_no_repeat_so_a_held_toggle_fires_once() {
        let chord = Chord::parse("CTRL+ALT+SHIFT+R").unwrap();
        let flags = modifiers(chord);
        assert!(flags & MOD_NOREPEAT == MOD_NOREPEAT, "a held key must not repeat");
        assert!(flags & MOD_CONTROL == MOD_CONTROL);
        assert!(flags & MOD_ALT == MOD_ALT);
        assert!(flags & MOD_SHIFT == MOD_SHIFT);
        assert!(flags & MOD_WIN != MOD_WIN);
    }

    #[test]
    fn letters_map_to_their_own_ascii_value() {
        assert_eq!(virtual_key(Key::Char('V')), Some(0x56));
        assert_eq!(virtual_key(Key::Char('4')), Some(0x34));
    }

    /// The one test that asks Windows for a real key. It takes a chord no
    /// sane person has bound, proves it registers, and proves that asking
    /// twice is refused rather than silently shadowed.
    #[test]
    fn a_chord_registers_once_and_the_second_ask_is_refused() {
        let chord = Chord::parse("CTRL+ALT+SHIFT+WIN+F24").unwrap();
        let bindings = {
            let mut b = BindingSet::default();
            for action in azure_settings::ACTIONS {
                b.set(action, None);
            }
            b.set(Action::ToggleEnabled, Some(chord));
            b
        };

        let (first, regs) = HotkeyThread::spawn(&bindings, |_| {});
        assert_eq!(regs.len(), 1);
        if let Outcome::Refused { reason } = &regs[0].outcome {
            eprintln!("skipping: this machine would not give up {chord} ({reason})");
            return;
        }

        let (_second, again) = HotkeyThread::spawn(&bindings, |_| {});
        assert!(
            matches!(again[0].outcome, Outcome::Refused { .. }),
            "the same chord twice should be refused, got {:?}",
            again[0].outcome
        );

        drop(first);
    }
}
