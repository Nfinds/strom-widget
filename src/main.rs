#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated, dead_code)]

#[macro_use]
mod i18n;
mod autostart;
mod claude;
mod power;
mod sensors;
mod session;
mod taskbar;
mod theme;

use i18n::t;
use claude::{ask_async, ClaudeStatus, ConvoTurn};
use eframe::egui::{
    self, Align, Color32, FontData, FontDefinitions, FontFamily, FontId, Key, Layout, Margin,
    Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2, ViewportCommand,
};
use power::{class_of, compute, parse_price, ComputedReading, RawSensorSample, Settings};
use sensors::{HardwareSampler, SysMonView};
use session::{compact_minute, now_ms, CompactMinute, SessionManager, SessionSummary, Spike};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use taskbar::{
    compute_window_rect, enforce_taskbar_overlay_style, spawn_taskbar_zorder_guardian,
    system_idle_seconds,
};
use theme::{
    class_bg_color, class_number_color, double_bezel_card, draw_chevron_icon, draw_class_arrow,
    draw_class_ring_badge, draw_close_icon, draw_diag_arrow_icon, draw_gear_icon,
    draw_gradient_bar, draw_moon_icon, draw_net_arrow, draw_pin_icon, draw_send_icon,
    draw_status_led, load_color, number_color, pct_color, ThemePalette, CLASSES, THEMES,
};

const WINDOW_TITLE: &str = "Strom-Widget";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Live,
    Spikes,
    Sessions,
    System,
    Settings,
    Summary,
}

struct SharedSensorData {
    raw: Option<RawSensorSample>,
    sys: SysMonView,
    sys_addon_enabled: bool,
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
    rest_w_buf: String,
    psu_w_buf: String,
    monitor_w_bufs: Vec<String>,

    // Claude state
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
        session_mgr.add(&reading, settings.price_ct);

        let shared = Arc::new(Mutex::new(SharedSensorData {
            raw: Some(init_raw),
            sys: SysMonView::default(),
            sys_addon_enabled: settings.sys_addon,
        }));

        let shared_bg = Arc::clone(&shared);
        let ctx_bg = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let mut sampler = HardwareSampler::new();
            loop {
                let sys_on = shared_bg
                    .lock()
                    .map(|s| s.sys_addon_enabled)
                    .unwrap_or(true);
                let raw = sampler.sample(sys_on);
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
        let rest_w_buf = format!("{:.0}", settings.rest_w);
        let psu_w_buf = format!("{:.0}", settings.psu_w);
        let monitor_w_bufs = settings
            .monitors
            .iter()
            .map(|m| format!("{:.0}", m.w))
            .collect();

        let sessions_cache = session_mgr.list_sessions(60);

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
            active_summary,
            summary_title,
            summary_fresh: false,
            frozen_chart: None,
            shutdown_armed_until: None,
            shutdown_scheduled: false,
            price_input_buf,
            price_input_valid: true,
            rest_w_buf,
            psu_w_buf,
            monitor_w_bufs,
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

    /// Moves the taskbar strip with the mouse. Uses the absolute cursor position, because the
    /// window itself moves while dragging (window-relative deltas would feed back and jitter).
    fn handle_strip_drag(&mut self, ctx: &egui::Context, resp: &egui::Response) {
        let scale = ctx.pixels_per_point().max(1.0);
        if resp.drag_started() {
            if let Some(cx) = taskbar::cursor_x() {
                self.strip_drag = Some((cx, self.settings.taskbar_offset_x));
            }
        }
        if let Some((start_x, start_ofs)) = self.strip_drag {
            if resp.dragged() {
                if let Some(cx) = taskbar::cursor_x() {
                    let delta_pts = ((cx - start_x) as f32 / scale).round() as i32;
                    let (cw, _) = self.compact_dimensions();
                    let next = taskbar::clamp_taskbar_offset(
                        start_ofs + delta_pts,
                        cw,
                        scale,
                        self.settings.taskbar_display == "secondary",
                    );
                    if next != self.settings.taskbar_offset_x {
                        self.settings.taskbar_offset_x = next;
                        self.sync_window_geometry(ctx, true);
                    }
                }
            }
            if resp.drag_stopped() {
                self.strip_drag = None;
                self.settings.save();
            }
        }
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
            // Snug width for Ring Badge + Watt + ct/h + Session + CPU/RAM + Up/Down + Chevron with zero dead space
            let w = if self.settings.sys_addon {
                if self.settings.size == "m" { 316 } else { 302 }
            } else if self.settings.size == "m" {
                206
            } else {
                192
            };
            let h = 42;
            (w, h)
        } else {
            let (base_w, base_h, sys_h) = if self.settings.size == "m" {
                (320, 68, 22)
            } else {
                (232, 48, 18)
            };
            let h = base_h + if self.settings.sys_addon { sys_h } else { 0 };
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
            self.session_mgr.add(&self.reading, self.settings.price_ct);
        }
    }

