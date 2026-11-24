//! Notices when Windows shuts down, restarts or signs out, so the session is not later listed
//! as "ended unexpectedly".
//!
//! Windows sends `WM_ENDSESSION` to every top-level window just before it ends the process.
//! A hidden window on its own thread answers it by writing a small marker file (reason and
//! time). Finishing the session itself waits for the next start, where `SessionManager`
//! reads the marker: after `WM_ENDSESSION` the process may be ended at any moment, and the
//! session lives on the UI thread.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Name of the marker file next to `current.json`.
pub const MARKER_FILE: &str = "session-end.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndKind {
    /// Shut down or restarted (Windows does not say which).
    Shutdown,
    SignOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndMarker {
    pub kind: EndKind,
    /// When Windows announced the end (ms since the epoch).
    pub t: u64,
}

static MARKER_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Starts the watcher thread; `marker` is where the marker file goes.
pub fn spawn_watcher(marker: PathBuf) {
    if MARKER_PATH.set(marker).is_err() {
        return; // already running
    }
    let _ = std::thread::Builder::new().name("end-session".into()).spawn(|| unsafe { run() });
}

unsafe fn run() {
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, MSG, WINDOW_EX_STYLE, WNDCLASSW, WS_POPUP,
    };

    let Ok(module) = GetModuleHandleW(None) else { return };
    let class = w!("StromWidgetEndSession");
    let wc = WNDCLASSW { lpfnWndProc: Some(wnd_proc), hInstance: module.into(), lpszClassName: class, ..Default::default() };
    RegisterClassW(&wc);
    // A real top-level window that is never shown: message-only windows (HWND_MESSAGE) do not
    // get the end-of-session broadcast.
    let created = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("Strom-Widget end session"), WS_POPUP, 0, 0, 0, 0, None, None, module, None);
    if created.is_err() {
        return;
    }
    // Blocks until a message arrives: no wakeups while nothing happens.
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, ENDSESSION_LOGOFF, WM_ENDSESSION, WM_QUERYENDSESSION};
    match msg {
        // Never hold up the shutdown.
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            // wParam = 0: the end was cancelled after all.
            if wparam.0 != 0 {
                let kind = if (lparam.0 as u32) & ENDSESSION_LOGOFF != 0 { EndKind::SignOut } else { EndKind::Shutdown };
                write_marker(kind);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn write_marker(kind: EndKind) {
    let Some(path) = MARKER_PATH.get() else { return };
    let marker = EndMarker { kind, t: crate::session::now_ms() };
    if let Ok(json) = serde_json::to_string(&marker) {
        let _ = std::fs::write(path, json);
    }
}

/// Reads and removes the marker. Called once at start, before the session is recovered.
pub fn take_marker(path: &std::path::Path) -> Option<EndMarker> {
    let raw = std::fs::read_to_string(path).ok();
    let _ = std::fs::remove_file(path);
    serde_json::from_str(&raw?).ok()
}

/// Whether the marker belongs to a session that was last saved at `last` (ms). The session
/// is saved every 30 s, so the end comes at most that long after the last save; a marker from
/// another session (e.g. one that was too short to keep) lies outside this window.
pub fn marker_matches(m: &EndMarker, start: u64, last: u64) -> bool {
    m.t >= start && m.t <= last.saturating_add(120_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_window() {
        let m = EndMarker { kind: EndKind::Shutdown, t: 1_000_000 };
        assert!(marker_matches(&m, 0, 990_000)); // 10 s after the last save
        assert!(!marker_matches(&m, 0, 800_000)); // over 2 min after: from another run
        assert!(!marker_matches(&m, 1_100_000, 1_200_000)); // before this session began
    }
}
