#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated, dead_code)]

#[macro_use]
mod i18n;
mod autostart;
mod claude;
mod daybook;
mod procs;
mod format;
mod power;
mod selftest;
mod sensors;
mod session;
mod taskbar;
mod theme;
mod ui;

use claude::{ClaudeStatus, ConvoTurn};
use format::format_de_price;
use i18n::t;
use power::{compute, ComputedReading, RawSensorSample, Settings};
use selftest::apply_demo_system_data;
use sensors::{HardwareSampler, SysMonView};
use session::{SessionManager, SessionSummary, Spike};
use taskbar::{
    compute_window_rect, enforce_taskbar_overlay_style, spawn_taskbar_zorder_guardian,
    system_idle_seconds,
};
use theme::ThemePalette;
use eframe::egui::{
    self, FontData, FontDefinitions, FontFamily, Key, Margin, Pos2, Rounding, Stroke, Vec2,
    ViewportCommand,
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WINDOW_TITLE: &str = "Strom-Widget";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Live,
    Spikes,
    Sessions,
    Net,
    System,
    Settings,
    Summary,
}

/// Period shown in the History tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistPeriod {
    Day,
    Week,
    Month,
    Year,
}

struct SharedSensorData {
    raw: Option<RawSensorSample>,
    sys: SysMonView,
    sys_addon_enabled: bool,
    /// The System tab is visible: scan ports often (otherwise about once a minute).
    ports_wanted: bool,
}

struct FrozenChart {
    points: Vec<(u64, f64)>,
    from: u64,
    to: u64,
    spikes: Vec<Spike>,
    label: String,
}

struct StromWidgetApp {
    settings: Settings,
    session_mgr: SessionManager,
    reading: ComputedReading,
    sys_view: SysMonView,
    sensor_shared: Arc<Mutex<SharedSensorData>>,

    // UI state
    expanded: bool,
    grows_up: bool,
    active_tab: ActiveTab,
    chart_range_s: u64, // 900 (15m), 3600 (1h), 0 (Session)
    open_spike_idx: Option<usize>,
    show_all_ports: bool,

    // History & network tabs
    hist_period: HistPeriod,
    sessions_shown: usize,
    /// Network tab: true = month view of the daily volume, false = last 7 days.
    net_month: bool,
    /// Result of the last CSV export (path or error) for the note under the list.
    export_note: Option<String>,

    // Summary state
    active_summary: Option<SessionSummary>,
    summary_title: String,
    summary_fresh: bool,
    frozen_chart: Option<FrozenChart>,
    shutdown_armed_until: Option<Instant>,
    shutdown_scheduled: bool,

    // Settings inputs
    price_input_buf: String,
    price_input_valid: bool,
    budget_buf: String,
    rest_w_buf: String,
    psu_w_buf: String,
    monitor_w_bufs: Vec<String>,

    // Claude state
    claude_bar_open: bool,
    claude_input: String,
    claude_status: Arc<Mutex<ClaudeStatus>>,
    claude_history: Arc<Mutex<Vec<ConvoTurn>>>,

    // Window positioning & timing
    last_sensor_tick: Instant,
    last_rect_sync: Instant,
    // Strip drag inside the taskbar: (cursor x at start in physical px, offset at start)
    strip_drag: Option<(i32, i32)>,
    last_applied_rect: (i32, i32, i32, i32),
    z_state: Arc<AtomicU8>,
    sessions_cache: Vec<SessionSummary>,

    // Optional automated visual verification mode (--self-test-screenshots <dir>)
    self_test_dir: Option<String>,
    self_test_step: u32,
    self_test_timer: Instant,
    self_test_waiting: bool,
    self_test_scroll: Option<f32>,
    // --demo-data: example ports/web UIs/processes instead of this machine's (for public screenshots)
    demo_data: bool,
}