    fn toggle_expand(&mut self, ctx: &egui::Context, want: Option<bool>) {
        let next = want.unwrap_or(!self.expanded);
        if !next {
            self.session_mgr.mark_seen();
        }
        self.expanded = next;
        if self.expanded {
            self.sessions_cache = self.session_mgr.list_sessions(60);
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
        self.sessions_cache = self.session_mgr.list_sessions(60);
        self.open_summary_view(summary, t("Session beendet", "Session ended"), true);
        if !self.expanded {
            self.toggle_expand(ctx, Some(true));
        }
    }

    fn open_summary_view(&mut self, sum: Option<SessionSummary>, title: &str, fresh: bool) {
        self.summary_title = title.to_string();
        self.summary_fresh = fresh;
        if let Some(ref s) = sum {
            if let Some(detail) = self.session_mgr.load_session_detail(&s.id) {
                let pts: Vec<(u64, f64)> = detail
                    .minutes
                    .iter()
                    .map(|m| (m.t + 30_000, m.avg))
                    .collect();
                self.frozen_chart = Some(FrozenChart {
                    points: pts,
                    from: s.start,
                    to: s.end,
                    spikes: detail.spikes,
                    label: tr!(
                        "Session {}\u{2013}{}", "Session {}\u{2013}{}",
                        fmt_clock(s.start),
                        fmt_clock(s.end)
                    ),
                });
            }
        }
        self.active_summary = sum;
        self.active_tab = ActiveTab::Summary;
    }

    fn ask_claude(&mut self, question: &str, spike_idx: Option<usize>, label: &str) {
        let cur_sum = self.session_mgr.current_summary();
        let mins: Vec<CompactMinute> = self
            .session_mgr
            .s
            .minutes
            .iter()
            .rev()
            .take(180)
            .rev()
            .map(compact_minute)
            .collect();
        let spikes: Vec<&Spike> = self.session_mgr.s.spikes.iter().rev().take(25).collect();
        let asked_spike = spike_idx.and_then(|i| self.session_mgr.s.spikes.get(i));

        let ctx_val = serde_json::json!({
            "jetzt": {
                "wattGesamt": self.reading.watts.round(),
                "wattGemessen": self.reading.measured_w.round(),
                "wattGeschaetzt": self.reading.estimated_w.round(),
                "cpuW": self.reading.parts.cpu.w.round(),
                "gpuW": self.reading.parts.gpu.w.round(),
                "cpuAuslastung": self.reading.util,
                "gpuAuslastung": self.reading.gpu_util,
                "gpuTemp": self.reading.gpu_temp,
                "vordergrund": self.reading.fg,
                "topProzesse": self.reading.top,
            },
            "preisCtProKWh": self.settings.price_ct,
            "waehrung": self.settings.currency,
            "modus": self.settings.mode,
            "session": cur_sum,
            "minuten": mins,
            "spikes": spikes,
            "frageZuSpike": asked_spike,
            "system": if self.settings.sys_addon { Some(&self.sys_view) } else { None },
        });

        let hist = self
            .claude_history
            .lock()
            .map(|h| h.iter().rev().take(3).rev().cloned().collect())
            .unwrap_or_default();

        ask_async(
            question.to_string(),
            label.to_string(),
            ctx_val.to_string(),
            hist,
            Arc::clone(&self.claude_status),
            Arc::clone(&self.claude_history),
        );
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

impl StromWidgetApp {
    fn handle_self_test(&mut self, ctx: &egui::Context) {
        let Some(out_dir) = self.self_test_dir.clone() else {
            return;
        };
        const SHOTS: [&str; 7] = ["compact", "live", "spikes", "history", "settings", "settings-bottom", "system"];

        // A screenshot event arrived this frame: save it and move to the next view.
        let mut captured_img: Option<Arc<egui::ColorImage>> = None;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Screenshot { image, .. } = ev {
                    captured_img = Some(Arc::clone(image));
                }
            }
        });

        if let Some(img) = captured_img {
            let name = SHOTS.get(self.self_test_step as usize).copied().unwrap_or("extra");
            let full_path = std::path::Path::new(&out_dir).join(format!("{}.png", name));
            let raw_rgba: Vec<u8> = img
                .pixels
                .iter()
                .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
                .collect();
            let _ = image::save_buffer(
                &full_path,
                &raw_rgba,
                img.width() as u32,
                img.height() as u32,
                image::ColorType::Rgba8,
            );
            self.self_test_step += 1;
            self.self_test_timer = Instant::now();
            self.self_test_waiting = false;
            match self.self_test_step {
                1 => {
                    self.active_tab = ActiveTab::Live;
                    self.toggle_expand(ctx, Some(true));
                }
                2 => self.active_tab = ActiveTab::Spikes,
                3 => self.active_tab = ActiveTab::Sessions,
                4 => self.active_tab = ActiveTab::Settings,
                5 => self.self_test_scroll = Some(260.0),
                6 => {
                    self.self_test_scroll = Some(0.0);
                    self.active_tab = ActiveTab::System;
                }
                _ => {}
            }
        }

        if self.self_test_step as usize >= SHOTS.len() {
            std::process::exit(0);
        }
        // Telemetry and the port scanner need a moment before the first shot.
        let wait = if self.self_test_step == 0 { 2600 } else { 800 };
        if !self.self_test_waiting && self.self_test_timer.elapsed() >= Duration::from_millis(wait) {
            self.self_test_waiting = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot);
        }
        ctx.request_repaint();
    }
    // =========================================================================
    // COMPACT STRIP (Taskbar-Docked Overlay OR Desktop Corner Strip)
    // =========================================================================
    fn render_compact_strip(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &ThemePalette) {
        let avail = ui.available_rect_before_wrap();
        let in_taskbar = self.settings.placement == "taskbar";

        let ticker_h = if in_taskbar { 3.0 } else { 5.0 };
        let sys_h = if self.settings.sys_addon && !in_taskbar {
            16.0
        } else {
            0.0
        };
        let ticker_rect = Rect::from_min_max(
            Pos2::new(avail.left() + 6.0, avail.bottom() - ticker_h - sys_h - 1.5),
            Pos2::new(avail.right() - 6.0, avail.bottom() - sys_h - 1.5),
        );
        self.draw_mini_ticker(ui.painter(), ticker_rect);

        // Shared vertical centerline above the mini ticker bar
        let content_top = avail.top() + 1.0;
        let content_bot = ticker_rect.top() - 1.0;
        let cy = (content_top + content_bot) * 0.5;
        let row1_y = cy - 6.2;
        let row2_y = cy + 6.2;

        let watts = self.reading.watts;
        let cls = class_of(watts);
        let num_col = number_color(watts);

        let painter = ui.painter();
        let mut x = avail.left() + if in_taskbar { 9.0 } else { 11.0 };

        // 1. Leftmost: Circular Efficiency Ring Badge (A-G) in live EU Energy Label color
        let ring_r = if in_taskbar { 11.0 } else { 12.5 };
        let ring_center = Pos2::new(x + ring_r, cy);
        draw_class_ring_badge(
            painter,
            ring_center,
            ring_r,
            cls,
            if in_taskbar { 11.5 } else { 13.0 },
        );
        x += ring_r * 2.0 + 7.0;

        // 2. Live Watt Figure + "W" unit on the shared centerline
        let w_font_sz = if in_taskbar {
            19.5
        } else if self.settings.size == "m" {
            25.0
        } else {
            21.0
        };
        let w_str = format!("{:.0}", watts);
        let w_galley = painter.layout_no_wrap(w_str, FontId::proportional(w_font_sz), num_col);
        let w_w = w_galley.size().x;
        painter.galley(
            Pos2::new(x, cy - w_galley.size().y * 0.5),
            w_galley,
            num_col,
        );
        x += w_w + 2.5;

        let u_galley = painter.layout_no_wrap(
            "W".to_string(),
            FontId::proportional(if in_taskbar { 10.5 } else { 12.0 }),
            pal.ink_muted,
        );
        let u_w = u_galley.size().x;
        painter.galley(
            Pos2::new(x, cy - u_galley.size().y * 0.5 + 1.5),
            u_galley,
            pal.ink_muted,
        );
        x += u_w + 7.5;

        // Crisp 1px vertical hairline divider
        painter.line_segment(
            [Pos2::new(x, cy - 10.5), Pos2::new(x, cy + 10.5)],
            Stroke::new(1.0_f32, pal.rule_strong),
        );
        x += 7.5;

        // 3. Cost per hour + running session cost/duration on exact row1_y / row2_y
        let ct_per_h = (watts / 1000.0) * self.settings.price_ct;
        let cur_sess = self.session_mgr.current_summary();
        let ct_str = format!("{}/h", fmt_money_ct(ct_per_h));
        let sess_str = format!("{} \u{00b7} {}", fmt_euro(cur_sess.cost), fmt_dur(cur_sess.dur_s));

        let ct_galley = painter.layout_no_wrap(
            ct_str,
            FontId::proportional(if in_taskbar { 11.2 } else { 12.5 }),
            pal.ink,
        );
        let sess_galley = painter.layout_no_wrap(
            sess_str,
            FontId::proportional(if in_taskbar { 9.5 } else { 10.5 }),
            pal.ink_muted,
        );
        let col3_w = ct_galley.size().x.max(sess_galley.size().x);
        painter.galley(
            Pos2::new(x, row1_y - ct_galley.size().y * 0.5),
            ct_galley,
            pal.ink,
        );
        painter.galley(
            Pos2::new(x, row2_y - sess_galley.size().y * 0.5),
            sess_galley,
            pal.ink_muted,
        );
        x += col3_w + 8.0;

        // 4. In Taskbar Mode with System-Addon: Clean aligned CPU/RAM & Down/Up columns
        if in_taskbar && self.settings.sys_addon {
            painter.line_segment(
                [Pos2::new(x, cy - 10.5), Pos2::new(x, cy + 10.5)],
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            x += 7.5;

            // Sub-column A: CPU % & RAM %
            let cpu_lbl = painter.layout_no_wrap(
                "CPU".to_string(),
                FontId::proportional(9.5),
                pal.ink_muted,
            );
            let ram_lbl = painter.layout_no_wrap(
                "RAM".to_string(),
                FontId::proportional(9.5),
                pal.ink_muted,
            );
            let lbl_w = cpu_lbl.size().x.max(ram_lbl.size().x);

            painter.galley(
                Pos2::new(x, row1_y - cpu_lbl.size().y * 0.5),
                cpu_lbl,
                pal.ink_muted,
            );
            painter.galley(
                Pos2::new(x, row2_y - ram_lbl.size().y * 0.5),
                ram_lbl,
                pal.ink_muted,
            );

            let val_x = x + lbl_w + 4.0;
            let cpu_col = pct_color(self.sys_view.cpu);
            let ram_col = pct_color(self.sys_view.ram);
            let cpu_val = painter.layout_no_wrap(
                format!("{:.0}%", self.sys_view.cpu),
                FontId::proportional(10.2),
                cpu_col,
            );
            let ram_val = painter.layout_no_wrap(
                format!("{:.0}%", self.sys_view.ram),
                FontId::proportional(10.2),
                ram_col,
            );
            let val_w = cpu_val.size().x.max(ram_val.size().x).max(22.0);

            painter.galley(
                Pos2::new(val_x, row1_y - cpu_val.size().y * 0.5),
                cpu_val,
                cpu_col,
            );
            painter.galley(
                Pos2::new(val_x, row2_y - ram_val.size().y * 0.5),
                ram_val,
                ram_col,
            );
            x = val_x + val_w + 7.5;

            // Sub-column B: Down (↓) & Up (↑)
            draw_net_arrow(painter, Pos2::new(x + 3.0, row1_y), true, pal.action);
            draw_net_arrow(painter, Pos2::new(x + 3.0, row2_y), false, pal.ink_muted);

            let net_x = x + 9.0;
            let down_galley = painter.layout_no_wrap(
                fmt_rate(self.sys_view.down),
                FontId::proportional(10.2),
                pal.ink,
            );
            let up_galley = painter.layout_no_wrap(
                fmt_rate(self.sys_view.up),
                FontId::proportional(9.8),
                pal.ink_muted,
            );
            painter.galley(
                Pos2::new(net_x, row1_y - down_galley.size().y * 0.5),
                down_galley,
                pal.ink,
            );
            painter.galley(
                Pos2::new(net_x, row2_y - up_galley.size().y * 0.5),
                up_galley,
                pal.ink_muted,
            );
        }

        // 5. Rightmost Vector Chevron (snug at the right edge on the shared centerline cy)
        let chevron_col_w = 18.0;
        let chevron_rect = Rect::from_min_max(
            Pos2::new(avail.right() - chevron_col_w, content_top),
            Pos2::new(avail.right() - 3.0, content_bot),
        );
        let chev_resp = ui.interact(chevron_rect, egui::Id::new("strip_chevron"), Sense::click());
        let chev_col = if chev_resp.hovered() {
            pal.ink
        } else {
            pal.ink_muted
        };
        draw_chevron_icon(
            ui.painter(),
            Pos2::new(chevron_rect.center().x, cy),
            self.grows_up,
            chev_col,
        );

        // Desktop compact sysline (when not in taskbar)
        if !in_taskbar && self.settings.sys_addon {
            let sys_rect = Rect::from_min_max(
                Pos2::new(avail.left() + 10.0, avail.bottom() - sys_h),
                Pos2::new(avail.right() - 10.0, avail.bottom() - 2.0),
            );
            ui.allocate_ui_at_rect(sys_rect, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(
                        RichText::new(format!("CPU {:.0}%", self.sys_view.cpu))
                            .font(FontId::proportional(10.0))
                            .color(pct_color(self.sys_view.cpu)),
                    );
                    ui.label(
                        RichText::new(format!("RAM {:.0}%", self.sys_view.ram))
                            .font(FontId::proportional(10.0))
                            .color(pct_color(self.sys_view.ram)),
                    );
                    let (d_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 10.0), Sense::hover());
                    draw_net_arrow(ui.painter(), d_rect.center(), true, pal.ink_muted);
                    ui.label(
                        RichText::new(fmt_rate_short(self.sys_view.down))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                    let (u_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 10.0), Sense::hover());
                    draw_net_arrow(ui.painter(), u_rect.center(), false, pal.ink_muted);
                    ui.label(
                        RichText::new(fmt_rate_short(self.sys_view.up))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                });
            });
        }

        let sense = if in_taskbar { Sense::click_and_drag() } else { Sense::click() };
        // No hover tooltips here: egui draws them inside this 42 px window, covering the strip.
        let bg_interact = ui.interact(avail, egui::Id::new("strip_bg_click"), sense);
        if in_taskbar {
            self.handle_strip_drag(ctx, &bg_interact);
        }
        if chev_resp.clicked() || bg_interact.clicked() {
            self.toggle_expand(ctx, Some(true));
        }
    }

    // =========================================================================
    // EXPANDED FLYOUT PANEL (456 x 800 — Obsidian Linear Architecture)
    // =========================================================================
    fn render_expanded_flyout(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        egui::Frame::none()
            .inner_margin(Margin::symmetric(12.0, 10.0))
            .show(ui, |ui| {
                // 1. Double-Bezel Hero Card + 4-Tile Telemetry Micro-Grid
                self.render_expanded_header(ui, ctx, pal);
                ui.add_space(7.0);

                // 2. Interactive History & Spike Chart with Peak Badge & Segmented Switcher
                self.render_chart_section(ui, pal);
                ui.add_space(7.0);

                // 3. Unified Segmented Pill Navigation Dock
                self.render_navigation_tabs(ui, pal);
                ui.add_space(7.0);

                // 4. Scrollable Main Tab Content (leaves space at bottom for Claude bar + Footer)
                let claude_open =
                    !matches!(*self.claude_status.lock().unwrap(), ClaudeStatus::Idle);
                let bottom_reserve = if claude_open { 172.0 } else { 96.0 };
                let content_h = (ui.available_height() - bottom_reserve).max(200.0);

                let mut scroll = egui::ScrollArea::vertical();
                if let Some(y) = self.self_test_scroll {
                    scroll = scroll.vertical_scroll_offset(y);
                }
                scroll
                    .max_height(content_h)
                    .min_scrolled_height(content_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.active_tab {
                        ActiveTab::Live => self.render_tab_live(ui, pal),
                        ActiveTab::Spikes => self.render_tab_spikes(ui, pal),
                        ActiveTab::Sessions => self.render_tab_sessions(ui, pal),
                        ActiveTab::System => self.render_tab_system(ui, pal),
                        ActiveTab::Settings => self.render_tab_settings(ui, ctx, pal),
                        ActiveTab::Summary => self.render_tab_summary(ui, pal),
                    });

                ui.add_space(6.0);

                // 5. Claude Answer Drawer (when active)
                self.render_claude_answer_box(ui, pal);

                // 6. Claude Quick-Chips & Input Bar
                self.render_claude_prompt_bar(ui, pal);
                ui.add_space(6.0);

                // 7. Footer ("Gute Nacht" primary pill button + Beenden)
                self.render_footer(ui, ctx, pal);
            });
    }

    fn render_expanded_header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        double_bezel_card(ui, pal, 14.0, Vec2::new(12.0, 10.0), |ui| {
            let watts = self.reading.watts;
            let cls = class_of(watts);
            let num_col = number_color(watts);

            // Top Hero Row: Allocate a single 32px tall rect so Watt, W, Class Arrow, Divider, Cost/h, Pin, and Zuklappen share an exact vertical center
            let avail_w = ui.available_width();
            let (hero_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 32.0), Sense::hover());
            let cy = hero_rect.center().y;
            let mut x = hero_rect.left();

            // 1. Live Watt Figure + "W"
            let w_galley = ui.painter().layout_no_wrap(
                format!("{:.0}", watts),
                FontId::proportional(30.0),
                num_col,
            );
            let w_w = w_galley.size().x;
            ui.painter().galley(
                Pos2::new(x, cy - w_galley.size().y * 0.5),
                w_galley,
                num_col,
            );
            x += w_w + 4.0;

            let u_galley = ui.painter().layout_no_wrap(
                "W".to_string(),
                FontId::proportional(13.5),
                pal.ink_muted,
            );
            let u_w = u_galley.size().x;
            ui.painter().galley(
                Pos2::new(x, cy - u_galley.size().y * 0.5 + 3.0),
                u_galley,
                pal.ink_muted,
            );
            x += u_w + 8.0;

            // 2. EU Energy Label Pointed Class Arrow (A-G)
            let arrow_rect =
                Rect::from_min_size(Pos2::new(x, cy - 10.5), Vec2::new(30.0, 21.0));
            draw_class_arrow(
                ui.painter(),
                arrow_rect,
                7.5,
                class_bg_color(cls),
                cls,
                Color32::from_rgb(0x07, 0x09, 0x0C),
                12.0,
            );
            x += 30.0 + 10.0;

            // 3. Vertical Hairline Divider
            ui.painter().line_segment(
                [Pos2::new(x, cy - 12.0), Pos2::new(x, cy + 12.0)],
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            x += 10.0;

            // 4. Cost per hour + running session cost/duration
            let ct_per_h = (watts / 1000.0) * self.settings.price_ct;
            let cur_sess = self.session_mgr.current_summary();
            ui.painter().text(
                Pos2::new(x, cy - 6.8),
                egui::Align2::LEFT_CENTER,
                format!("{}/h", fmt_money_ct(ct_per_h)),
                FontId::proportional(13.8),
                pal.ink,
            );
            ui.painter().text(
                Pos2::new(x, cy + 7.2),
                egui::Align2::LEFT_CENTER,
                format!("{} in {}", fmt_euro(cur_sess.cost), fmt_dur(cur_sess.dur_s)),
                FontId::proportional(11.0),
                pal.ink_muted,
            );

            // 5. Right-aligned Pill Buttons: Pin + "✕ Zuklappen"
            let close_w = 88.0;
            let close_rect = Rect::from_min_size(
                Pos2::new(hero_rect.right() - close_w, cy - 13.0),
                Vec2::new(close_w, 26.0),
            );
            let close_resp = ui.interact(close_rect, egui::Id::new("hdr_close_btn"), Sense::click());
            ui.painter().rect(
                close_rect,
                Rounding::same(13.0),
                if close_resp.hovered() {
                    pal.rule
                } else {
                    pal.hover
                },
                Stroke::new(1.0_f32, pal.rule_strong),
            );
            draw_close_icon(
                ui.painter(),
                Pos2::new(close_rect.left() + 13.0, close_rect.center().y),
                6.5,
                pal.ink,
            );
            ui.painter().text(
                Pos2::new(close_rect.left() + 24.0, close_rect.center().y),
                egui::Align2::LEFT_CENTER,
                t("Zuklappen", "Collapse"),
                FontId::proportional(11.5),
                pal.ink,
            );
            if close_resp.clicked() {
                self.toggle_expand(ctx, Some(false));
            }

            let pin_w = 28.0;
            let pin_rect = Rect::from_min_size(
                Pos2::new(close_rect.left() - 6.0 - pin_w, cy - 13.0),
                Vec2::new(pin_w, 26.0),
            );
            let pin_resp = ui.interact(pin_rect, egui::Id::new("hdr_pin_btn"), Sense::click());
            let pin_bg = if self.settings.pinned {
                pal.action.gamma_multiply(0.18)
            } else if pin_resp.hovered() {
                pal.rule
            } else {
                pal.hover
            };
            let pin_stroke = if self.settings.pinned {
                pal.action
            } else {
                pal.rule_strong
            };
            ui.painter().rect(
                pin_rect,
                Rounding::same(13.0),
                pin_bg,
                Stroke::new(1.0_f32, pin_stroke),
            );
            draw_pin_icon(
                ui.painter(),
                pin_rect.center(),
                self.settings.pinned,
                if self.settings.pinned {
                    pal.action
                } else {
                    pal.ink_muted
                },
            );
            if pin_resp
                .on_hover_text(t("Immer im Vordergrund umschalten", "Toggle always on top"))
                .clicked()
            {
                self.settings.pinned = !self.settings.pinned;
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }

            // Obsidian Linear 4-Tile Telemetry Micro-Grid (CPU, RAM, DOWN, UP) — Direct Mathematical Painter Grid
            if self.settings.sys_addon {
                ui.add_space(9.0);
                let cpu_pct = self.sys_view.cpu;
                let ram_pct = self.sys_view.ram;
                let cpu_w = self.reading.parts.cpu.w;
                let down_txt = fmt_rate(self.sys_view.down);
                let up_txt = fmt_rate(self.sys_view.up);
                let down_share = ((self.sys_view.down / 5_242_880.0) as f32).clamp(0.06, 1.0);
                let up_share = ((self.sys_view.up / 2_097_152.0) as f32).clamp(0.06, 1.0);

                let grid_w = ui.available_width();
                let tile_gap = 6.0;
                let tile_w = (grid_w - tile_gap * 3.0) / 4.0;
                let tile_h = 48.0;
                let (grid_rect, _) =
                    ui.allocate_exact_size(Vec2::new(grid_w, tile_h), Sense::hover());

                let tiles = [
                    (
                        "CPU",
                        Some(format!("{:.0} W", cpu_w)),
                        pct_color(cpu_pct),
                        format!("{:.0} %", cpu_pct),
                        pal.ink,
                        (cpu_pct / 100.0) as f32,
                        pct_color(cpu_pct),
                        None,
                    ),
                    (
                        "RAM",
                        Some(t("Belegt", "Used").to_string()),
                        pal.ink_muted,
                        format!("{:.0} %", ram_pct),
                        pct_color(ram_pct),
                        (ram_pct / 100.0) as f32,
                        pct_color(ram_pct),
                        None,
                    ),
                    (
                        "DOWN",
                        None,
                        pal.action,
                        down_txt,
                        pal.ink,
                        down_share,
                        pal.action,
                        Some(true),
                    ),
                    (
                        "UP",
                        None,
                        pal.ink_muted,
                        up_txt,
                        pal.ink_muted,
                        up_share,
                        pal.ink_muted,
                        Some(false),
                    ),
                ];

                for (i, (lbl, right_sub, sub_col, main_val, val_col, bar_share, bar_col, arrow_down)) in
                    tiles.iter().enumerate()
                {
                    let tx = grid_rect.left() + (i as f32) * (tile_w + tile_gap);
                    let t_rect =
                        Rect::from_min_size(Pos2::new(tx, grid_rect.top()), Vec2::new(tile_w, tile_h));

                    ui.painter().rect(
                        t_rect,
                        Rounding::same(8.0),
                        pal.input_ground,
                        Stroke::new(1.0_f32, pal.rule),
                    );

                    // Top row inside tile: Label on left, sub-value or arrow on right
                    let top_y = t_rect.top() + 11.0;
                    ui.painter().text(
                        Pos2::new(t_rect.left() + 8.5, top_y),
                        egui::Align2::LEFT_CENTER,
                        *lbl,
                        FontId::proportional(9.5),
                        pal.ink_muted,
                    );

                    if let Some(ref sub_txt) = right_sub {
                        ui.painter().text(
                            Pos2::new(t_rect.right() - 8.5, top_y),
                            egui::Align2::RIGHT_CENTER,
                            sub_txt,
                            FontId::proportional(9.5),
                            *sub_col,
                        );
                    } else if let Some(is_down) = arrow_down {
                        draw_net_arrow(
                            ui.painter(),
                            Pos2::new(t_rect.right() - 11.5, top_y),
                            *is_down,
                            *sub_col,
                        );
                    }

                    // Middle row inside tile: Main Telemetry Value (clean proportional font, zero wide gaps)
                    let mid_y = t_rect.top() + 27.5;
                    ui.painter().text(
                        Pos2::new(t_rect.left() + 8.5, mid_y),
                        egui::Align2::LEFT_CENTER,
                        main_val,
                        FontId::proportional(13.0),
                        *val_col,
                    );

                    // Bottom row inside tile: Clean 3px Progress Bar
                    let bar_rect = Rect::from_min_max(
                        Pos2::new(t_rect.left() + 8.5, t_rect.bottom() - 8.5),
                        Pos2::new(t_rect.right() - 8.5, t_rect.bottom() - 5.5),
                    );
                    draw_meter_bar(ui.painter(), bar_rect, *bar_share, *bar_col, pal.rule);
                }
            }
        });
    }

    fn render_chart_section(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        double_bezel_card(ui, pal, 12.0, Vec2::new(11.0, 8.0), |ui| {
            // Allocate a single 24px tall header strip so Title, Peak Badge, and Time Range Switcher share an exact centerline
            let hdr_w = ui.available_width();
            let (hdr_rect, _) = ui.allocate_exact_size(Vec2::new(hdr_w, 24.0), Sense::hover());
            let cy = hdr_rect.center().y;

            let title = if let Some(ref fz) = self.frozen_chart {
                fz.label.clone()
            } else {
                match self.chart_range_s {
                    900 => t("Leistung \u{00b7} 15 min", "Power \u{00b7} 15 min").to_string(),
                    3600 => t("Leistung \u{00b7} 1 Stunde", "Power \u{00b7} 1 hour").to_string(),
                    _ => t("Leistung \u{00b7} Session", "Power \u{00b7} session").to_string(),
                }
            };
            let title_galley = ui.painter().layout_no_wrap(
                title,
                FontId::proportional(11.5),
                pal.ink_muted,
            );
            let title_w = title_galley.size().x;
            ui.painter().galley(
                Pos2::new(hdr_rect.left(), cy - title_galley.size().y * 0.5),
                title_galley,
                pal.ink_muted,
            );

            // Obsidian Linear Peak Wattage Callout Pill ("Spitze XXX W") aligned to cy
            let peak_w = self.session_mgr.s.peak.w;
            if peak_w > 1.0 {
                let p_col = number_color(peak_w);
                let p_txt = tr!("Spitze {:.0} W", "Peak {:.0} W", peak_w);
                let p_galley = ui.painter().layout_no_wrap(
                    p_txt,
                    FontId::proportional(10.2),
                    p_col,
                );
                let pill_w = p_galley.size().x + 14.0;
                let pill_h = 19.0;
                let pill_rect = Rect::from_min_size(
                    Pos2::new(hdr_rect.left() + title_w + 8.0, cy - pill_h * 0.5),
                    Vec2::new(pill_w, pill_h),
                );
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(9.5),
                    p_col.gamma_multiply(0.12),
                    Stroke::new(1.0_f32, p_col.gamma_multiply(0.32)),
                );
                ui.painter().galley(
                    Pos2::new(
                        pill_rect.center().x - p_galley.size().x * 0.5,
                        cy - p_galley.size().y * 0.5,
                    ),
                    p_galley,
                    p_col,
                );
            }

            // Recessed Segmented Switcher ("15m | 1 h | Session") aligned to cy on the right
            let seg_items: [(u64, &str, f32); 3] = [
                (900, "15m", 34.0),
                (3600, "1 h", 30.0),
                (0, "Session", 50.0),
            ];
            let total_seg_w: f32 = seg_items.iter().map(|x| x.2).sum::<f32>() + 4.0 + 4.0;
            let seg_h = 22.0;
            let seg_outer = Rect::from_min_size(
                Pos2::new(hdr_rect.right() - total_seg_w, cy - seg_h * 0.5),
                Vec2::new(total_seg_w, seg_h),
            );
            ui.painter().rect(
                seg_outer,
                Rounding::same(7.0),
                pal.input_ground,
                Stroke::new(1.0_f32, pal.rule),
            );

            let mut sx = seg_outer.left() + 2.0;
            for (secs, label, bw) in seg_items {
                let active = self.frozen_chart.is_none() && self.chart_range_s == secs;
                let btn_rect = Rect::from_min_size(
                    Pos2::new(sx, cy - (seg_h - 4.0) * 0.5),
                    Vec2::new(bw, seg_h - 4.0),
                );
                let resp = ui.interact(
                    btn_rect,
                    egui::Id::new(("chart_seg", secs)),
                    Sense::click(),
                );
                if active || resp.hovered() {
                    ui.painter().rect(
                        btn_rect,
                        Rounding::same(5.0),
                        if active { pal.hover } else { pal.card },
                        if active {
                            Stroke::new(1.0_f32, pal.rule_strong)
                        } else {
                            Stroke::NONE
                        },
                    );
                }
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(10.5),
                    if active { pal.ink } else { pal.ink_muted },
                );
                if resp.clicked() {
                    self.frozen_chart = None;
                    self.chart_range_s = secs;
                }
                sx += bw + 2.0;
            }

            ui.add_space(5.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 78.0), Sense::hover());
            self.draw_history_chart(ui.painter(), rect, pal);
        });
    }

    fn render_navigation_tabs(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // Unified Segmented Pill Navigation Dock — Direct Painter Layout so all tabs, badges, and gear share an exact centerline
        let dock_w = ui.available_width();
        let dock_h = 32.0;
        let (dock_rect, _) = ui.allocate_exact_size(Vec2::new(dock_w, dock_h), Sense::hover());
        let cy = dock_rect.center().y;

        ui.painter().rect(
            dock_rect,
            Rounding::same(10.0),
            pal.input_ground,
            Stroke::new(1.0_f32, pal.rule),
        );

        let spike_count = self.session_mgr.s.spikes.len();
        let port_count = self.sys_view.all_ports.len();

        let mut tabs: Vec<(ActiveTab, &str, Option<usize>)> = vec![
            (ActiveTab::Live, "Live", None),
            (
                ActiveTab::Spikes,
                "Spikes",
                if spike_count > 0 {
                    Some(spike_count)
                } else {
                    None
                },
            ),
            (ActiveTab::Sessions, t("Verlauf", "History"), None),
        ];
        if self.settings.sys_addon {
            tabs.push((
                ActiveTab::System,
                t("System & Ports", "System & Ports"),
                if port_count > 0 {
                    Some(port_count)
                } else {
                    None
                },
            ));
        }

        let pill_h = 26.0;
        let mut tx = dock_rect.left() + 3.0;

        for (tab_id, title, badge_cnt) in tabs {
            let is_on = self.active_tab == tab_id;
            let title_galley = ui.painter().layout_no_wrap(
                title.to_string(),
                FontId::proportional(11.5),
                if is_on { pal.ink } else { pal.ink_muted },
            );
            let title_w = title_galley.size().x;

            let badge_info = badge_cnt.map(|cnt| {
                let b_galley = ui.painter().layout_no_wrap(
                    format!("{}", cnt),
                    FontId::proportional(9.8),
                    pal.action,
                );
                let bw = (b_galley.size().x + 10.0).max(18.0);
                (b_galley, bw)
            });

            let badge_extra = badge_info.as_ref().map(|(_, bw)| 5.0 + *bw).unwrap_or(0.0);
            let pill_w = 18.0 + title_w + badge_extra;
            let pill_rect = Rect::from_min_size(
                Pos2::new(tx, cy - pill_h * 0.5),
                Vec2::new(pill_w, pill_h),
            );
            let resp = ui.interact(
                pill_rect,
                egui::Id::new(("nav_tab", title)),
                Sense::click(),
            );

            if is_on || resp.hovered() {
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(7.0),
                    if is_on { pal.hover } else { pal.card },
                    if is_on {
                        Stroke::new(1.0_f32, pal.rule_strong)
                    } else {
                        Stroke::NONE
                    },
                );
            }

            let text_x = pill_rect.left() + 9.0;
            ui.painter().galley(
                Pos2::new(text_x, cy - title_galley.size().y * 0.5),
                title_galley,
                if is_on { pal.ink } else { pal.ink_muted },
            );

            if let Some((b_galley, bw)) = badge_info {
                let bh = 16.0;
                let b_rect = Rect::from_min_size(
                    Pos2::new(text_x + title_w + 5.0, cy - bh * 0.5),
                    Vec2::new(bw, bh),
                );
                ui.painter().rect_filled(
                    b_rect,
                    Rounding::same(8.0),
                    pal.action.gamma_multiply(0.18),
                );
                ui.painter().galley(
                    Pos2::new(
                        b_rect.center().x - b_galley.size().x * 0.5,
                        cy - b_galley.size().y * 0.5,
                    ),
                    b_galley,
                    pal.action,
                );
            }

            if resp.clicked() {
                if self.active_tab == ActiveTab::Summary && tab_id != ActiveTab::Summary {
                    self.frozen_chart = None;
                    self.summary_fresh = false;
                }
                self.active_tab = tab_id;
                if tab_id == ActiveTab::Spikes {
                    self.session_mgr.mark_seen();
                }
                if tab_id == ActiveTab::Sessions {
                    self.sessions_cache = self.session_mgr.list_sessions(60);
                }
            }

            tx += pill_w + 3.0;
        }

        // Right-aligned Vector Gear Button inside the Segmented Dock on the exact same cy
        let gear_on = self.active_tab == ActiveTab::Settings;
        let gear_w = 28.0;
        let gear_rect = Rect::from_min_size(
            Pos2::new(dock_rect.right() - 3.0 - gear_w, cy - pill_h * 0.5),
            Vec2::new(gear_w, pill_h),
        );
        let gear_resp = ui.interact(gear_rect, egui::Id::new("nav_gear_btn"), Sense::click());
        if gear_on || gear_resp.hovered() {
            ui.painter().rect(
                gear_rect,
                Rounding::same(7.0),
                if gear_on { pal.hover } else { pal.card },
                if gear_on {
                    Stroke::new(1.0_f32, pal.rule_strong)
                } else {
                    Stroke::NONE
                },
            );
        }
        draw_gear_icon(
            ui.painter(),
            gear_rect.center(),
            if gear_on { pal.action } else { pal.ink_muted },
        );
        if gear_resp.on_hover_text(t("Einstellungen", "Settings")).clicked() {
            self.active_tab = ActiveTab::Settings;
        }
    }

    // =========================================================================
    // TAB 1: LIVE (EU Energy Label A-G Scale + Parts Breakdown + Fact Cards)
    // =========================================================================
    fn render_tab_live(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        // 1. Signature EU Energy Label A-G Scale with Pointer
        double_bezel_card(ui, pal, 10.0, Vec2::new(10.0, 8.0), |ui| {
            let cur_cls = class_of(self.reading.watts);
            let avail_w = ui.available_width();
            let gap = 4.0;
            let arrow_w = (avail_w - gap * 6.0) / 7.0;
            let row_h = 26.0;

            let (scale_rect, _) =
                ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::hover());

            for (i, &(_, k)) in CLASSES.iter().enumerate() {
                let is_cur = k == cur_cls;
                let h = if is_cur { 25.0 } else { 18.0 };
                let y0 = scale_rect.top() + (row_h - h) * 0.5;
                let x0 = scale_rect.left() + (i as f32) * (arrow_w + gap);
                let r = Rect::from_min_size(Pos2::new(x0, y0), Vec2::new(arrow_w, h));
                draw_class_arrow(
                    ui.painter(),
                    r,
                    7.0,
                    class_bg_color(k),
                    k,
                    Color32::from_rgb(0x0B, 0x0C, 0x0E),
                    if is_cur { 13.5 } else { 11.0 },
                );
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Draw small vector right triangle pointer
                let (p_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 12.0), Sense::hover());
                let c = p_rect.center();
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        Pos2::new(c.x - 3.0, c.y - 4.0),
                        Pos2::new(c.x + 3.0, c.y),
                        Pos2::new(c.x - 3.0, c.y + 4.0),
                    ],
                    class_number_color(cur_cls),
                    Stroke::NONE,
                ));

                ui.label(
                    RichText::new(tr!("Klasse {}", "Class {}", cur_cls))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(class_number_color(cur_cls)),
                );
                ui.label(
                    RichText::new(tr!("jetzt {:.0} W", "now {:.0} W", self.reading.watts))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(t("A <150 W \u{00b7} +100 W/Klasse \u{00b7} G \u{2265}650 W", "A <150 W \u{00b7} +100 W/class \u{00b7} G \u{2265}650 W"))
                            .font(FontId::proportional(10.0))
                            .color(pal.ink_muted),
                    );
                });
            });
        });

        ui.add_space(7.0);

        // 2. Parts Breakdown Table (CPU, GPU, Restsystem, Netzteilverlust, Monitore, Steckdose)
        double_bezel_card(ui, pal, 10.0, Vec2::new(11.0, 8.0), |ui| {
            let total_w = self.reading.watts.max(1.0);
            let rows = [
                (
                    t("Prozessor (CPU)", "Processor (CPU)"),
                    self.reading.parts.cpu.w,
                    self.reading.parts.cpu.src.as_str(),
                    false,
                ),
                (
                    t("Grafikkarte (GPU)", "Graphics card (GPU)"),
                    self.reading.parts.gpu.w,
                    self.reading.parts.gpu.src.as_str(),
                    false,
                ),
                (
                    t("Restsystem", "Rest of system"),
                    self.reading.parts.rest.w,
                    self.reading.parts.rest.src.as_str(),
                    false,
                ),
                (
                    t("Netzteilverlust", "PSU loss"),
                    self.reading.parts.psu.w,
                    self.reading.parts.psu.src.as_str(),
                    false,
                ),
                (
                    t("Monitore", "Monitors"),
                    self.reading.parts.mon.w,
                    self.reading.parts.mon.src.as_str(),
                    false,
                ),
                (
                    t("Steckdose (Gesamt)", "Wall socket (total)"),
                    self.reading.watts,
                    if self.settings.mode == "measured" {
                        "Summe"
                    } else {
                        t("Sch\u{00e4}tzung", "Estimate")
                    },
                    true,
                ),
            ];

            for (label, w_val, src, is_total) in rows {
                if is_total {
                    ui.add_space(2.0);
                    ui.separator();
                    ui.add_space(2.0);
                }
                ui.horizontal(|ui| {
                    ui.allocate_ui(Vec2::new(132.0, 17.0), |ui| {
                        ui.label(
                            RichText::new(label)
                                .font(FontId::proportional(11.5))
                                .color(if is_total { pal.ink } else { pal.ink_muted })
                                .strong(),
                        );
                    });
                    ui.allocate_ui(Vec2::new(54.0, 17.0), |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0} W", w_val))
                                    .font(FontId::proportional(12.0))
                                    .strong()
                                    .color(if is_total {
                                        number_color(w_val)
                                    } else {
                                        pal.ink
                                    }),
                            );
                        });
                    });
                    ui.allocate_ui(Vec2::new(66.0, 17.0), |ui| {
                        let src_full = match src {
                            "gem." => t("gemessen", "measured"),
                            "aus" => t("aus", "off"),
                            "Summe" => t("Summe", "Sum"),
                            _ => t("gesch\u{00e4}tzt", "estimated"),
                        };
                        ui.label(
                            RichText::new(src_full)
                                .font(FontId::proportional(10.5))
                                .color(if src == "gem." {
                                    pal.action
                                } else {
                                    pal.ink_muted
                                }),
                        );
                    });

                    let bar_w = ui.available_width().max(40.0);
                    let (bar_rect, _) =
                        ui.allocate_exact_size(Vec2::new(bar_w, 7.5), Sense::hover());
                    let share = (w_val / total_w).clamp(0.0, 1.0) as f32;
                    let fill_col = if is_total {
                        load_color(w_val)
                    } else {
                        pal.action
                    };
                    draw_meter_bar(ui.painter(), bar_rect, share, fill_col, pal.rule);
                });
            }

            ui.add_space(5.0);
            let diff_pct = if self.reading.measured_w > 0.0 {
                ((self.reading.estimated_w - self.reading.measured_w) / self.reading.measured_w)
                    * 100.0
            } else {
                0.0
            };
            let sign = if diff_pct >= 0.0 { "+" } else { "\u{2212}" };
            let gpu_str = self
                .reading
                .gpu_util
                .map(|u| format!("{:.0} %", u))
                .unwrap_or_else(|| "\u{2013}".to_string());
            let temp_str = self
                .reading
                .gpu_temp
                .map(|t| format!(" \u{00b7} {:.0} \u{00b0}C", t))
                .unwrap_or_default();
            let fg_str = self
                .reading
                .fg
                .as_ref()
                .map(|f| tr!(" \u{00b7} Fokus: {}", " \u{00b7} Focus: {}", f))
                .unwrap_or_default();

            ui.label(
                RichText::new(tr!(
                    "Gemessen {:.0} W \u{00b7} Gesch\u{00e4}tzt {:.0} W \u{00b7} Abweichung {}{:.0} %\nCPU {:.0} % \u{00b7} GPU {}{}{}", "Measured {:.0} W \u{00b7} Estimated {:.0} W \u{00b7} Deviation {}{:.0} %\nCPU {:.0} % \u{00b7} GPU {}{}{}",
                    self.reading.measured_w,
                    self.reading.estimated_w,
                    sign,
                    diff_pct.abs(),
                    self.reading.util,
                    gpu_str,
                    temp_str,
                    fg_str
                ))
                .font(FontId::proportional(11.0))
                .color(pal.ink_muted),
            );
        });

        ui.add_space(7.0);

        // 3. 6 Fact Cards in a clean 2-row x 3-column Equal-Width Layout
        let s = self.session_mgr.current_summary();
        let facts = [
            (t("Seit", "Since"), fmt_clock(s.start)),
            (t("Dauer", "Duration"), fmt_dur(s.dur_s)),
            (t("Kosten", "Cost"), fmt_euro(s.cost)),
            (t("Energie", "Energy"), format!("{:.2} kWh", s.wh / 1000.0).replace('.', crate::i18n::dec())),
            (t("Durchschnitt", "Average"), format!("{:.0} W", s.avg_w)),
            (t("Spitze", "Peak"), format!("{:.0} W", s.peak.w)),
        ];

        for row_pair in facts.chunks(3) {
            ui.columns(3, |cols| {
                for (c_idx, (k, v)) in row_pair.iter().enumerate() {
                    egui::Frame::none()
                        .fill(pal.card)
                        .stroke(Stroke::new(1.0_f32, pal.rule))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::symmetric(10.0, 6.0))
                        .show(&mut cols[c_idx], |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(*k)
                                    .font(FontId::proportional(10.5))
                                    .color(pal.ink_muted),
                            );
                            ui.label(
                                RichText::new(v)
                                    .font(FontId::proportional(14.0))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                }
            });
            ui.add_space(5.0);
        }

        ui.label(
            RichText::new(tr!(
                "Nativ in Rust gemessen: CPU-Paket (RAPL PDH), Grafikkarte (NVML). Wirkungsgrad {:.0} % \u{00b7} Tarif {:.1} {}/kWh.", "Measured natively in Rust: CPU package (RAPL PDH), graphics card (NVML). Efficiency {:.0} % \u{00b7} Tariff {:.1} {}/kWh.",
                self.reading.eff * 100.0,
                self.settings.price_ct,
                i18n::cent_unit()
            ))
            .font(FontId::proportional(10.5))
            .color(pal.ink_muted),
        );
    }

    // =========================================================================
    // TAB 2: SPIKES
    // =========================================================================
    fn render_tab_spikes(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        if self.session_mgr.s.spikes.is_empty() {
            double_bezel_card(ui, pal, 10.0, Vec2::new(12.0, 12.0), |ui| {
                ui.label(
                    RichText::new(
                        t("Noch keine Spikes in dieser Session.\nAls Spike z\u{00e4}hlt, wenn die Leistung mindestens 60 W und 35 % \u{00fc}ber der Grundlast der letzten zwei Minuten liegt.", "No spikes in this session yet.\nA spike is counted when the power is at least 60 W and 35 % above the base load of the last two minutes."),
                    )
                    .font(FontId::proportional(11.5))
                    .color(pal.ink_muted),
                );
            });
            return;
        }

        let spikes_snapshot: Vec<(usize, Spike)> = self
            .session_mgr
            .s
            .spikes
            .iter()
            .cloned()
            .enumerate()
            .rev()
            .collect();

        let mut ask_spike_req: Option<(usize, String)> = None;

        for (idx, sp) in spikes_snapshot {
            let is_open = self.open_spike_idx == Some(idx);
            let secs = ((sp.end.saturating_sub(sp.start)) as f64 / 1000.0).max(2.0);
            let dur_txt = if secs < 60.0 {
                format!("{:.0} s", secs)
            } else {
                fmt_dur(secs)
            };

            double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
                let resp = ui
                    .horizontal(|ui| {
                        let cls = class_of(sp.peak);
                        let (badge_rect, _) =
                            ui.allocate_exact_size(Vec2::new(22.0, 15.0), Sense::hover());
                        draw_class_arrow(
                            ui.painter(),
                            badge_rect,
                            5.5,
                            class_bg_color(cls),
                            cls,
                            Color32::from_rgb(0x0B, 0x0C, 0x0E),
                            9.5,
                        );

                        ui.label(
                            RichText::new(fmt_clock(sp.start))
                                .font(FontId::proportional(12.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(tr!("bis {:.0} W \u{00b7} {}", "up to {:.0} W \u{00b7} {}", sp.peak, dur_txt))
                                .font(FontId::proportional(11.5))
                                .color(pal.ink_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("+{}", fmt_money_ct(sp.extra_ct)))
                                    .font(FontId::proportional(12.5))
                                    .strong()
                                    .color(number_color(sp.peak)),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click());

                ui.label(
                    RichText::new(sp.cause_line())
                        .font(FontId::proportional(11.0))
                        .color(pal.ink_muted),
                );

                if resp.clicked() {
                    self.open_spike_idx = if is_open { None } else { Some(idx) };
                }

                if is_open {
                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);
                    let procs_txt = if sp.procs.is_empty() {
                        String::new()
                    } else {
                        tr!(
                            "\nProzesse: {}", "\nProcesses: {}",
                            sp.procs
                                .iter()
                                .map(|p| format!("{} {:.0} %", p.n, p.c))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let fg_txt = sp
                        .fg
                        .as_ref()
                        .map(|f| tr!("\nIm Vordergrund: {}", "\nIn foreground: {}", f))
                        .unwrap_or_default();

                    ui.label(
                        RichText::new(tr!(
                            "Grundlast vorher {:.0} W, Spitze {:.0} W um {}. Mehrverbrauch {:.1} Wh.\nProzessor +{:.0} W, Grafikkarte +{:.0} W.{}{}", "Base load before {:.0} W, peak {:.0} W at {}. Extra use {:.1} Wh.\nProcessor +{:.0} W, graphics card +{:.0} W.{}{}",
                            sp.base,
                            sp.peak,
                            fmt_clock(sp.peak_t),
                            sp.extra_wh,
                            sp.cpu_rise,
                            sp.gpu_rise,
                            procs_txt,
                            fg_txt
                        ))
                        .font(FontId::proportional(11.0))
                        .color(pal.ink),
                    );

                    ui.add_space(4.0);
                    let ask_btn = egui::Button::new(
                        RichText::new(t("Claude zu diesem Spike fragen", "Ask Claude about this spike"))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink),
                    )
                    .fill(pal.hover)
                    .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                    .rounding(Rounding::same(5.0));
                    if ui.add(ask_btn).clicked() {
                        ask_spike_req = Some((idx, tr!("Spike um {}", "Spike at {}", fmt_clock(sp.start))));
                    }
                }
            });
            ui.add_space(5.0);
        }

        if let Some((idx, lbl)) = ask_spike_req {
            self.ask_claude(
                t("Was hat diesen Spike verursacht, war er normal, und was hat er gekostet?", "What caused this spike, was it normal, and what did it cost?"),
                Some(idx),
                &lbl,
            );
        }
    }

    // =========================================================================
    // TAB 3: SESSIONS (Verlauf)
    // =========================================================================
    fn render_tab_sessions(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let cur = self.session_mgr.current_summary();
        let total_ct: f64 = self.sessions_cache.iter().map(|s| s.cost).sum::<f64>() + cur.cost;
        let total_wh: f64 = self.sessions_cache.iter().map(|s| s.wh).sum::<f64>() + cur.wh;

        double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
            ui.label(
                RichText::new(tr!(
                    "Gesamtverlauf: {} \u{00b7} {:.2} kWh \u{00b7} {} Sessions", "Total history: {} \u{00b7} {:.2} kWh \u{00b7} {} session(s)",
                    fmt_euro(total_ct),
                    total_wh / 1000.0,
                    self.sessions_cache.len() + 1
                ))
                .font(FontId::proportional(12.0))
                .strong()
                .color(pal.ink),
            );
        });

        ui.add_space(6.0);

        if self.sessions_cache.is_empty() {
            ui.label(
                RichText::new(
                    t("Noch keine abgeschlossene Session. Die erste entsteht mit \"Gute Nacht\" oder beim Herunterfahren.", "No completed session yet. The first one is created with \"Good night\" or when Windows shuts down."),
                )
                .font(FontId::proportional(11.5))
                .color(pal.ink_muted),
            );
            return;
        }

        let mut clicked_summary: Option<SessionSummary> = None;
        for s in &self.sessions_cache {
            double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
                let r = ui
                    .horizontal(|ui| {
                        let cls = class_of(s.avg_w);
                        let (b_rect, _) =
                            ui.allocate_exact_size(Vec2::new(22.0, 15.0), Sense::hover());
                        draw_class_arrow(
                            ui.painter(),
                            b_rect,
                            5.5,
                            class_bg_color(cls),
                            cls,
                            Color32::from_rgb(0x0B, 0x0C, 0x0E),
                            9.5,
                        );

                        ui.label(
                            RichText::new(fmt_day(s.start))
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{}\u{2013}{} \u{00b7} {} \u{00b7} {:.2} kWh",
                                fmt_clock(s.start),
                                fmt_clock(s.end),
                                fmt_dur(s.dur_s),
                                s.wh / 1000.0
                            ))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink_muted),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(fmt_euro(s.cost))
                                    .font(FontId::proportional(12.5))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                    })
                    .response
                    .interact(Sense::click());

                ui.label(
                    RichText::new(tr!(
                        "{} \u{00b7} \u{00d8} {:.0} W \u{00b7} {} Spikes", "{} \u{00b7} avg {:.0} W \u{00b7} {} spikes",
                        s.reason, s.avg_w, s.spikes
                    ))
                    .font(FontId::proportional(10.5))
                    .color(pal.ink_muted),
                );

                if r.clicked() {
                    clicked_summary = Some(s.clone());
                }
            });
            ui.add_space(4.0);
        }

        if let Some(sum) = clicked_summary {
            self.open_summary_view(Some(sum), "Session", false);
        }
    }

    // =========================================================================
    // TAB 4: SYSTEM & PORTS (Web UIs, All Listening TCP Ports + PID, Top RAM)
    // =========================================================================
    fn render_tab_system(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t("Erkannte Web-Oberfl\u{00e4}chen (HTTP/HTTPS)", "Detected web interfaces (HTTP/HTTPS)"))
                    .font(FontId::proportional(11.5))
                    .strong()
                    .color(pal.ink_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(if self.sys_view.web.is_empty() {
                        t("keine aktiv", "none active").to_string()
                    } else {
                        tr!("{} erreichbar", "{} reachable", self.sys_view.web.len())
                    })
                    .font(FontId::proportional(11.0))
                    .color(pal.action),
                );
            });
        });
        ui.add_space(4.0);

        if self.sys_view.web.is_empty() {
            double_bezel_card(ui, pal, 10.0, Vec2::new(10.0, 8.0), |ui| {
                ui.label(
                    RichText::new(
                        t("Gerade antwortet kein lokaler Port mit einer Web-Oberfl\u{00e4}che. Sobald ein Server startet, erscheint er hier klickbar.", "No local port is currently answering with a web interface. As soon as a server starts, it shows up here as a link."),
                    )
                    .font(FontId::proportional(11.0))
                    .color(pal.ink_muted),
                );
            });
        } else {
            let row_w = ui.available_width();
            let row_h = 44.0;
            for (idx, w) in self.sys_view.web.iter().enumerate() {
                let (card_rect, _) =
                    ui.allocate_exact_size(Vec2::new(row_w, row_h), Sense::hover());
                let cy = card_rect.center().y;

                // Measure port pill first so we can check hover on the pill
                let port_str = format!(":{}", w.port);
                let port_galley = ui.painter().layout_no_wrap(
                    port_str,
                    FontId::proportional(11.5),
                    pal.ink,
                );
                let port_text_w = port_galley.size().x;
                // 11px left pad + text_w + 9px gap + 20px circle + 3px right pad
                let pill_w = (11.0 + port_text_w + 9.0 + 20.0 + 3.0).max(66.0);
                let pill_h = 26.0;
                let pill_rect = Rect::from_min_size(
                    Pos2::new(card_rect.right() - 10.0 - pill_w, cy - pill_h * 0.5),
                    Vec2::new(pill_w, pill_h),
                );
                let pill_resp = ui.interact(
                    pill_rect,
                    egui::Id::new(("web_port_pill", idx, w.port)),
                    Sense::click(),
                );

                // Sleek single-border Obsidian card (avoids 14 stacked double-bezel lines)
                let card_border = if pill_resp.hovered() {
                    pal.rule_strong
                } else {
                    pal.rule
                };
                ui.painter().rect(
                    card_rect,
                    Rounding::same(10.0),
                    pal.card,
                    Stroke::new(1.0_f32, card_border),
                );

                // 1. Glowing Status LED on the left
                let led_center = Pos2::new(card_rect.left() + 16.0, cy);
                draw_status_led(ui.painter(), led_center, w.lan);

                // 2. Title + Process/PID subtitle stacked cleanly around cy (clipped before pill_rect)
                let title = if w.title.is_empty() {
                    &w.name
                } else {
                    &w.title
                };
                let text_left = card_rect.left() + 30.0;
                let text_clip = Rect::from_min_max(
                    Pos2::new(text_left, card_rect.top()),
                    Pos2::new((pill_rect.left() - 8.0).max(text_left + 20.0), card_rect.bottom()),
                );
                let text_painter = ui.painter().with_clip_rect(text_clip);
                text_painter.text(
                    Pos2::new(text_left, cy - 6.8),
                    egui::Align2::LEFT_CENTER,
                    title,
                    FontId::proportional(12.2),
                    pal.ink,
                );
                let sub_str = format!(
                    "{} (PID {}){}",
                    w.name,
                    w.pid,
                    if w.lan { t(" \u{00b7} LAN", " \u{00b7} LAN") } else { t(" \u{00b7} lokal", " \u{00b7} local") }
                );
                text_painter.text(
                    Pos2::new(text_left, cy + 7.2),
                    egui::Align2::LEFT_CENTER,
                    sub_str,
                    FontId::proportional(10.2),
                    pal.ink_muted,
                );

                // 3. Obsidian Linear Button-in-Button Port Pill (":4196 [↗]") with guaranteed 9px gap
                let bg_col = if pill_resp.hovered() {
                    pal.hover
                } else {
                    pal.input_ground
                };
                let border_col = if pill_resp.hovered() {
                    pal.action.gamma_multiply(0.65)
                } else {
                    pal.rule_strong
                };
                ui.painter().rect(
                    pill_rect,
                    Rounding::same(13.0),
                    bg_col,
                    Stroke::new(1.0_f32, border_col),
                );

                ui.painter().galley(
                    Pos2::new(pill_rect.left() + 11.0, cy - port_galley.size().y * 0.5),
                    port_galley,
                    pal.ink,
                );

                let circle_center = Pos2::new(pill_rect.right() - 13.0, cy);
                ui.painter().circle_filled(
                    circle_center,
                    10.0,
                    if pill_resp.hovered() {
                        pal.action
                    } else {
                        pal.action.gamma_multiply(0.20)
                    },
                );
                draw_diag_arrow_icon(
                    ui.painter(),
                    circle_center,
                    if pill_resp.hovered() {
                        pal.action_ink
                    } else {
                        pal.action
                    },
                );

                if pill_resp.on_hover_text(&w.url).clicked() {
                    let _ = open::that(&w.url);
                }

                ui.add_space(5.0);
            }
        }

        ui.add_space(8.0);

        // 2. All Listening Ports & owning processes
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr!(
                    "Belegte Ports & Dienste ({})", "Used ports & services ({})",
                    self.sys_view.all_ports.len()
                ))
                .font(FontId::proportional(11.5))
                .strong()
                .color(pal.ink_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let toggle_txt = if self.show_all_ports {
                    t("Ausblenden", "Hide")
                } else {
                    t("Alle anzeigen", "Show all")
                };
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(toggle_txt)
                                .font(FontId::proportional(10.5))
                                .color(pal.ink),
                        )
                        .fill(pal.card)
                        .stroke(Stroke::new(1.0_f32, pal.rule))
                        .rounding(Rounding::same(5.0)),
                    )
                    .clicked()
                {
                    self.show_all_ports = !self.show_all_ports;
                }
            });
        });
        ui.add_space(4.0);

        if self.show_all_ports && !self.sys_view.all_ports.is_empty() {
            double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
                for p in &self.sys_view.all_ports {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(58.0, 17.0), |ui| {
                            ui.label(
                                RichText::new(format!(":{}", p.port))
                                    .font(FontId::proportional(11.5))
                                    .strong()
                                    .color(if p.is_web { pal.action } else { pal.ink }),
                            );
                        });
                        ui.allocate_ui(Vec2::new(165.0, 17.0), |ui| {
                            ui.label(
                                RichText::new(&p.name)
                                    .font(FontId::proportional(11.5))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                        ui.label(
                            RichText::new(format!("PID {} \u{00b7} {}", p.pid, p.addr))
                                .font(FontId::proportional(10.5))
                                .color(pal.ink_muted),
                        );
                        if let Some(ref url) = p.web_url {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .small_button(
                                        RichText::new(t("\u{00d6}ffnen", "Open")).color(pal.action),
                                    )
                                    .clicked()
                                {
                                    let _ = open::that(url);
                                }
                            });
                        }
                    });
                }
            });
            ui.add_space(8.0);
        }

        // 3. Top RAM & CPU Consumers
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t("Gr\u{00f6}\u{00df}te RAM- & CPU-Verbraucher", "Biggest RAM & CPU consumers"))
                    .font(FontId::proportional(11.5))
                    .strong()
                    .color(pal.ink_muted),
            );
            if let Some(first) = self.sys_view.top.first() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(tr!(
                            "{} {} \u{00b7} Gesamt {:.0} %", "{} {} \u{00b7} total {:.0} %",
                            first.title,
                            fmt_bytes(first.mem),
                            self.sys_view.ram
                        ))
                        .font(FontId::proportional(10.5))
                        .color(pal.ink_muted),
                    );
                });
            }
        });
        ui.add_space(4.0);

        double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 8.0), |ui| {
            let max_mem = self
                .sys_view
                .top
                .first()
                .map(|g| g.mem.max(1))
                .unwrap_or(1) as f32;
            for g in &self.sys_view.top {
                ui.horizontal(|ui| {
                    ui.allocate_ui(Vec2::new(148.0, 18.0), |ui| {
                        let label = if g.count > 1 {
                            format!("{} ({})", g.title, g.count)
                        } else {
                            g.title.clone()
                        };
                        ui.label(
                            RichText::new(label)
                                .font(FontId::proportional(11.5))
                                .color(pal.ink),
                        );
                    });

                    let (bar_rect, _) =
                        ui.allocate_exact_size(Vec2::new(90.0, 7.0), Sense::hover());
                    let share = ((g.mem as f32) / max_mem).clamp(0.0, 1.0);
                    draw_meter_bar(ui.painter(), bar_rect, share, pal.action, pal.rule);

                    ui.allocate_ui(Vec2::new(68.0, 18.0), |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(fmt_bytes(g.mem))
                                    .font(FontId::proportional(11.0))
                                    .strong()
                                    .color(pal.ink),
                            );
                        });
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{:.1} %", g.cpu))
                                .font(FontId::proportional(11.0))
                                .color(pal.ink_muted),
                        );
                    });
                });
            }
        });

        ui.add_space(6.0);
        let ask_ram = egui::Button::new(
            RichText::new(t("Was kann ich schlie\u{00df}en? Claude fragen", "What can I close? Ask Claude"))
                .font(FontId::proportional(11.5))
                .color(pal.ink),
        )
        .fill(pal.card)
        .stroke(Stroke::new(1.0_f32, pal.rule_strong))
        .rounding(Rounding::same(6.0));
        if ui.add(ask_ram).clicked() {
            self.ask_claude(
                t("Welche Programme belegen gerade am meisten Arbeitsspeicher, brauche ich die gerade, und was kann ich gefahrlos schlie\u{00df}en?", "Which programs are using the most memory right now, do I need them, and what can I safely close?"),
                None,
                t("Was frisst RAM?", "What eats RAM?"),
            );
        }
    }

    // =========================================================================
    // TAB 5: SETTINGS (Einstellungen)
    // =========================================================================
    fn render_tab_settings(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &ThemePalette,
    ) {
        double_bezel_card(ui, pal, 10.0, Vec2::new(12.0, 10.0), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Position", "Placement"))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [
                        ("br", t("Unten re.", "Bottom right")),
                        ("tc", t("Oben Mitte", "Top center")),
                        ("tr", t("Oben re.", "Top right")),
                        ("taskbar", t("Taskleiste (Dock)", "Taskbar (dock)")),
                    ] {
                        let on = self.settings.placement == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.placement = code.to_string();
                            self.settings.save();
                            self.sync_window_geometry(ctx, true);
                        }
                    }
                });
            });

            if self.settings.placement == "taskbar" {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("Taskleiste auf", "Taskbar on"))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for (code, lbl) in [
                            ("secondary", t("Zweitbildschirm", "Second screen")),
                            ("primary", t("Hauptbildschirm", "Main screen")),
                        ] {
                            let on = self.settings.taskbar_display == code;
                            let btn = egui::Button::new(
                                RichText::new(lbl)
                                    .font(FontId::proportional(11.0))
                                    .color(if on { pal.action_ink } else { pal.ink }),
                            )
                            .fill(if on { pal.action } else { pal.input_ground })
                            .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                            .rounding(Rounding::same(5.0));
                            if ui.add(btn).clicked() {
                                self.settings.taskbar_display = code.to_string();
                                self.settings.save();
                                self.sync_window_geometry(ctx, true);
                            }
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("Versatz (oder Streifen ziehen)", "Offset (or drag the strip)"))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink_muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ofs = self.settings.taskbar_offset_x;
                        let small = |s: &str| {
                            egui::Button::new(RichText::new(s).font(FontId::proportional(11.0)).color(pal.ink))
                                .fill(pal.input_ground)
                                .stroke(Stroke::new(1.0_f32, pal.rule))
                                .rounding(Rounding::same(5.0))
                        };
                        if ui.add(small(t("Zur\u{00fc}cksetzen", "Reset"))).clicked() {
                            self.nudge_taskbar_offset(ctx, 0);
                        }
                        if ui.add(small("10 \u{25b6}")).on_hover_text(t("10 px nach rechts", "10 px to the right")).clicked() {
                            self.nudge_taskbar_offset(ctx, ofs + 10);
                        }
                        ui.label(
                            RichText::new(format!("{} px", ofs))
                                .font(FontId::monospace(11.0))
                                .color(pal.ink),
                        );
                        if ui.add(small("\u{25c0} 10")).on_hover_text(t("10 px nach links", "10 px to the left")).clicked() {
                            self.nudge_taskbar_offset(ctx, ofs - 10);
                        }
                    });
                });
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // 2. Strompreis
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr!("Strompreis ({}/kWh)", "Electricity price ({}/kWh)", i18n::cent_unit()))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("= {:.1}", self.settings.price_ct).replace('.', crate::i18n::dec()) + &format!(" {}/kWh", i18n::cent_unit()))
                            .font(FontId::proportional(11.0))
                            .color(if self.price_input_valid {
                                pal.action
                            } else {
                                Color32::from_rgb(0xFF, 0x4D, 0x4D)
                            }),
                    );
                    let resp = ui.add_sized(
                        [85.0, 22.0],
                        egui::TextEdit::singleline(&mut self.price_input_buf),
                    );
                    if resp.changed() {
                        if let Some(p) = parse_price(&self.price_input_buf) {
                            self.price_input_valid = true;
                            self.settings.price_ct = (p * 100.0).round() / 100.0;
                            self.settings.save();
                        } else {
                            self.price_input_valid = false;
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // 3. Messmodus
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Messung", "Measurement"))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [
                        ("estimated", t("Gesch\u{00e4}tzt", "Estimated")),
                        ("measured", t("Gemessen (RAPL+NVML)", "Measured (RAPL+NVML)")),
                    ] {
                        let on = self.settings.mode == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .stroke(Stroke::new(1.0_f32, if on { pal.action } else { pal.rule }))
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.mode = code.to_string();
                            self.settings.save();
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // 4. Monitors
            for i in 0..self.settings.monitors.len() {
                if self.monitor_w_bufs.len() <= i {
                    self.monitor_w_bufs
                        .push(format!("{:.0}", self.settings.monitors[i].w));
                }
                let m_name = self.settings.monitors[i].name.clone();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr!("Monitor: {}", "Monitor: {}", m_name))
                            .font(FontId::proportional(11.5))
                            .color(pal.ink),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("W").color(pal.ink_muted));
                        if ui
                            .add_sized(
                                [60.0, 20.0],
                                egui::TextEdit::singleline(&mut self.monitor_w_bufs[i]),
                            )
                            .changed()
                        {
                            if let Ok(v) = self.monitor_w_bufs[i].parse::<f64>() {
                                self.settings.monitors[i].w = v.clamp(0.0, 400.0);
                                self.settings.save();
                            }
                        }
                    });
                });
            }

            ui.add_space(4.0);

            // 5. Restsystem & Netzteil
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Rest-System (Mainboard, RAM, NVMe)", "Rest of system (mainboard, RAM, NVMe)"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("W").color(pal.ink_muted));
                    if ui
                        .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.rest_w_buf))
                        .changed()
                    {
                        if let Ok(v) = self.rest_w_buf.parse::<f64>() {
                            self.settings.rest_w = v.clamp(0.0, 300.0);
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Netzteil-Nennleistung (80+ Gold)", "PSU rating (80+ Gold)"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("W").color(pal.ink_muted));
                    if ui
                        .add_sized([60.0, 20.0], egui::TextEdit::singleline(&mut self.psu_w_buf))
                        .changed()
                    {
                        if let Ok(v) = self.psu_w_buf.parse::<f64>() {
                            self.settings.psu_w = v.clamp(200.0, 2000.0);
                            self.settings.save();
                        }
                    }
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // 6. Collapsed size and theme
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Gr\u{00f6}\u{00df}e zugeklappt", "Collapsed size"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("m", t("M (Breiter)", "M (wider)")), ("xs", t("XS (Kompakt)", "XS (compact)"))] {
                        let on = self.settings.size == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.size = code.to_string();
                            self.settings.save();
                            self.sync_window_geometry(ctx, true);
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Sprache", "Language"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("en", "English"), ("de", "Deutsch")] {
                        let on = self.settings.lang == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.lang = code.to_string();
                            i18n::set_en(code == "en");
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("W\u{00e4}hrung", "Currency"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for (code, lbl) in [("USD", "$ Dollar"), ("EUR", "\u{20ac} Euro")] {
                        let on = self.settings.currency == code;
                        let btn = egui::Button::new(
                            RichText::new(lbl)
                                .font(FontId::proportional(11.0))
                                .color(if on { pal.action_ink } else { pal.ink }),
                        )
                        .fill(if on { pal.action } else { pal.input_ground })
                        .rounding(Rounding::same(5.0));
                        if ui.add(btn).clicked() {
                            self.settings.currency = code.to_string();
                            i18n::set_usd(code == "USD");
                            self.settings.save();
                        }
                    }
                });
            });

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Design / Farbschema", "Design / color scheme"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let current_name = THEMES
                        .iter()
                        .find(|&&(id, _)| id == self.settings.theme)
                        .map(|&(id, _)| theme::theme_label(id))
                        .unwrap_or("Energielabel");
                    egui::ComboBox::from_id_salt("theme_combo")
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for &(id, _) in THEMES {
                                let name = theme::theme_label(id);
                                if ui
                                    .selectable_label(self.settings.theme == id, name)
                                    .clicked()
                                {
                                    self.settings.theme = id.to_string();
                                    self.settings.save();
                                }
                            }
                        });
                });
            });

            ui.add_space(4.0);

            // 7. Checkboxes & Opacity
            if ui
                .checkbox(
                    &mut self.settings.monitors_off_after_idle,
                    t("Monitore bei Leerlauf (>10 min) herausrechnen", "Exclude monitors when idle (>10 min)"),
                )
                .changed()
            {
                self.settings.save();
            }
            if ui
                .checkbox(
                    &mut self.settings.sys_addon,
                    t("System-Addon (CPU, RAM, Netzwerk, Ports & Web-UIs)", "System add-on (CPU, RAM, network, ports & web UIs)"),
                )
                .changed()
            {
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }
            if ui
                .checkbox(&mut self.settings.pinned, t("Immer im Vordergrund halten", "Keep always on top"))
                .changed()
            {
                self.settings.save();
                self.sync_window_geometry(ctx, true);
            }
            if ui
                .checkbox(&mut self.settings.autostart, t("Mit Windows starten", "Start with Windows"))
                .changed()
            {
                let want = self.settings.autostart;
                if !autostart::set(want) {
                    self.settings.autostart = autostart::is_enabled();
                }
                self.settings.save();
            }

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t("Deckkraft zugeklappt", "Collapsed opacity"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut op = self.settings.opacity as u32;
                    if ui
                        .add(egui::Slider::new(&mut op, 35..=100).suffix(" %"))
                        .changed()
                    {
                        self.settings.opacity = op as u8;
                        self.settings.save();
                    }
                });
            });
        });
    }

    // =========================================================================
    // TAB 6: SESSION SUMMARY ("Gute Nacht" Receipt)
    // =========================================================================
    fn render_tab_summary(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let Some(sum) = self.active_summary.clone() else {
            double_bezel_card(ui, pal, 10.0, Vec2::new(12.0, 12.0), |ui| {
                ui.label(
                    RichText::new(t("Session zu kurz", "Session too short"))
                        .font(FontId::proportional(15.0))
                        .strong()
                        .color(pal.ink),
                );
                ui.label(
                    RichText::new(
                        t("Unter 5 Sekunden, daher keine Zusammenfassung gespeichert. Eine neue Session l\u{00e4}uft bereits.", "Under 5 seconds, so no summary was saved. A new session is already running."),
                    )
                    .font(FontId::proportional(11.5))
                    .color(pal.ink_muted),
                );
                ui.add_space(8.0);
                if ui.button(t("Weiter zu Live", "Continue to Live")).clicked() {
                    self.active_tab = ActiveTab::Live;
                }
            });
            return;
        };

        double_bezel_card(ui, pal, 10.0, Vec2::new(12.0, 10.0), |ui| {
            ui.label(
                RichText::new(format!(
                    "{} \u{00b7} {} {}\u{2013}{}",
                    self.summary_title,
                    fmt_day(sum.start),
                    fmt_clock(sum.start),
                    fmt_clock(sum.end)
                ))
                .font(FontId::proportional(13.0))
                .strong()
                .color(pal.ink),
            );
            ui.add_space(8.0);

            let monthly_ct = (sum.cost / sum.dur_s.max(1.0)) * 3600.0 * 4.0 * 30.0;
            ui.columns(2, |cols| {
                egui::Frame::none()
                    .fill(pal.input_ground)
                    .stroke(Stroke::new(1.5_f32, pal.rule_strong))
                    .rounding(Rounding::same(7.0))
                    .inner_margin(Margin::symmetric(10.0, 8.0))
                    .show(&mut cols[0], |ui| {
                        ui.label(
                            RichText::new(fmt_euro(sum.cost))
                                .font(FontId::proportional(24.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(tr!(
                                "{:.2} kWh in {}", "{:.2} kWh in {}",
                                sum.wh / 1000.0,
                                fmt_dur(sum.dur_s)
                            ))
                            .font(FontId::proportional(11.0))
                            .color(pal.ink_muted),
                        );
                    });

                egui::Frame::none()
                    .fill(pal.input_ground)
                    .stroke(Stroke::new(1.5_f32, pal.rule_strong))
                    .rounding(Rounding::same(7.0))
                    .inner_margin(Margin::symmetric(10.0, 8.0))
                    .show(&mut cols[1], |ui| {
                        ui.label(
                            RichText::new(fmt_euro(monthly_ct))
                                .font(FontId::proportional(24.0))
                                .strong()
                                .color(pal.ink),
                        );
                        ui.label(
                            RichText::new(t("pro Monat bei 4 h t\u{00e4}glich", "per month at 4 h a day"))
                                .font(FontId::proportional(11.0))
                                .color(pal.ink_muted),
                        );
                    });
            });

            ui.add_space(8.0);
            let cls = class_of(sum.avg_w);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(
                    RichText::new(tr!("\u{00d8} {:.0} W (Klasse {})", "avg {:.0} W (class {})", sum.avg_w, cls))
                        .font(FontId::proportional(12.0))
                        .strong()
                        .color(class_number_color(cls)),
                );
                ui.label(
                    RichText::new(tr!("Spitze {:.0} W", "Peak {:.0} W", sum.peak.w))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink),
                );
                ui.label(
                    RichText::new(tr!("{} Spikes", "{} spikes", sum.spikes))
                        .font(FontId::proportional(12.0))
                        .color(pal.ink_muted),
                );
            });

            if !sum.top.is_empty() {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t("CPU-Last nach Programm", "CPU load by program"))
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(pal.ink_muted),
                );
                let tot_share: f64 = sum.top.iter().map(|p| p.share).sum::<f64>().max(1.0);
                for p in &sum.top {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(Vec2::new(150.0, 16.0), |ui| {
                            ui.label(
                                RichText::new(&p.n)
                                    .font(FontId::proportional(11.5))
                                    .color(pal.ink),
                            );
                        });
                        let (r_bar, _) =
                            ui.allocate_exact_size(Vec2::new(140.0, 7.0), Sense::hover());
                        draw_meter_bar(
                            ui.painter(),
                            r_bar,
                            (p.share / tot_share) as f32,
                            pal.action,
                            pal.rule,
                        );
                        ui.label(
                            RichText::new(format!("{:.0} %", p.share))
                                .font(FontId::proportional(11.0))
                                .color(pal.ink_muted),
                        );
                    });
                }
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if self.summary_fresh {
                    if !self.shutdown_scheduled {
                        let is_armed = self
                            .shutdown_armed_until
                            .map(|t| Instant::now() < t)
                            .unwrap_or(false);
                        let btn_txt = if is_armed {
                            t("Zum Best\u{00e4}tigen nochmal klicken", "Click again to confirm")
                        } else {
                            t("PC herunterfahren", "Shut down PC")
                        };
                        let sh_btn = egui::Button::new(
                            RichText::new(btn_txt)
                                .font(FontId::proportional(11.5))
                                .strong()
                                .color(pal.action_ink),
                        )
                        .fill(pal.action)
                        .rounding(Rounding::same(6.0));
                        if ui.add(sh_btn).clicked() {
                            if is_armed {
                                self.shutdown_armed_until = None;
                                self.shutdown_scheduled = true;
                                let _ = Command::new("shutdown")
                                    .creation_flags(CREATE_NO_WINDOW)
                                    .args([
                                        "/s",
                                        "/t",
                                        "10",
                                        "/c",
                                        t("Gute Nacht. Strom-Widget f\u{00e4}hrt den PC herunter.", "Good night. Strom-Widget is shutting the PC down."),
                                    ])
                                    .spawn();
                            } else {
                                self.shutdown_armed_until =
                                    Some(Instant::now() + Duration::from_secs(4));
                            }
                        }
                    } else {
                        let abort_btn = egui::Button::new(
                            RichText::new(t("F\u{00e4}hrt in 10 s herunter \u{00b7} Abbrechen", "Shutting down in 10 s \u{00b7} Cancel"))
                                .font(FontId::proportional(11.5))
                                .color(pal.ink),
                        )
                        .fill(pal.hover)
                        .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                        .rounding(Rounding::same(6.0));
                        if ui.add(abort_btn).clicked() {
                            self.shutdown_scheduled = false;
                            let _ = Command::new("shutdown")
                                .creation_flags(CREATE_NO_WINDOW)
                                .arg("/a")
                                .spawn();
                        }
                    }
                }

                let claude_btn = egui::Button::new(
                    RichText::new(t("Fazit von Claude", "Summary from Claude"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule_strong))
                .rounding(Rounding::same(6.0));
                if ui.add(claude_btn).clicked() {
                    let q = tr!(
                        "Schreib ein kurzes Fazit zu dieser abgeschlossenen Session ({}\u{2013}{}): Kosten {}, {:.2} kWh, \u{00d8} {:.0} W, Spitze {:.0} W, {} Spikes.", "Write a short summary of this completed session ({}\u{2013}{}): cost {}, {:.2} kWh, avg {:.0} W, peak {:.0} W, {} spikes.",
                        fmt_clock(sum.start),
                        fmt_clock(sum.end),
                        fmt_euro(sum.cost),
                        sum.wh / 1000.0,
                        sum.avg_w,
                        sum.peak.w,
                        sum.spikes
                    );
                    self.ask_claude(&q, None, t("Fazit", "Summary"));
                }

                let close_btn = egui::Button::new(
                    RichText::new(t("Weiter", "Next"))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule))
                .rounding(Rounding::same(6.0));
                if ui.add(close_btn).clicked() {
                    self.frozen_chart = None;
                    self.summary_fresh = false;
                    self.active_tab = ActiveTab::Live;
                }
            });
        });
    }

    // =========================================================================
    // CLAUDE ANSWER DRAWER & PROMPT BAR
    // =========================================================================
    fn render_claude_answer_box(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        let st_snapshot = self.claude_status.lock().unwrap().clone();
        if matches!(st_snapshot, ClaudeStatus::Idle) {
            return;
        }

        double_bezel_card(ui, pal, 8.0, Vec2::new(10.0, 7.0), |ui| {
            ui.horizontal(|ui| {
                let lbl = match &st_snapshot {
                    ClaudeStatus::Asking { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Done { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Error { label, .. } => tr!("Claude \u{00b7} {}", "Claude \u{00b7} {}", label),
                    ClaudeStatus::Idle => String::new(),
                };
                ui.label(
                    RichText::new(lbl)
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(pal.action),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (x_rect, x_resp) =
                        ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::click());
                    draw_close_icon(ui.painter(), x_rect.center(), 7.0, pal.ink_muted);
                    if x_resp.clicked() {
                        *self.claude_status.lock().unwrap() = ClaudeStatus::Idle;
                    }
                });
            });

            match st_snapshot {
                ClaudeStatus::Asking { started_at, .. } => {
                    ui.label(
                        RichText::new(tr!(
                            "Claude liest die Messdaten \u{00b7} {} s ...", "Claude is reading the measurements \u{00b7} {} s ...",
                            started_at.elapsed().as_secs()
                        ))
                        .font(FontId::proportional(11.5))
                        .color(pal.ink_muted),
                    );
                }
                ClaudeStatus::Done { answer, .. } => {
                    egui::ScrollArea::vertical()
                        .max_height(55.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(answer)
                                    .font(FontId::proportional(11.5))
                                    .color(pal.ink),
                            );
                        });
                }
                ClaudeStatus::Error { error, .. } => {
                    ui.label(
                        RichText::new(tr!("Keine Antwort: {}", "No answer: {}", error))
                            .font(FontId::proportional(11.0))
                            .color(Color32::from_rgb(0xFF, 0x8A, 0x4A)),
                    );
                }
                ClaudeStatus::Idle => {}
            }
        });
        ui.add_space(5.0);
    }

    fn render_claude_prompt_bar(&mut self, ui: &mut egui::Ui, pal: &ThemePalette) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let chips = [
                (
                    t("Zusammenfassung", "Summary"),
                    t("Fass diese Session kurz zusammen: Kosten, was hat am meisten Strom gezogen, und was war auff\u{00e4}llig?", "Summarize this session briefly: cost, what drew the most power, and what stood out?"),
                ),
                (
                    t("Gr\u{00f6}\u{00df}ter Spike", "Biggest spike"),
                    t("Was war der gr\u{00f6}\u{00df}te Spike, was hat ihn verursacht und was hat er gekostet?", "What was the biggest spike, what caused it, and what did it cost?"),
                ),
                (
                    t("Sparpotenzial", "Savings potential"),
                    t("Wo k\u{00f6}nnte ich bei diesem Nutzungsmuster konkret Strom sparen, und wie viel w\u{00e4}re das ungef\u{00e4}hr pro Monat?", "Where could I concretely save power with this usage pattern, and roughly how much would that be per month?"),
                ),
            ];
            for (lbl, q) in chips {
                let chip = egui::Button::new(
                    RichText::new(lbl)
                        .font(FontId::proportional(10.5))
                        .color(pal.ink_muted),
                )
                .fill(pal.card)
                .stroke(Stroke::new(1.0_f32, pal.rule))
                .rounding(Rounding::same(10.0));
                if ui.add(chip).clicked() {
                    self.ask_claude(q, None, lbl);
                }
            }
        });

        ui.add_space(4.0);
        egui::Frame::none()
            .fill(pal.input_ground)
            .stroke(Stroke::new(1.0_f32, pal.rule))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(8.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let edit = egui::TextEdit::singleline(&mut self.claude_input)
                        .frame(false)
                        .hint_text(t("Claude zu dieser Session fragen ...", "Ask Claude about this session ..."))
                        .desired_width(ui.available_width() - 32.0);
                    let resp = ui.add(edit);
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    // Custom Vector Send Button aligned inside the input pill
                    let (send_rect, send_resp) =
                        ui.allocate_exact_size(Vec2::new(26.0, 20.0), Sense::click());
                    ui.painter().rect_filled(
                        send_rect,
                        Rounding::same(5.0),
                        if send_resp.hovered() {
                            Color32::from_rgb(0x16, 0xD0, 0x6A)
                        } else {
                            pal.action
                        },
                    );
                    draw_send_icon(ui.painter(), send_rect.center(), pal.action_ink);

                    if (send_resp.on_hover_text(t("Fragen", "Ask")).clicked() || enter)
                        && !self.claude_input.trim().is_empty()
                    {
                        let q = self.claude_input.trim().to_string();
                        self.claude_input.clear();
                        self.ask_claude(&q, None, &q);
                    }
                });
            });
    }

    fn render_footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &ThemePalette) {
        let foot_w = ui.available_width();
        let (foot_rect, _) = ui.allocate_exact_size(Vec2::new(foot_w, 26.0), Sense::hover());
        let cy = foot_rect.center().y;

        // 1. Obsidian Linear Pill-shaped "Gute Nacht" Button with nested dark circle around Moon icon
        let btn_rect = Rect::from_min_size(
            Pos2::new(foot_rect.left(), cy - 13.0),
            Vec2::new(116.0, 26.0),
        );
        let btn_resp = ui.interact(btn_rect, egui::Id::new("footer_goodnight"), Sense::click());
        let fill_c = if btn_resp.hovered() {
            Color32::from_rgb(0x16, 0xD0, 0x6A)
        } else {
            pal.action
        };
        ui.painter()
            .rect_filled(btn_rect, Rounding::same(13.0), fill_c);

        let moon_center = Pos2::new(btn_rect.left() + 13.0, cy);
        let moon_bg = Color32::from_rgb(0x06, 0x28, 0x14);
        ui.painter().circle_filled(moon_center, 9.0, moon_bg);
        draw_moon_icon(
            ui.painter(),
            moon_center,
            Color32::from_rgb(0xE2, 0xF8, 0xEC),
            moon_bg,
        );
        ui.painter().text(
            Pos2::new(btn_rect.left() + 27.0, cy),
            egui::Align2::LEFT_CENTER,
            t("Gute Nacht", "Good night"),
            FontId::proportional(12.0),
            pal.action_ink,
        );
        if btn_resp.clicked() {
            self.trigger_goodnight(ctx);
        }

        // 2. Subtitle aligned on the exact same centerline cy
        ui.painter().text(
            Pos2::new(btn_rect.right() + 10.0, cy),
            egui::Align2::LEFT_CENTER,
            t("schlie\u{00df}t die Session mit Beleg ab", "closes the session with a receipt"),
            FontId::proportional(11.0),
            pal.ink_muted,
        );

        // 3. Right-aligned "Beenden" button on the exact same centerline cy
        let quit_rect = Rect::from_min_size(
            Pos2::new(foot_rect.right() - 52.0, cy - 11.0),
            Vec2::new(52.0, 22.0),
        );
        let quit_resp = ui.interact(quit_rect, egui::Id::new("footer_quit"), Sense::click());
        ui.painter().text(
            Pos2::new(foot_rect.right() - 2.0, cy),
            egui::Align2::RIGHT_CENTER,
            t("Beenden", "Quit"),
            FontId::proportional(11.0),
            if quit_resp.hovered() {
                pal.ink
            } else {
                pal.ink_muted
            },
        );
        if quit_resp.clicked() {
            self.session_mgr.save();
            std::process::exit(0);
        }
    }

    // =========================================================================
    // CANVAS PAINTERS: Mini Ticker & Full History Chart
    // =========================================================================
    fn draw_mini_ticker(&self, painter: &egui::Painter, rect: Rect) {
        let n = 60usize;
        let len = self.session_mgr.live.len();
        if len == 0 {
            return;
        }
        let slice = &self.session_mgr.live[len.saturating_sub(n)..];
        let min_w = slice
            .iter()
            .map(|s| s.w)
            .fold(f64::INFINITY, f64::min)
            * 0.6;
        let max_w = slice
            .iter()
            .map(|s| s.w)
            .fold(min_w + 150.0, f64::max);

        let slot = rect.width() / (n as f32);
        let offset = n.saturating_sub(slice.len());
        for (i, s) in slice.iter().enumerate() {
            let x = rect.left() + ((offset + i) as f32) * slot;
            let frac = ((s.w - min_w) / (max_w - min_w)).clamp(0.18, 1.0) as f32;
            let bh = (rect.height() * frac).max(1.5);
            let bar_rect = Rect::from_min_max(
                Pos2::new(x, rect.bottom() - bh),
                Pos2::new(x + (slot - 1.0).max(1.0), rect.bottom()),
            );
            painter.rect_filled(bar_rect, 0.0, load_color(s.w));
        }
    }

    fn draw_history_chart(&self, painter: &egui::Painter, rect: Rect, pal: &ThemePalette) {
        let now = now_ms();
        let (pts, from, to, spikes): (Vec<(u64, f64)>, u64, u64, &[Spike]) =
            if let Some(ref fz) = self.frozen_chart {
                (fz.points.clone(), fz.from, fz.to, &fz.spikes)
            } else if self.chart_range_s == 0 {
                let p: Vec<(u64, f64)> = if self.session_mgr.s.minutes.len() * 30
                    > self.session_mgr.live.len()
                {
                    self.session_mgr
                        .s
                        .minutes
                        .iter()
                        .map(|m| (m.t + 30_000, m.avg_w()))
                        .collect()
                } else {
                    self.session_mgr.live.iter().map(|s| (s.t, s.w)).collect()
                };
                (
                    p,
                    self.session_mgr.s.start,
                    now,
                    &self.session_mgr.s.spikes,
                )
            } else {
                let from_t = now.saturating_sub(self.chart_range_s * 1000);
                let p: Vec<(u64, f64)> = self
                    .session_mgr
                    .live
                    .iter()
                    .filter(|s| s.t >= from_t)
                    .map(|s| (s.t, s.w))
                    .collect();
                (p, from_t, now, &self.session_mgr.s.spikes)
            };

        let gutter = 32.0;
        let plot_rect = Rect::from_min_max(
            Pos2::new(rect.left() + gutter, rect.top() + 4.0),
            Pos2::new(rect.right(), rect.bottom() - 2.0),
        );

        let peak_w = self.session_mgr.s.peak.w;
        let max_scale = (((peak_w * 1.1) / 100.0).ceil() * 100.0).max(400.0);

        let step = if max_scale > 800.0 { 400.0 } else { 200.0 };
        let mut v = 0.0;
        while v <= max_scale {
            let frac = (v / max_scale) as f32;
            let y = plot_rect.bottom() - frac * plot_rect.height();
            painter.line_segment(
                [Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)],
                Stroke::new(1.0_f32, pal.rule),
            );
            painter.text(
                Pos2::new(rect.left(), y),
                egui::Align2::LEFT_CENTER,
                format!("{:.0}", v),
                FontId::proportional(10.0),
                pal.ink_muted,
            );
            v += step;
        }

        let span = (to.saturating_sub(from)).max(1) as f64;

        for sp in spikes {
            if sp.end < from || sp.start > to {
                continue;
            }
            let f0 = ((sp.start.saturating_sub(from)) as f64 / span).clamp(0.0, 1.0) as f32;
            let f1 = ((sp.end.saturating_sub(from)) as f64 / span).clamp(0.0, 1.0) as f32;
            let x0 = plot_rect.left() + f0 * plot_rect.width();
            let x1 = (plot_rect.left() + f1 * plot_rect.width()).max(x0 + 3.0);
            let sp_rect = Rect::from_min_max(
                Pos2::new(x0, plot_rect.top()),
                Pos2::new(x1, plot_rect.bottom()),
            );
            painter.rect_filled(sp_rect, 0.0, load_color(sp.peak).gamma_multiply(0.20));
        }

        let col_w = 4.5f32;
        let nb = (plot_rect.width() / col_w).floor().max(1.0) as usize;
        let mut buckets: Vec<Option<f64>> = vec![None; nb];
        for (t, w) in pts {
            if t < from || t > to {
                continue;
            }
            let idx = (((t - from) as f64 / span) * (nb as f64)).floor() as usize;
            if idx < nb {
                buckets[idx] = Some(buckets[idx].map_or(w, |prev| prev.max(w)));
            }
        }

        for (i, val) in buckets.into_iter().enumerate() {
            if let Some(w_val) = val {
                let frac = (w_val / max_scale).clamp(0.03, 1.0) as f32;
                let bh = frac * plot_rect.height();
                let x = plot_rect.left() + (i as f32) * col_w;
                let bar_r = Rect::from_min_max(
                    Pos2::new(x, plot_rect.bottom() - bh),
                    Pos2::new(x + col_w - 1.2, plot_rect.bottom()),
                );
                draw_gradient_bar(painter, bar_r, load_color(w_val));
            }
        }
    }
}

