// Windows 11 Taskbar-Docked Overlay & Placement Engine (TrafficMonitor Method)
// Locates Shell_TrayWnd and TrayNotifyWnd so our borderless topmost strip sits
// seamlessly inside the Windows 11 taskbar without ever getting in the way.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetForegroundWindow, GetSystemMetrics,
    GetWindow, GetWindowLongPtrW, GetWindowLongW, GetWindowRect, IsWindowVisible,
    SetWindowLongPtrW, SetWindowLongW, SetWindowPos, GA_ROOT, GWLP_HWNDPARENT, GWL_EXSTYLE,
    GW_HWNDPREV, HWND_BOTTOM, HWND_NOTOPMOST, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSENDCHANGING, SWP_NOSIZE, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

#[derive(Debug, Clone, Copy)]
pub struct TaskbarGeometry {
    pub taskbar_left: i32,
    pub taskbar_top: i32,
    pub taskbar_right: i32,
    pub taskbar_bottom: i32,
    pub tray_left: i32,
}

impl TaskbarGeometry {
    pub fn height(&self) -> i32 {
        (self.taskbar_bottom - self.taskbar_top).max(36)
    }
}

pub fn query_taskbar_geometry(secondary: bool) -> Option<TaskbarGeometry> {
    unsafe {
        // Shell_SecondaryTrayWnd is the taskbar on the other screen (Windows 11 "show taskbar on all displays").
        // Fall back to the primary taskbar if the secondary one does not exist.
        let secondary_hwnd: HWND = if secondary {
            FindWindowW(w!("Shell_SecondaryTrayWnd"), PCWSTR::null()).unwrap_or_default()
        } else {
            HWND::default()
        };
        let tray_hwnd: HWND = if !secondary_hwnd.0.is_null() {
            secondary_hwnd
        } else {
            FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?
        };
        if tray_hwnd.0.is_null() {
            return None;
        }
        let mut tb_rect = RECT::default();
        GetWindowRect(tray_hwnd, &mut tb_rect).ok()?;

        // Locate TrayNotifyWnd (system tray clock + notification icons on the right)
        let mut tray_left = tb_rect.right - 220;
        if let Ok(notify_hwnd) =
            FindWindowExW(tray_hwnd, HWND::default(), w!("TrayNotifyWnd"), PCWSTR::null())
        {
            if !notify_hwnd.0.is_null() {
                let mut notify_rect = RECT::default();
                if GetWindowRect(notify_hwnd, &mut notify_rect).is_ok()
                    && notify_rect.left > tb_rect.left
                {
                    tray_left = notify_rect.left;
                }
            }
        }

        Some(TaskbarGeometry {
            taskbar_left: tb_rect.left,
            taskbar_top: tb_rect.top,
            taskbar_right: tb_rect.right,
            taskbar_bottom: tb_rect.bottom,
            tray_left,
        })
    }
}