impl StromWidgetApp {
    fn new(cc: &eframe::CreationContext<'_>, self_test_dir: Option<String>) -> Self {
        Self::configure_fonts(&cc.egui_ctx);

        let mut settings = Settings::load();
        settings.autostart = autostart::is_enabled();
        i18n::set_usd(settings.currency == "USD");
        let app_dir = Settings::app_data_dir();
        let mut session_mgr = SessionManager::new(app_dir);

        let init_raw = RawSensorSample {
            cpu_w: Some(38.0),
            util: 8.0,
            gpu_w: Some(32.0),
            gpu_util: Some(2.0),
            gpu_temp: Some(42.0),
            rapl_available: true,
            nv_available: true,
            top: Vec::new(),
            fg: None,
        };
        let reading = compute(&init_raw, &settings, true);
        session_mgr.add(&reading, settings.price_ct, (0.0, 0.0));

        let shared = Arc::new(Mutex::new(SharedSensorData {
            raw: Some(init_raw),
            sys: SysMonView::default(),
            sys_addon_enabled: settings.sys_addon,
            ports_wanted: false,
        }));

        let shared_bg = Arc::clone(&shared);
        let ctx_bg = cc.egui_ctx.clone();
        let _ = std::thread::Builder::new().name("sampler".into()).spawn(move || {
            let mut sampler = HardwareSampler::new();
            loop {
                let (sys_on, ports_wanted) = shared_bg
                    .lock()
                    .map(|s| (s.sys_addon_enabled, s.ports_wanted))
                    .unwrap_or((true, false));
                let raw = sampler.sample(sys_on, ports_wanted);
                let sys_snapshot = sampler.last_sys_view.clone();
                if let Ok(mut guard) = shared_bg.lock() {
                    guard.raw = Some(raw);
                    guard.sys = sys_snapshot;
                }
                ctx_bg.request_repaint();
                std::thread::sleep(Duration::from_millis(1800));
            }
        });

        let init_z_bits = (if settings.pinned { 1 } else { 0 })
            | (if settings.placement == "taskbar" { 2 } else { 0 })
            | (if settings.taskbar_display == "secondary" { 4 } else { 0 });
        let z_state = Arc::new(AtomicU8::new(init_z_bits));
        spawn_taskbar_zorder_guardian(WINDOW_TITLE, Arc::clone(&z_state));

        let price_input_buf = format_de_price(settings.price_ct);
        let budget_buf = if settings.monthly_budget > 0.0 {
            format_de_price(settings.monthly_budget)
        } else {
            String::new()
        };
        let rest_w_buf = format!("{:.0}", settings.rest_w);
        let psu_w_buf = format!("{:.0}", settings.psu_w);
        let monitor_w_bufs = settings
            .monitors
            .iter()
            .map(|m| format!("{:.0}", m.w))
            .collect();

        let sessions_cache = session_mgr.list_sessions(20);

        let mut active_summary = None;
        let mut summary_title = String::new();
        if let Some(rec) = session_mgr.recovered.clone() {
            active_summary = Some(rec);
            summary_title = t("Letzte Session (wiederhergestellt)", "Last session (restored)").to_string();
        }

        Self {
            settings,
            session_mgr,
            reading,
            sys_view: SysMonView::default(),
            sensor_shared: shared,
            expanded: false,
            grows_up: true,
            active_tab: ActiveTab::Live,
            chart_range_s: 900,
            open_spike_idx: None,
            show_all_ports: true,
            hist_period: HistPeriod::Month,
            sessions_shown: 20,
            net_month: true,
            export_note: None,
            active_summary,
            summary_title,
            summary_fresh: false,
            frozen_chart: None,
            shutdown_armed_until: None,
            shutdown_scheduled: false,
            price_input_buf,
            price_input_valid: true,
            budget_buf,
            rest_w_buf,
            psu_w_buf,
            monitor_w_bufs,
            claude_bar_open: false,
            claude_input: String::new(),
            claude_status: Arc::new(Mutex::new(ClaudeStatus::Idle)),
            claude_history: Arc::new(Mutex::new(Vec::new())),
            last_sensor_tick: Instant::now() - Duration::from_secs(5),
            last_rect_sync: Instant::now() - Duration::from_secs(5),
            strip_drag: None,
            last_applied_rect: (0, 0, 0, 0),
            z_state,
            sessions_cache,
            self_test_dir,
            self_test_step: 0,
            self_test_timer: Instant::now(),
            self_test_waiting: false,
            self_test_scroll: None,
            demo_data: false,
        }
    }

    fn configure_fonts(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();

        let font_candidates = [
            (
                "segoe_var",
                r"C:\Windows\Fonts\SegoeUIVar.ttf",
                FontFamily::Proportional,
            ),
            (
                "segoe_semi",
                r"C:\Windows\Fonts\seguisb.ttf",
                FontFamily::Proportional,
            ),
            (
                "consolas",
                r"C:\Windows\Fonts\consola.ttf",
                FontFamily::Monospace,
            ),
        ];

        for (name, path, family) in font_candidates {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert(name.to_string(), FontData::from_owned(bytes));
                if let Some(fam) = fonts.families.get_mut(&family) {
                    fam.insert(0, name.to_string());
                }
            }
        }