fn draw_meter_bar(
    painter: &egui::Painter,
    rect: Rect,
    share: f32,
    fill_col: Color32,
    bg_col: Color32,
) {
    let r = (rect.height() * 0.5).min(2.0);
    painter.rect_filled(rect, Rounding::same(r), bg_col);
    let s = share.clamp(0.0, 1.0);
    if s > 0.015 {
        let w = (rect.width() * s).max(2.0);
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(w, rect.height()));
        painter.rect_filled(fill_rect, Rounding::same(r), fill_col);
    }
}

// =============================================================================
// GERMAN NUMBER & TIME FORMATTERS (Pure Unicode Escapes)
// =============================================================================
fn format_de_price(v: f64) -> String {
    if (v - v.round()).abs() < 0.01 {
        format!("{:.0}", v)
    } else {
        format!("{:.2}", v).replace('.', crate::i18n::dec())
    }
}

/// Small amounts in cents ("7,0 ct" / "7.0\u{00a2}"), from one euro/dollar on as money.
fn fmt_money_ct(ct: f64) -> String {
    if ct >= 100.0 {
        return i18n::money(ct);
    }
    let num = if ct < 1.0 { format!("{:.2}", ct) } else { format!("{:.1}", ct) }.replace('.', crate::i18n::dec());
    if i18n::is_usd() {
        format!("{}\u{00a2}", num)
    } else {
        format!("{} ct", num)
    }
}

