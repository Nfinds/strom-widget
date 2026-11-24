// Power model & Settings: turns raw readings into wall-socket watts, two ways.
//   measured:  CPU package from RAPL + GPU board power from NVML
//   estimated: CPU and GPU derived from utilisation only
// Both add the same rest-of-system and monitor estimate and divide by PSU efficiency.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub name: String,
    pub w: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct SavedBounds {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub price_ct: f64,
    pub mode: String, // "measured" | "estimated"
    pub rest_w: f64,
    pub psu_w: f64,
    pub monitors: Vec<MonitorConfig>,
    pub monitors_off_after_idle: bool,
    pub cpu_idle_w: f64,
    pub cpu_max_w: f64,
    pub gpu_idle_w: f64,
    pub gpu_max_w: f64,
    pub pinned: bool,
    pub placement: String, // "taskbar" | "tr" | "tc" | "br" | "custom"
    pub autostart: bool,
    pub opacity: u8,
    pub theme: String,
    pub sys_addon: bool,
    /// CPU, RAM and network also in the collapsed strip (makes it about 110 px wider).
    #[serde(default)]
    pub strip_sys: bool,
    pub size: String, // "xs" | "m"
    #[serde(default = "default_lang")]
    pub lang: String, // "de" | "en"
    #[serde(default = "default_currency")]
    pub currency: String, // "EUR" | "USD"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<SavedBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_summary_id: Option<String>,
    #[serde(default = "default_taskbar_offset")]
    pub taskbar_offset_x: i32,
    /// Which taskbar the strip docks into: "secondary" (second screen) or "primary".
    #[serde(default = "default_taskbar_display")]
    pub taskbar_display: String,
}

fn default_currency() -> String {
    crate::i18n::system_default_currency().to_string()
}

fn default_lang() -> String {
    crate::i18n::system_default_lang().to_string()
}

fn default_taskbar_offset() -> i32 {
    0
}

fn default_taskbar_display() -> String {
    "primary".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            price_ct: 35.0,
            mode: "measured".to_string(),
            rest_w: 35.0,
            psu_w: 750.0,
            monitors: vec![MonitorConfig {
                name: "Monitor 1".to_string(),
                w: 30.0,
            }],
            monitors_off_after_idle: true,
            cpu_idle_w: 30.0,
            cpu_max_w: 120.0,
            gpu_idle_w: 30.0,
            gpu_max_w: 250.0,
            pinned: true,
            placement: "taskbar".to_string(),
            autostart: false,
            opacity: 100,
            theme: "label".to_string(),
            sys_addon: true,
            strip_sys: false,
            size: "xs".to_string(),
            lang: default_lang(),
            currency: default_currency(),
            bounds: None,
            display_id: None,
            pending_summary_id: None,
            taskbar_offset_x: 0,
            taskbar_display: default_taskbar_display(),
        }
    }
}