/// Calculates physical pixel coordinates (x, y, w, h, grows_up) for either:
/// - "taskbar": Docked inside the Windows 11 taskbar right next to the system tray (TrafficMonitor style)
/// - "tr" / "tc" / "br": Corner placements
#[allow(clippy::too_many_arguments)]
pub fn compute_window_rect(
    placement: &str,
    expanded: bool,
    compact_w: i32,
    compact_h: i32,
    expanded_w: i32,
    expanded_h: i32,
    taskbar_offset_x: i32,
    scale_factor: f32,
    taskbar_secondary: bool,
) -> (f32, f32, f32, f32, bool) {
    let sf = scale_factor.max(1.0);
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    if placement == "taskbar" {
        if let Some(tb) = query_taskbar_geometry(taskbar_secondary) {
            let tb_h_pts = (tb.height() as f32 / sf).clamp(38.0, 56.0);
            let strip_w_pts = compact_w as f32;
            let strip_h_pts = (tb_h_pts - 4.0).max(36.0);

            // Position right to the left of TrayNotifyWnd (system tray + clock), plus user offset
            let tray_left_pts = (tb.tray_left as f32) / sf;
            let tb_top_pts = (tb.taskbar_top as f32) / sf;
            let tb_left_pts = (tb.taskbar_left as f32) / sf;
            let tb_right_pts = (tb.taskbar_right as f32) / sf;

            let anchor_right_pts =
                (tray_left_pts - 8.0 + taskbar_offset_x as f32).clamp(tb_left_pts + strip_w_pts + 40.0, tb_right_pts - 20.0);
            // The flyout stays anchored at the tray and ignores the user offset, so changing the
            // offset while the panel is open never moves the panel under the mouse.
            let flyout_anchor_pts = (tray_left_pts - 8.0).clamp(tb_left_pts + strip_w_pts + 40.0, tb_right_pts - 20.0);

            if !expanded {
                let x = anchor_right_pts - strip_w_pts;
                let y = tb_top_pts + (tb_h_pts - strip_h_pts) / 2.0;
                return (x, y, strip_w_pts, strip_h_pts, true);
            } else {
                // Flyout mode: opens directly above the taskbar!
                let w = expanded_w as f32;
                let h = expanded_h as f32;
                let x = (flyout_anchor_pts - w).clamp(tb_left_pts + 8.0, tb_right_pts - w - 8.0);
                let y = (tb_top_pts - h - 8.0).max(8.0);
                return (x, y, w, h, true);
            }
        }
    }

    // Fallback or desktop corner placements ("tr", "tc", "br")
    let screen_w_pts = (screen_w as f32) / sf;
    let screen_h_pts = ((screen_h - 48) as f32) / sf;
    let margin = 16.0;
    let w = if expanded {
        expanded_w as f32
    } else {
        compact_w as f32
    };
    let h = if expanded {
        expanded_h as f32
    } else {
        compact_h as f32
    };

    let grows_up = placement.starts_with('b');
    let x = match placement {
        "tc" => ((screen_w_pts - w) / 2.0).round(),
        "bl" | "tl" => margin,
        _ => (screen_w_pts - w - margin).max(margin),
    };
    let y = if grows_up {
        (screen_h_pts - h - margin).max(margin)
    } else {
        margin
    };
    (x, y, w, h, grows_up)
}

unsafe fn is_shell_or_desktop_window(w: HWND) -> bool {
    let mut buf = [0u16; 64];
    let len = GetClassNameW(w, &mut buf) as usize;
    if len == 0 {
        return true;
    }
    let cls = String::from_utf16_lossy(&buf[..len]);
    matches!(
        cls.as_str(),
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
            | "ForegroundStaging"
    )
}

unsafe fn window_covers_full_screen_and_taskbar(w: HWND, hwnd: HWND, tray_hwnd: HWND, tb_rect: &RECT) -> bool {
    if w.0.is_null() || w == hwnd || w == tray_hwnd || !IsWindowVisible(w).as_bool() {
        return false;
    }
    let ex = GetWindowLongW(w, GWL_EXSTYLE) as u32;
    // Ignore click-through overlays or toolwindows
    if (ex & (WS_EX_TRANSPARENT.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0)) != 0 {
        return false;
    }
    if is_shell_or_desktop_window(w) {
        return false;
    }
    let mut r = RECT::default();
    if GetWindowRect(w, &mut r).is_err() {
        return false;
    }
    // A maximized window stops at tb_rect.top (+ ~8px border).
    // A fullscreen window (F11 browser, fullscreen video, game) extends from the top of the screen
    // all the way down to tb_rect.bottom, covering the taskbar.
    r.top <= 4
        && r.left <= tb_rect.left + 4
        && r.right >= tb_rect.right - 4
        && r.bottom >= tb_rect.bottom - 4
}