/// Money amount in the chosen currency (name kept for history: it is not euro-only).
fn fmt_euro(ct: f64) -> String {
    i18n::money(ct)
}

fn fmt_dur(secs: f64) -> String {
    let m = (secs / 60.0).floor() as u64;
    if m < 60 {
        format!("{} min", m)
    } else {
        format!("{}:{:02} h", m / 60, m % 60)
    }
}

fn fmt_clock(ms: u64) -> String {
    use chrono::{Local, TimeZone};
    if let Some(dt) = Local.timestamp_millis_opt(ms as i64).single() {
        dt.format("%H:%M").to_string()
    } else {
        "--:--".to_string()
    }
}

fn fmt_day(ms: u64) -> String {
    use chrono::{Datelike, Local, TimeZone, Weekday};
    if let Some(dt) = Local.timestamp_millis_opt(ms as i64).single() {
        if crate::i18n::is_en() {
            return dt.format("%a %b %d").to_string();
        }
        let wd = match dt.weekday() {
            Weekday::Mon => "Mo",
            Weekday::Tue => "Di",
            Weekday::Wed => "Mi",
            Weekday::Thu => "Do",
            Weekday::Fri => "Fr",
            Weekday::Sat => "Sa",
            Weekday::Sun => "So",
        };
        format!("{} {:02}.{:02}.", wd, dt.day(), dt.month())
    } else {
        String::new()
    }
}

