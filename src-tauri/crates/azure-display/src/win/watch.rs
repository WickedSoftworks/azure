//! Noticing which game is in front.
//!
//! `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` asks Windows to deliver
//! foreground events to *this* process. Nothing of Azure's is loaded into
//! the game, no DLL, no hook procedure running in its address space — the
//! whole difference between a watcher and the thing kernel anti-cheat
//! exists to stop. The only other call here reads a process's own image
//! path with `PROCESS_QUERY_LIMITED_INFORMATION`, which is deliberately
//! not `PROCESS_VM_READ`; Azure never asks to read another process.
//!
//! `WINEVENT_SKIPOWNPROCESS` matters as much for behaviour as for safety:
//! without it, alt-tabbing into Azure to adjust a slider would count as a
//! focus change and yank the game's preset out from under the value being
//! adjusted.

use std::cell::{Cell, RefCell};

use crate::backend::Foreground;
use std::thread::{self, JoinHandle};

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, KillTimer,
    PostThreadMessageW, SetTimer, TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG, OBJID_WINDOW,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_QUIT, WM_TIMER,
};

/// How long the foreground has to hold still before Azure acts on it.
/// Alt-tabbing through four windows should change the display once, at the
/// end, not four times on the way.
const SETTLE_MS: u32 = 300;

type Sink = Box<dyn Fn(Foreground)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    static PENDING: Cell<isize> = const { Cell::new(0) };
    static TIMER: Cell<usize> = const { Cell::new(0) };
}

/// A running watcher. Dropping it stops the thread and unhooks.
pub struct Watcher {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl Watcher {
    /// Starts watching. `sink` is called on the watcher's own thread once
    /// the foreground has settled, never during the event itself.
    pub fn spawn(sink: impl Fn(Foreground) + Send + 'static) -> Watcher {
        let (tx, rx) = std::sync::mpsc::channel::<u32>();

        let handle = thread::Builder::new()
            .name("azure-foreground-watcher".into())
            .spawn(move || {
                SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
                // SAFETY: the id is this thread's own, used only to post it
                // a quit message.
                let _ = tx.send(unsafe { GetCurrentThreadId() });

                // SAFETY: out-of-process hook. Windows queues events to
                // this thread; no code of ours enters another process.
                let hook = unsafe {
                    SetWinEventHook(
                        EVENT_SYSTEM_FOREGROUND,
                        EVENT_SYSTEM_FOREGROUND,
                        None,
                        Some(on_foreground),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    )
                };
                if hook.is_invalid() {
                    return;
                }

                pump();

                // SAFETY: paired with the SetWinEventHook above, on the
                // thread that set it, as the API requires.
                unsafe {
                    let _ = UnhookWinEvent(hook);
                }
            })
            .expect("the foreground watcher thread could not start");

        let thread_id = rx.recv().unwrap_or(0);
        Watcher { thread_id, handle: Some(handle) }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            // SAFETY: posts WM_QUIT to our own watcher thread, which ends
            // its message loop and unhooks on the way out.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn pump() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: standard message loop on this thread's own queue.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            // 0 is WM_QUIT, -1 is an error; either way we are done.
            return;
        }
        if msg.message == WM_TIMER {
            settle();
            continue;
        }
        // SAFETY: the message came from GetMessageW above.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The hook callback. Does as little as possible: records the window and
/// restarts the settle timer. Resolving a process here would run inside
/// the event delivery, and a slow resolve would hold up the queue.
unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    if hwnd.is_invalid() {
        return;
    }

    PENDING.with(|p| p.set(hwnd.0 as isize));
    TIMER.with(|t| {
        let existing = t.get();
        if existing != 0 {
            // SAFETY: a thread timer this thread created.
            let _ = unsafe { KillTimer(None, existing) };
        }
        // SAFETY: a thread timer (null window), delivered to this queue.
        t.set(unsafe { SetTimer(None, 0, SETTLE_MS, None) });
    });
}

/// The foreground has held still. Resolve it and hand it over.
fn settle() {
    TIMER.with(|t| {
        let id = t.get();
        if id != 0 {
            // SAFETY: a thread timer this thread created.
            let _ = unsafe { KillTimer(None, id) };
            t.set(0);
        }
    });

    let hwnd = HWND(PENDING.with(|p| p.replace(0)) as *mut _);
    if hwnd.is_invalid() {
        return;
    }
    let Some(found) = resolve(hwnd) else { return };
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(found);
        }
    });
}

/// Whatever is in front right now. Used by the watcher and by the
/// interface's "capture the next window" binding.
pub fn current_foreground() -> Option<Foreground> {
    // SAFETY: reads the foreground window handle; no state changes.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    resolve(hwnd)
}

fn resolve(hwnd: HWND) -> Option<Foreground> {
    let mut pid = 0u32;
    // SAFETY: reads the owning process id of a window handle.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    // SAFETY: reads our own process id.
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return None;
    }

    let path = full_path(pid);
    let exe = path
        .as_deref()
        .map(|p| basename(p).to_string())
        .or_else(|| exe_name_from_snapshot(pid))?;

    Some(Foreground { path, exe })
}

/// The process's own image path.
///
/// `PROCESS_QUERY_LIMITED_INFORMATION` is the narrowest right that answers
/// this question, and notably not `PROCESS_VM_READ`. Elevated games refuse
/// even this, which is why the caller has a fallback rather than an error.
fn full_path(pid: u32) -> Option<String> {
    // SAFETY: opens a handle for one documented query and closes it below.
    let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` is `len` wide characters and outlives the call.
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    }
    .is_ok();
    // SAFETY: the handle came from OpenProcess above and is closed once.
    unsafe {
        let _ = CloseHandle(handle);
    }

    ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// The executable name without opening the process at all.
///
/// A snapshot lists elevated processes by name even when a handle to them
/// is refused, which is how a preset still matches VALORANT.
fn exe_name_from_snapshot(pid: u32) -> Option<String> {
    // SAFETY: a read-only snapshot of the process list, closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    let mut found = None;
    // SAFETY: `entry` is sized as the API requires and lives for the walk.
    unsafe {
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    found = Some(String::from_utf16_lossy(&entry.szExeFile[..end]));
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    found
}

fn basename(path: &str) -> &str {
    let cut = path.rfind(['\\', '/']).map(|i| i + 1).unwrap_or(0);
    &path[cut..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_foreground_resolves_to_something_named() {
        // Whatever is in front on this machine. None is a legitimate
        // answer on a session with no foreground window, or when the
        // foreground is Azure's own test runner.
        match current_foreground() {
            Some(found) => {
                assert!(!found.exe.is_empty(), "a process with no name is not an answer");
                assert!(found.exe.to_lowercase().ends_with(".exe"), "{found:?}");
                if let Some(path) = &found.path {
                    assert!(path.to_lowercase().ends_with(&found.exe.to_lowercase()));
                }
            }
            None => eprintln!("skipping: nothing resolvable in the foreground here"),
        }
    }

    #[test]
    fn a_watcher_starts_and_stops_without_hanging() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        let watcher = Watcher::spawn(move |f| sink.lock().unwrap().push(f));
        // Nothing is asserted about events: a test cannot make another
        // application take the foreground. What this pins down is that the
        // hook installs, the message loop runs, and Drop unwinds it rather
        // than leaving a thread behind.
        drop(watcher);
    }
}