impl Settings {
    pub fn app_data_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("Strom-Widget")
        } else {
            PathBuf::from(".").join("Strom-Widget-Data")
        }
    }

    /// Whether the collapsed strip shows the CPU/RAM/network columns.
    pub fn strip_shows_sys(&self) -> bool {
        self.sys_addon && self.strip_sys
    }

    /// Width of the strip docked in the taskbar (points): watts, session cost and time,
    /// chevron, plus the CPU/RAM and network columns when they are switched on.
    pub fn taskbar_strip_width(&self) -> i32 {
        match (self.strip_shows_sys(), self.size == "m") {
            (true, true) => 262,
            (true, false) => 248,
            (false, true) => 152,
            (false, false) => 138,
        }
    }

    pub fn load() -> Self {
        let dir = Self::app_data_dir();
        let file = dir.join("settings.json");
        let mut s = Self::default();
        if let Ok(raw) = fs::read_to_string(&file) {
            if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(obj) = parsed.as_object_mut() {
                    if let Some(v) = obj.get("priceCt").and_then(|x| x.as_f64()) {
                        s.price_ct = v;
                    }
                    if let Some(v) = obj.get("mode").and_then(|x| x.as_str()) {
                        s.mode = v.to_string();
                    }
                    if let Some(v) = obj.get("restW").and_then(|x| x.as_f64()) {
                        s.rest_w = v;
                    }
                    if let Some(v) = obj.get("psuW").and_then(|x| x.as_f64()) {
                        s.psu_w = v;
                    }
                    if let Some(arr) = obj.get("monitors").and_then(|x| x.as_array()) {
                        let mut mons = Vec::new();
                        for m in arr {
                            if let (Some(n), Some(w)) = (
                                m.get("name").and_then(|x| x.as_str()),
                                m.get("w").and_then(|x| x.as_f64()),
                            ) {
                                mons.push(MonitorConfig {
                                    name: n.to_string(),
                                    w,
                                });
                            }
                        }
                        if !mons.is_empty() {
                            s.monitors = mons;
                        }
                    }
                    if let Some(v) = obj.get("monitorsOffAfterIdle").and_then(|x| x.as_bool()) {
                        s.monitors_off_after_idle = v;
                    }
                    if let Some(v) = obj.get("cpuIdleW").and_then(|x| x.as_f64()) {
                        s.cpu_idle_w = v;
                    }
                    if let Some(v) = obj.get("cpuMaxW").and_then(|x| x.as_f64()) {
                        s.cpu_max_w = v;
                    }
                    if let Some(v) = obj.get("gpuIdleW").and_then(|x| x.as_f64()) {
                        s.gpu_idle_w = v;
                    }
                    if let Some(v) = obj.get("gpuMaxW").and_then(|x| x.as_f64()) {
                        s.gpu_max_w = v;
                    }
                    if let Some(v) = obj.get("pinned").and_then(|x| x.as_bool()) {
                        s.pinned = v;
                    }
                    if let Some(v) = obj.get("placement").and_then(|x| x.as_str()) {
                        // Upgrade legacy "custom" or old placements to "taskbar" if not explicitly changed in Rust,
                        // or preserve if valid. Default is "taskbar".
                        if v == "taskbar" || v == "tr" || v == "tc" || v == "br" {
                            s.placement = v.to_string();
                        } else {
                            s.placement = "taskbar".to_string();
                        }
                    }
                    if let Some(v) = obj.get("autostart").and_then(|x| x.as_bool()) {
                        s.autostart = v;
                    }
                    if let Some(v) = obj.get("opacity").and_then(|x| x.as_u64()) {
                        s.opacity = v.clamp(35, 100) as u8;
                    }
                    if let Some(v) = obj.get("theme").and_then(|x| x.as_str()) {
                        s.theme = v.to_string();
                    }
                    if let Some(v) = obj.get("sysAddon").and_then(|x| x.as_bool()) {
                        s.sys_addon = v;
                    }
                    if let Some(v) = obj.get("stripSys").and_then(|x| x.as_bool()) {
                        s.strip_sys = v;
                    }
                    if let Some(v) = obj.get("size").and_then(|x| x.as_str()) {
                        s.size = v.to_string();
                    }
                    if let Some(v) = obj.get("currency").and_then(|x| x.as_str()) {
                        s.currency = if v == "USD" { "USD" } else { "EUR" }.to_string();
                    }
                    if let Some(v) = obj.get("lang").and_then(|x| x.as_str()) {
                        s.lang = if v == "de" { "de" } else { "en" }.to_string();
                    }
                    if let Some(v) = obj.get("taskbarOffsetX").and_then(|x| x.as_i64()) {
                        s.taskbar_offset_x = v as i32;
                    }
                    if let Some(v) = obj.get("taskbarDisplay").and_then(|x| x.as_str()) {
                        s.taskbar_display = if v == "primary" { "primary" } else { "secondary" }.to_string();
                    }
                    if let Some(v) = obj.get("pendingSummaryId").and_then(|x| x.as_str()) {
                        s.pending_summary_id = Some(v.to_string());
                    }
                }
            }
        }
        // Always ensure taskbar mode by default if coming from old Electron config "custom"
        s
    }

    pub fn save(&self) {
        let dir = Self::app_data_dir();
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("settings.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(file, json);
        }
    }
}

const EFF: [(f64, f64); 6] = [
    (0.0, 0.70),
    (0.05, 0.80),
    (0.10, 0.85),
    (0.20, 0.89),
    (0.50, 0.91),
    (1.0, 0.88),
];

pub fn psu_efficiency(dc_w: f64, psu_w: f64) -> f64 {
    let f = (dc_w / psu_w.max(1.0)).max(0.0);
    for i in 1..EFF.len() {
        let (x1, y1) = EFF[i];
        let (x0, y0) = EFF[i - 1];
        if f <= x1 {
            return y0 + ((f - x0) / (x1 - x0)) * (y1 - y0);
        }
    }
    EFF[EFF.len() - 1].1
}