fn fmt_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.1} GB", (b as f64) / 1_073_741_824.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0} MB", (b as f64) / 1_048_576.0)
    }
}

fn fmt_rate(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1} MB/s", b / 1_048_576.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0} KB/s", b / 1024.0)
    }
}

fn fmt_rate_short(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1}M", b / 1_048_576.0).replace('.', crate::i18n::dec())
    } else {
        format!("{:.0}K", b / 1024.0)
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
    if !claim_single_instance() {
        return Ok(());
    }
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

    let mut settings = Settings::load();
    settings.autostart = autostart::is_enabled();
    i18n::set_en(settings.lang == "en");
    i18n::set_usd(settings.currency == "USD");
    let (cw, ch) = if settings.placement == "taskbar" {
        let w = if settings.sys_addon {
            if settings.size == "m" { 316.0 } else { 302.0 }
        } else if settings.size == "m" {
            206.0
        } else {
            192.0
        };
        (w, 42.0)
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

/// Example system data for screenshots that must not show the real machine's ports and programs.
fn apply_demo_system_data(v: &mut sensors::SysMonView) {
    use sensors::{ListeningPortEntry, RamGroup, WebServiceInfo};
    let web = [
        (3000u16, 4120u32, "node", "Next.js App", "http://localhost:3000/"),
        (5173, 4388, "node", "Vite + React", "http://localhost:5173/"),
        (8000, 5210, "python", "Directory listing for /", "http://localhost:8000/"),
    ];
    v.web = web
        .iter()
        .map(|&(port, pid, name, title, url)| WebServiceInfo {
            port,
            pid,
            name: name.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            lan: false,
        })
        .collect();
    let mut ports: Vec<ListeningPortEntry> = web
        .iter()
        .map(|&(port, pid, name, title, url)| ListeningPortEntry {
            port,
            pid,
            name: name.to_string(),
            addr: "127.0.0.1".to_string(),
            is_web: true,
            web_title: Some(title.to_string()),
            web_url: Some(url.to_string()),
        })
        .collect();
    for (port, pid, name) in [(5432u16, 3016u32, "postgres"), (6379, 2984, "redis-server"), (27017, 3340, "mongod")] {
        ports.push(ListeningPortEntry {
            port,
            pid,
            name: name.to_string(),
            addr: "127.0.0.1".to_string(),
            is_web: false,
            web_title: None,
            web_url: None,
        });
    }
    ports.sort_by_key(|p| p.port);
    v.all_ports = ports;
    v.top = [
        ("chrome", "Google Chrome", 24usize, 2_254_000_000u64, 6.5f64),
        ("code", "VS Code", 9, 1_380_000_000, 3.1),
        ("discord", "Discord", 6, 640_000_000, 0.8),
        ("steam", "Steam", 5, 455_000_000, 0.4),
    ]
    .iter()
    .map(|&(n, title, count, mem, cpu)| RamGroup {
        n: n.to_string(),
        title: title.to_string(),
        count,
        mem,
        cpu,
    })
    .collect();
}