/// Returns true if the active foreground application (or a window currently above `Shell_TrayWnd`)
/// is in fullscreen mode (e.g. F11 in Browser, fullscreen video, game) covering the taskbar.
unsafe fn is_fullscreen_covering_taskbar(hwnd: HWND, tray_hwnd: HWND) -> bool {
    let mut tb_rect = RECT::default();
    if tray_hwnd.0.is_null() || GetWindowRect(tray_hwnd, &mut tb_rect).is_err() {
        return false;
    }

    // 1. Check the active foreground window (and its root window)
    let fg = GetForegroundWindow();
    if !fg.0.is_null() {
        let root = GetAncestor(fg, GA_ROOT);
        let target = if !root.0.is_null() { root } else { fg };
        if target == tray_hwnd || is_shell_or_desktop_window(target) {
            // The user explicitly focused the taskbar or Start Menu (e.g. pressed Win key),
            // so the taskbar is intentionally shown!
            return false;
        }
        if window_covers_full_screen_and_taskbar(target, hwnd, tray_hwnd, &tb_rect) {
            return true;
        }
    }

    // 2. Also check if any visible application window in front of `Shell_TrayWnd` covers the screen
    let mut cur = GetWindow(tray_hwnd, GW_HWNDPREV).unwrap_or_default();
    let mut steps = 0;
    while !cur.0.is_null() && steps < 40 {
        if window_covers_full_screen_and_taskbar(cur, hwnd, tray_hwnd, &tb_rect) {
            return true;
        }
        cur = GetWindow(cur, GW_HWNDPREV).unwrap_or_default();
        steps += 1;
    }

    false
}

unsafe fn is_tray_above_widget(hwnd: HWND, tray_hwnd: HWND) -> bool {
    let mut cur = GetWindow(hwnd, GW_HWNDPREV).unwrap_or_default();
    let mut steps = 0;
    while !cur.0.is_null() && steps < 64 {
        if cur == tray_hwnd {
            return true;
        }
        cur = GetWindow(cur, GW_HWNDPREV).unwrap_or_default();
        steps += 1;
    }
    false
}

/// Ensures our window has WS_EX_TOOLWINDOW (hidden from Alt-Tab and normal taskbar app list),
/// stays above `Shell_TrayWnd` when the taskbar is visible, and automatically steps behind
/// fullscreen windows (F11 Browser, videos, games) when the taskbar itself is covered.
/// The taskbar the widget is docked to: the secondary one if requested and present, else the primary one.
/// Owner/z-order logic must use the same taskbar the widget sits in, otherwise clicking that taskbar
/// raises it above the widget and nothing brings the widget back.
pub fn resolve_tray_hwnd(secondary: bool) -> HWND {
    unsafe {
        if secondary {
            if let Ok(h) = FindWindowW(w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) {
                if !h.0.is_null() {
                    return h;
                }
            }
        }
        FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).unwrap_or_default()
    }
}

pub fn enforce_taskbar_overlay_style(title: &str, pinned: bool, in_taskbar: bool, secondary: bool) {
    unsafe {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) {
            if !hwnd.0.is_null() {
                let tray_hwnd = resolve_tray_hwnd(secondary);
                let fullscreen = is_fullscreen_covering_taskbar(hwnd, tray_hwnd);
                let want_topmost = (pinned || in_taskbar) && !fullscreen;

                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                let desired = (ex_style | WS_EX_TOOLWINDOW.0) & !WS_EX_NOACTIVATE.0;
                if ex_style != desired {
                    let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, desired as i32);
                }

                if fullscreen {
                    if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != 0 {
                        let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
                    }
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_NOTOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                    );
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_BOTTOM,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                    );
                } else if want_topmost {
                    if !tray_hwnd.0.is_null() {
                        let target_owner = if in_taskbar { tray_hwnd.0 as isize } else { 0 };
                        if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != target_owner {
                            let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, target_owner);
                        }
                    }
                    let is_topmost = (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0) != 0;
                    if !is_topmost || (!tray_hwnd.0.is_null() && is_tray_above_widget(hwnd, tray_hwnd)) {
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOSENDCHANGING,
                        );
                    }
                }
            }
        }
    }
}