/// Accepts "35", "0,35", "35 ct", "0.35 €", "$0.17", "17¢": values below 2 are whole euros/dollars, otherwise cents.
pub fn parse_price(input: &str) -> Option<f64> {
    let cleaned = input.replace(',', ".");
    let mut num_str = String::new();
    let mut dot_seen = false;
    for ch in cleaned.chars() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
        } else if ch == '.' && !dot_seen && !num_str.is_empty() {
            dot_seen = true;
            num_str.push(ch);
        } else if !num_str.is_empty() {
            break;
        }
    }
    let v: f64 = num_str.parse().ok()?;
    if v <= 0.0 || v > 300.0 {
        return None;
    }
    Some(if v < 2.0 { v * 100.0 } else { v })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartReading {
    pub w: f64,
    pub src: String, // "gem." | "gesch." | "aus"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartsBreakdown {
    pub cpu: PartReading,
    pub gpu: PartReading,
    pub rest: PartReading,
    pub psu: PartReading,
    pub mon: PartReading,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopProcSample {
    pub n: String,
    pub c: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSensorSample {
    pub cpu_w: Option<f64>,
    pub util: f64,
    pub gpu_w: Option<f64>,
    pub gpu_util: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub rapl_available: bool,
    pub nv_available: bool,
    pub top: Vec<TopProcSample>,
    pub fg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputedReading {
    pub watts: f64,
    pub measured_w: f64,
    pub estimated_w: f64,
    pub parts: PartsBreakdown,
    pub eff: f64,
    pub util: f64,
    pub gpu_util: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub rapl: bool,
    pub nv: bool,
    pub top: Vec<TopProcSample>,
    pub fg: Option<String>,
}

pub fn compute(raw: &RawSensorSample, s: &Settings, monitors_on: bool) -> ComputedReading {
    let util = raw.util;
    let gpu_util = raw.gpu_util.unwrap_or(0.0);

    let cpu_est = (s.cpu_idle_w + util * ((s.cpu_max_w - s.cpu_idle_w) / 110.0))
        .clamp(s.cpu_idle_w, s.cpu_max_w);
    let gpu_est = (s.gpu_idle_w + (gpu_util / 100.0) * (s.gpu_max_w - s.gpu_idle_w))
        .clamp(s.gpu_idle_w, s.gpu_max_w);

    let monitors: f64 = if monitors_on {
        s.monitors.iter().map(|m| m.w).sum()
    } else {
        0.0
    };

    let wall = |cpu: f64, gpu: f64| -> (f64, f64, f64, f64) {
        let dc = cpu + gpu + s.rest_w;
        let eff = psu_efficiency(dc, s.psu_w);
        let loss = dc / eff - dc;
        let total = dc / eff + monitors;
        (dc, eff, loss, total)
    };

    let (_est_dc, est_eff, est_loss, est_total) = wall(cpu_est, gpu_est);
    let (_meas_dc, meas_eff, meas_loss, meas_total) =
        wall(raw.cpu_w.unwrap_or(cpu_est), raw.gpu_w.unwrap_or(gpu_est));

    let use_meas = s.mode == "measured";
    let (chosen_eff, chosen_loss, chosen_total) = if use_meas {
        (meas_eff, meas_loss, meas_total)
    } else {
        (est_eff, est_loss, est_total)
    };

    let cpu_part = match raw.cpu_w {
        Some(w) if use_meas => PartReading {
            w,
            src: "gem.".to_string(),
        },
        _ => PartReading {
            w: cpu_est,
            src: "gesch.".to_string(),
        },
    };

    let gpu_part = match raw.gpu_w {
        Some(w) if use_meas => PartReading {
            w,
            src: "gem.".to_string(),
        },
        _ => PartReading {
            w: gpu_est,
            src: "gesch.".to_string(),
        },
    };

    ComputedReading {
        watts: chosen_total,
        measured_w: meas_total,
        estimated_w: est_total,
        parts: PartsBreakdown {
            cpu: cpu_part,
            gpu: gpu_part,
            rest: PartReading {
                w: s.rest_w,
                src: "gesch.".to_string(),
            },
            psu: PartReading {
                w: chosen_loss,
                src: "gesch.".to_string(),
            },
            mon: PartReading {
                w: monitors,
                src: if monitors_on {
                    "gesch.".to_string()
                } else {
                    "aus".to_string()
                },
            },
        },
        eff: chosen_eff,
        util,
        gpu_util: raw.gpu_util,
        gpu_temp: raw.gpu_temp,
        rapl: raw.rapl_available && raw.cpu_w.is_some(),
        nv: raw.nv_available && raw.gpu_w.is_some(),
        top: raw.top.clone(),
        fg: raw.fg.clone(),
    }
}

/// EU Energy Label efficiency class A..G
pub fn class_of(watts: f64) -> &'static str {
    if watts < 150.0 {
        "A"
    } else if watts < 250.0 {
        "B"
    } else if watts < 350.0 {
        "C"
    } else if watts < 450.0 {
        "D"
    } else if watts < 550.0 {
        "E"
    } else if watts < 650.0 {
        "F"
    } else {
        "G"
    }
}