        ctx.set_fonts(fonts);
    }

    /// Changes the taskbar offset from the settings page (buttons), clamped to the taskbar.
    fn nudge_taskbar_offset(&mut self, ctx: &egui::Context, value: i32) {
        let scale = ctx.pixels_per_point().max(1.0);
        let (cw, _) = self.compact_dimensions();
        self.settings.taskbar_offset_x = taskbar::clamp_taskbar_offset(
            value,
            cw,
            scale,
            self.settings.taskbar_display == "secondary",
        );
        self.settings.save();
        self.sync_window_geometry(ctx, true);
    }

    fn compact_dimensions(&self) -> (i32, i32) {
        if self.settings.placement == "taskbar" {
            (self.settings.taskbar_strip_width(), 42)
        } else {
            let (base_w, base_h, sys_h) = if self.settings.size == "m" {
                (320, 68, 22)
            } else {
                (232, 48, 18)
            };
            let h = base_h + if self.settings.strip_shows_sys() { sys_h } else { 0 };
            (base_w, h)
        }
    }

    fn expanded_dimensions(&self) -> (i32, i32) {
        (456, 800)
    }

    fn sync_window_geometry(&mut self, ctx: &egui::Context, force: bool) {
        if !force && self.last_rect_sync.elapsed() < Duration::from_millis(450) {
            return;
        }
        self.last_rect_sync = Instant::now();

        let scale = ctx.pixels_per_point().max(1.0);
        let (cw, ch) = self.compact_dimensions();
        let (ew, eh) = self.expanded_dimensions();

        let (x, y, w, h, grows_up) = compute_window_rect(
            &self.settings.placement,
            self.expanded,
            cw,
            ch,
            ew,
            eh,
            self.settings.taskbar_offset_x,
            scale,
            self.settings.taskbar_display == "secondary",
        );
        self.grows_up = grows_up;

        let in_taskbar = self.settings.placement == "taskbar";
        let on_secondary = self.settings.taskbar_display == "secondary";
        let z_bits = (if self.settings.pinned { 1 } else { 0 })
            | (if in_taskbar { 2 } else { 0 })
            | (if on_secondary { 4 } else { 0 });
        self.z_state.store(z_bits, Ordering::Relaxed);

        let target_i = (
            (x * 10.0) as i32,
            (y * 10.0) as i32,
            (w * 10.0) as i32,
            (h * 10.0) as i32,
        );
        if force || target_i != self.last_applied_rect {
            self.last_applied_rect = target_i;
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(w, h)));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(x, y)));
        }

        enforce_taskbar_overlay_style(WINDOW_TITLE, self.settings.pinned, in_taskbar, on_secondary);
    }

    fn poll_sensors(&mut self) {
        if self.last_sensor_tick.elapsed() < Duration::from_millis(1750) {
            return;
        }
        self.last_sensor_tick = Instant::now();

        let maybe_raw = {
            if let Ok(mut guard) = self.sensor_shared.lock() {
                guard.sys_addon_enabled = self.settings.sys_addon;
                guard.ports_wanted = self.expanded && self.active_tab == ActiveTab::System;
                self.sys_view = guard.sys.clone();
                if self.demo_data {
                    apply_demo_system_data(&mut self.sys_view);
                }
                guard.raw.clone()
            } else {
                None
            }
        };

        if let Some(raw) = maybe_raw {
            let monitors_on = if self.settings.monitors_off_after_idle {
                system_idle_seconds() < 600
            } else {
                true
            };
            self.reading = compute(&raw, &self.settings, monitors_on);
            if self.demo_data {
                self.reading.fg = Some("VS Code".to_string());
            }
            // Network is only sampled with the system add-on; otherwise count nothing.
            let net = if self.settings.sys_addon {
                (self.sys_view.down, self.sys_view.up)
            } else {
                (0.0, 0.0)
            };
            self.session_mgr.add(&self.reading, self.settings.price_ct, net);
        }
    }

    fn toggle_expand(&mut self, ctx: &egui::Context, want: Option<bool>) {
        let next = want.unwrap_or(!self.expanded);
        if !next {
            self.session_mgr.mark_seen();
        }
        self.expanded = next;
        if self.expanded {
            self.sessions_cache = self.session_mgr.list_sessions(self.sessions_shown);
        }
        self.sync_window_geometry(ctx, true);
    }

    fn trigger_goodnight(&mut self, ctx: &egui::Context) {
        let dur_s =
            (self.session_mgr.s.last.saturating_sub(self.session_mgr.s.start)) as f64 / 1000.0;
        let summary = if dur_s >= 5.0 && self.session_mgr.s.wh > 0.0001 {
            let sum = self.session_mgr.finalize(t("Gute Nacht", "Good night"));
            self.settings.pending_summary_id = Some(sum.id.clone());
            self.settings.save();
            Some(sum)
        } else {
            None
        };
        self.session_mgr.reset();
        self.sessions_cache = self.session_mgr.list_sessions(self.sessions_shown);
        self.open_summary_view(summary, t("Session beendet", "Session ended"), true);
        if !self.expanded {
            self.toggle_expand(ctx, Some(true));
        }
    }
}