/// Spawns a lightweight background Win32 thread (25ms tick) that:
/// 1. Detects Fullscreen mode (F11 browser, video, game) and immediately sends the widget
///    behind the fullscreen window (`HWND_NOTOPMOST` + `HWND_BOTTOM`) just like the taskbar.
/// 2. When not in fullscreen, ONLY re-asserts `HWND_TOPMOST` if `Shell_TrayWnd` (the taskbar)
///    actually jumped in front of the widget (e.g. when clicking the taskbar).
pub fn spawn_taskbar_zorder_guardian(
    title: &'static str,
    z_state: std::sync::Arc<std::sync::atomic::AtomicU8>,
) {
    // z_state bits: bit 0 = pinned, bit 1 = in_taskbar
    std::thread::spawn(move || {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let mut was_fullscreen = false;
        loop {
            let bits = z_state.load(std::sync::atomic::Ordering::Relaxed);
            let pinned = (bits & 1) != 0;
            let in_taskbar = (bits & 2) != 0;
            let secondary = (bits & 4) != 0;
            if pinned || in_taskbar {
                unsafe {
                    if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) {
                        if !hwnd.0.is_null() {
                            let tray_hwnd = resolve_tray_hwnd(secondary);
                            let fullscreen = is_fullscreen_covering_taskbar(hwnd, tray_hwnd);

                            if fullscreen {
                                if !was_fullscreen
                                    || (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0)
                                        != 0
                                {
                                    if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != 0 {
                                        let _ = SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
                                    }
                                    let _ = SetWindowPos(
                                        hwnd,
                                        HWND_NOTOPMOST,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE
                                            | SWP_NOSIZE
                                            | SWP_NOACTIVATE
                                            | SWP_NOSENDCHANGING,
                                    );
                                    let _ = SetWindowPos(
                                        hwnd,
                                        HWND_BOTTOM,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE
                                            | SWP_NOSIZE
                                            | SWP_NOACTIVATE
                                            | SWP_NOSENDCHANGING,
                                    );
                                    was_fullscreen = true;
                                }
                            } else {
                                if !tray_hwnd.0.is_null() {
                                    let target_owner =
                                        if in_taskbar { tray_hwnd.0 as isize } else { 0 };
                                    if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != target_owner {
                                        let _ =
                                            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, target_owner);
                                    }
                                }

                                let is_topmost = (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32
                                    & WS_EX_TOPMOST.0)
                                    != 0;
                                let tray_above = !tray_hwnd.0.is_null()
                                    && is_tray_above_widget(hwnd, tray_hwnd);

                                if was_fullscreen || !is_topmost || tray_above {
                                    let _ = SetWindowPos(
                                        hwnd,
                                        HWND_TOPMOST,
                                        0,
                                        0,
                                        0,
                                        0,
                                        SWP_NOMOVE
                                            | SWP_NOSIZE
                                            | SWP_NOACTIVATE
                                            | SWP_NOSENDCHANGING,
                                    );
                                    was_fullscreen = false;
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
}


/// Returns system idle seconds (used for monitorsOffAfterIdle).
pub fn system_idle_seconds() -> u64 {
    unsafe {
        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut lii).as_bool() {
            let now = GetTickCount();
            let diff_ms = now.wrapping_sub(lii.dwTime);
            return (diff_ms / 1000) as u64;
        }
        0
    }
}

/// Clamps a taskbar offset to the range the strip can actually reach, so a stored value never
/// lies outside the taskbar (which would create a dead zone on the next drag).
pub fn clamp_taskbar_offset(offset: i32, compact_w: i32, scale_factor: f32, secondary: bool) -> i32 {
    let sf = scale_factor.max(1.0);
    let Some(tb) = query_taskbar_geometry(secondary) else {
        return offset;
    };
    let tray_left = tb.tray_left as f32 / sf;
    let min_anchor = tb.taskbar_left as f32 / sf + compact_w as f32 + 40.0;
    let max_anchor = tb.taskbar_right as f32 / sf - 20.0;
    let lo = (min_anchor - (tray_left - 8.0)).round() as i32;
    let hi = (max_anchor - (tray_left - 8.0)).round() as i32;
    offset.clamp(lo.min(0), hi.max(0))
}

/// Mouse position in physical screen pixels (independent of our own window moving).
pub fn cursor_x() -> Option<i32> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some(p.x)
}