impl eframe::App for StromWidgetApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_sensors();
        self.sync_window_geometry(ctx, false);

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            let mut closed_claude = false;
            if let Ok(mut st) = self.claude_status.lock() {
                if !matches!(*st, ClaudeStatus::Idle) {
                    *st = ClaudeStatus::Idle;
                    closed_claude = true;
                }
            }
            if !closed_claude && self.expanded {
                self.toggle_expand(ctx, Some(false));
            }
        }

        let pal = ThemePalette::for_id(&self.settings.theme);
        let alpha = if self.expanded {
            1.0
        } else {
            (self.settings.opacity as f32 / 100.0).clamp(0.35, 1.0)
        };

        let outer_frame = egui::Frame::none()
            .fill(pal.ground.gamma_multiply(alpha))
            .stroke(Stroke::new(1.0_f32, pal.frame))
            .rounding(Rounding::same(if self.settings.placement == "taskbar" && !self.expanded {
                8.0
            } else {
                10.0
            }))
            .inner_margin(Margin::same(0.0));

        egui::CentralPanel::default()
            .frame(outer_frame)
            .show(ctx, |ui| {
                if !self.expanded {
                    self.render_compact_strip(ui, ctx, &pal);
                } else {
                    self.render_expanded_flyout(ui, ctx, &pal);
                }
            });

        self.handle_self_test(ctx);
    }
}

/// Only one widget per user session: two instances share the window title and would move
/// each other's windows. Returns false if another instance already runs.
fn claim_single_instance() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    match unsafe { CreateMutexW(None, true, w!(r"Local\StromWidgetSingleInstance")) } {
        // The handle is intentionally kept open until the process exits.
        Ok(_handle) => (unsafe { GetLastError() }) != ERROR_ALREADY_EXISTS,
        Err(_) => true,
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut self_test_dir: Option<String> = None;
    let mut demo_data = false;
    // --zoom 2: render everything at 2x (sharper screenshots)
    let mut zoom: Option<f32> = None;
    for i in 0..args.len() {
        if args[i] == "--self-test-screenshots" && i + 1 < args.len() {
            self_test_dir = Some(args[i + 1].clone());
        }
        if args[i] == "--demo-data" {
            demo_data = true;
        }
        if args[i] == "--zoom" && i + 1 < args.len() {
            zoom = args[i + 1].parse::<f32>().ok().map(|z| z.clamp(1.0, 4.0));
        }
    }
    // The screenshot self-test may run beside the real widget (it uses its own APPDATA).
    if self_test_dir.is_none() && !claim_single_instance() {
        return Ok(());
    }

    let mut settings = Settings::load();
    settings.autostart = autostart::is_enabled();
    i18n::set_en(settings.lang == "en");
    i18n::set_usd(settings.currency == "USD");
    let (cw, ch) = if settings.placement == "taskbar" {
        (settings.taskbar_strip_width() as f32, 42.0)
    } else {
        (232.0, 64.0)
    };
    let (ix, iy, iw, ih, _) = compute_window_rect(
        &settings.placement,
        false,
        cw as i32,
        ch as i32,
        456,
        800,
        settings.taskbar_offset_x,
        1.0,
        settings.taskbar_display == "secondary",
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_inner_size([iw, ih])
        .with_position([ix, iy]);
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon.png")) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        viewport = viewport.with_icon(egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        });
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            if let Some(z) = zoom {
                cc.egui_ctx.set_zoom_factor(z);
            }
            let mut app = StromWidgetApp::new(cc, self_test_dir);
            app.demo_data = demo_data;
            Ok(Box::new(app))
        }),
    )
}
