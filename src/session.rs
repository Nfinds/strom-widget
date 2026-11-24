// Session bookkeeping: integrates energy, keeps history, detects spikes,
// persists continuously so a hard power-off loses at most ~30 s.
// 100% compatible with the existing JSON format in %APPDATA%\Strom-Widget.

use crate::i18n::t;
use crate::power::{ComputedReading, TopProcSample};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_DT: f64 = 10.0; // s; larger gaps are sleep or a stall, not energy
const LIVE_KEEP: usize = 1800; // samples (~1 h at 2 s)
const BASE_WINDOW: usize = 60; // samples for the rolling median baseline

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn median(a: &[f64]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let mut s = a.to_vec();
    s.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let m = s.len() / 2;
    if s.len() % 2 == 1 {
        s[m]
    } else {
        (s[m - 1] + s[m]) / 2.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeakRecord {
    pub w: f64,
    pub t: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinuteBucket {
    pub t: u64,
    #[serde(default)]
    pub sum: f64,
    #[serde(default)]
    pub avg: Option<f64>,
    pub max: f64,
    pub cpu: f64,
    pub gpu: f64,
    #[serde(default = "default_n")]
    pub n: u32,
}

fn default_n() -> u32 {
    1
}

impl MinuteBucket {
    pub fn avg_w(&self) -> f64 {
        if let Some(a) = self.avg {
            a
        } else if self.n > 0 {
            self.sum / (self.n as f64)
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactMinute {
    pub t: u64,
    pub avg: f64,
    pub max: f64,
    pub cpu: f64,
    pub gpu: f64,
}

pub fn compact_minute(m: &MinuteBucket) -> CompactMinute {
    if m.n > 0 && m.avg.is_none() {
        CompactMinute {
            t: m.t,
            avg: (m.sum / (m.n as f64)).round(),
            max: m.max.round(),
            cpu: (m.cpu / (m.n as f64)).round(),
            gpu: (m.gpu / (m.n as f64)).round(),
        }
    } else {
        CompactMinute {
            t: m.t,
            avg: m.avg.unwrap_or(0.0).round(),
            max: m.max.round(),
            cpu: m.cpu.round(),
            gpu: m.gpu.round(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spike {
    pub start: u64,
    pub end: u64,
    pub base: f64,
    pub peak: f64,
    pub peak_t: u64,
    #[serde(default)]
    pub sum: f64,
    #[serde(default)]
    pub n: u32,
    pub extra_wh: f64,
    pub extra_ct: f64,
    #[serde(default)]
    pub seen: bool,
    pub cpu_rise: f64,
    pub gpu_rise: f64,
    #[serde(default)]
    pub procs: Vec<TopProcSample>,
    pub fg: Option<String>,
    #[serde(default)]
    pub base_cpu: f64,
    #[serde(default)]
    pub base_gpu: f64,
}

impl Spike {
    pub fn cause_line(&self) -> String {
        let mut parts = Vec::new();
        if self.gpu_rise > 25.0 || self.cpu_rise > 25.0 {
            if self.gpu_rise >= self.cpu_rise {
                parts.push(tr!("Grafikkarte +{:.0} W", "Graphics card +{:.0} W", self.gpu_rise));
            } else {
                parts.push(tr!("Prozessor +{:.0} W", "Processor +{:.0} W", self.cpu_rise));
            }
        }
        let mut who: Vec<String> = self.procs.iter().take(2).map(|p| p.n.clone()).collect();
        if let Some(ref fg) = self.fg {
            if !who.contains(fg) {
                who.insert(0, fg.clone());
            }
        }
        if !who.is_empty() {
            parts.push(who.into_iter().take(2).collect::<Vec<_>>().join(", "));
        }
        if parts.is_empty() {
            t("Ursache unklar", "Cause unclear").to_string()
        } else {
            parts.join(" \u{00b7} ")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub id: String,
    pub start: u64,
    pub last: u64,
    pub wh: f64,
    pub cost: f64,
    pub wh_measured: f64,
    pub wh_estimated: f64,
    pub peak: PeakRecord,
    pub minutes: Vec<MinuteBucket>,
    pub spikes: Vec<Spike>,
    pub top_procs: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveSample {
    pub t: u64,
    pub w: f64,
    pub cpu: f64,
    pub gpu: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopProcShare {
    pub n: String,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub start: u64,
    pub end: u64,
    pub reason: String,
    pub dur_s: f64,
    pub wh: f64,
    pub cost: f64,
    pub avg_w: f64,
    pub wh_measured: f64,
    pub wh_estimated: f64,
    pub peak: PeakRecord,
    pub spikes: usize,
    #[serde(default)]
    pub big_spikes: Vec<Spike>,
    #[serde(default)]
    pub top: Vec<TopProcShare>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFileArchive {
    pub summary: SessionSummary,
    #[serde(default)]
    pub minutes: Vec<CompactMinute>,
    #[serde(default)]
    pub spikes: Vec<Spike>,
}

pub struct SessionManager {
    pub sessions_dir: PathBuf,
    pub current_file: PathBuf,
    pub recovered: Option<SessionSummary>,
    pub s: SessionState,
    pub live: Vec<LiveSample>,
    last_t: Option<u64>,
    active_spike: Option<usize>,
    calm: u32,
    above: u32,
    last_save: u64,
}

impl SessionManager {
    pub fn new(dir: PathBuf) -> Self {
        let sessions_dir = dir.join("sessions");
        let _ = fs::create_dir_all(&sessions_dir);
        let current_file = dir.join("current.json");
        let mut mgr = Self {
            sessions_dir,
            current_file,
            recovered: None,
            s: Self::fresh_state(),
            live: Vec::new(),
            last_t: None,
            active_spike: None,
            calm: 0,
            above: 0,
            last_save: 0,
        };
        mgr.recovered = mgr.recover_previous();
        mgr.reset();
        mgr
    }

    fn fresh_state() -> SessionState {
        let now = now_ms();
        let iso = chrono::Utc::now()
            .format("%Y-%m-%dT%H-%M-%S-%3fZ")
            .to_string();
        SessionState {
            id: iso,
            start: now,
            last: now,
            wh: 0.0,
            cost: 0.0,
            wh_measured: 0.0,
            wh_estimated: 0.0,
            peak: PeakRecord { w: 0.0, t: now },
            minutes: Vec::new(),
            spikes: Vec::new(),
            top_procs: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.s = Self::fresh_state();
        self.live.clear();
        self.last_t = None;
        self.active_spike = None;
        self.calm = 0;
        self.above = 0;
        self.last_save = 0;
    }

    fn recover_previous(&self) -> Option<SessionSummary> {
        if !self.current_file.exists() {
            return None;
        }
        let raw = fs::read_to_string(&self.current_file).ok()?;
        let _ = fs::remove_file(&self.current_file);
        let s: SessionState = serde_json::from_str(&raw).ok()?;
        if s.wh <= 0.01 || s.last.saturating_sub(s.start) < 60_000 {
            return None;
        }
        Some(self.write_final(&s, t("unerwartet beendet (Strom weg oder Absturz)", "ended unexpectedly (power loss or crash)")))
    }

    pub fn add(&mut self, r: &ComputedReading, price_ct: f64) -> Option<Spike> {
        let now = now_ms();
        let dt = self
            .last_t
            .map(|lt| (now.saturating_sub(lt) as f64) / 1000.0)
            .unwrap_or(2.0);
        self.last_t = Some(now);
        self.s.last = now;

        if dt > 0.0 && dt <= MAX_DT {
            let h = dt / 3600.0;
            self.s.wh += r.watts * h;
            self.s.wh_measured += r.measured_w * h;
            self.s.wh_estimated += r.estimated_w * h;
            self.s.cost += (r.watts * h / 1000.0) * price_ct;
            for p in &r.top {
                *self.s.top_procs.entry(p.n.clone()).or_insert(0.0) += p.c * dt;
            }
        }
        if r.watts > self.s.peak.w {
            self.s.peak = PeakRecord { w: r.watts, t: now };
        }

        self.live.push(LiveSample {
            t: now,
            w: r.watts,
            cpu: r.parts.cpu.w,
            gpu: r.parts.gpu.w,
        });
        if self.live.len() > LIVE_KEEP {
            self.live.remove(0);
        }

        let mt = (now / 60_000) * 60_000;
        let need_new = self.s.minutes.last().map(|m| m.t != mt).unwrap_or(true);
        if need_new {
            self.s.minutes.push(MinuteBucket {
                t: mt,
                sum: 0.0,
                avg: None,
                max: 0.0,
                cpu: 0.0,
                gpu: 0.0,
                n: 0,
            });
        }
        if let Some(m) = self.s.minutes.last_mut() {
            m.sum += r.watts;
            m.n += 1;
            m.max = m.max.max(r.watts);
            m.cpu += r.parts.cpu.w;
            m.gpu += r.parts.gpu.w;
        }

        let evt = self.detect_spike(r, now, dt, price_ct);
        if now.saturating_sub(self.last_save) > 30_000 {
            self.save();
        }
        evt
    }

    fn detect_spike(
        &mut self,
        r: &ComputedReading,
        now: u64,
        dt: f64,
        price_ct: f64,
    ) -> Option<Spike> {
        let len = self.live.len();
        if len < 16 {
            return None;
        }
        let start_idx = len.saturating_sub(BASE_WINDOW + 1);
        let hist: Vec<f64> = self.live[start_idx..len - 1].iter().map(|x| x.w).collect();
        if hist.len() < 15 {
            return None;
        }

        let base = if let Some(idx) = self.active_spike {
            self.s
                .spikes
                .get(idx)
                .map(|sp| sp.base)
                .unwrap_or_else(|| median(&hist))
        } else {
            median(&hist)
        };
        let thr = base + (base * 0.35).max(60.0);

        if self.active_spike.is_none() {
            if r.watts > thr {
                self.above += 1;
            } else {
                self.above = 0;
            }
            if self.above >= 2 {
                let prev_t = self
                    .live
                    .get(len.saturating_sub(2))
                    .map(|p| p.t)
                    .unwrap_or(now);
                let end_cpu = len.saturating_sub(3).max(start_idx);
                let base_cpu = median(
                    &self.live[start_idx..end_cpu]
                        .iter()
                        .map(|x| x.cpu)
                        .collect::<Vec<_>>(),
                );
                let base_gpu = median(
                    &self.live[start_idx..end_cpu]
                        .iter()
                        .map(|x| x.gpu)
                        .collect::<Vec<_>>(),
                );

                let sp = Spike {
                    start: prev_t,
                    end: now,
                    base,
                    peak: r.watts,
                    peak_t: now,
                    sum: 0.0,
                    n: 0,
                    extra_wh: 0.0,
                    extra_ct: 0.0,
                    seen: false,
                    cpu_rise: 0.0,
                    gpu_rise: 0.0,
                    procs: r.top.clone(),
                    fg: r.fg.clone(),
                    base_cpu,
                    base_gpu,
                };
                self.s.spikes.push(sp);
                let idx = self.s.spikes.len() - 1;
                self.active_spike = Some(idx);
                self.above = 0;
                self.accumulate(idx, r, now, dt, price_ct);
                return self.s.spikes.get(idx).cloned();
            }
            return None;
        }

        let idx = self.active_spike.unwrap();
        self.accumulate(idx, r, now, dt, price_ct);
        if r.watts < base + (thr - base) / 2.0 {
            self.calm += 1;
        } else {
            self.calm = 0;
        }
        if self.calm >= 3 {
            let done = self.s.spikes.get(idx).cloned();
            self.active_spike = None;
            self.calm = 0;
            return done;
        }
        None
    }

    fn accumulate(
        &mut self,
        idx: usize,
        r: &ComputedReading,
        now: u64,
        dt: f64,
        price_ct: f64,
    ) {
        if let Some(sp) = self.s.spikes.get_mut(idx) {
            sp.end = now;
            sp.sum += r.watts;
            sp.n += 1;
            if dt <= MAX_DT {
                let extra = (r.watts - sp.base).max(0.0) * dt / 3600.0;
                sp.extra_wh += extra;
                sp.extra_ct += (extra / 1000.0) * price_ct;
            }
            if r.watts >= sp.peak {
                sp.peak = r.watts;
                sp.peak_t = now;
                if !r.top.is_empty() {
                    sp.procs = r.top.clone();
                }
                if r.fg.is_some() {
                    sp.fg = r.fg.clone();
                }
            }
            sp.cpu_rise = sp.cpu_rise.max(r.parts.cpu.w - sp.base_cpu);
            sp.gpu_rise = sp.gpu_rise.max(r.parts.gpu.w - sp.base_gpu);
        }
    }

    pub fn mark_seen(&mut self) {
        for sp in &mut self.s.spikes {
            sp.seen = true;
        }
    }

    pub fn unseen(&self) -> usize {
        self.s.spikes.iter().filter(|x| !x.seen).count()
    }

    pub fn save(&mut self) {
        self.last_save = now_ms();
        let tmp = self.current_file.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_string(&self.s) {
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.current_file);
            }
        }
    }

    pub fn summary_of(&self, s: &SessionState, reason: &str) -> SessionSummary {
        let dur_s = ((s.last.saturating_sub(s.start)) as f64 / 1000.0).max(1.0);
        let mut top_entries: Vec<(&String, &f64)> = s.top_procs.iter().collect();
        top_entries.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top = top_entries
            .into_iter()
            .take(5)
            .map(|(n, v)| TopProcShare {
                n: n.clone(),
                share: *v / dur_s,
            })
            .collect();

        let mut big_spikes = s.spikes.clone();
        big_spikes.sort_by(|a, b| {
            b.extra_ct
                .partial_cmp(&a.extra_ct)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        big_spikes.truncate(5);

        SessionSummary {
            id: s.id.clone(),
            start: s.start,
            end: s.last,
            reason: reason.to_string(),
            dur_s,
            wh: s.wh,
            cost: s.cost,
            avg_w: s.wh / (dur_s / 3600.0),
            wh_measured: s.wh_measured,
            wh_estimated: s.wh_estimated,
            peak: s.peak.clone(),
            spikes: s.spikes.len(),
            big_spikes,
            top,
        }
    }

    pub fn write_final(&self, s: &SessionState, reason: &str) -> SessionSummary {
        let summary = self.summary_of(s, reason);
        let file = self.sessions_dir.join(format!("{}.json", s.id));
        let archive = SessionFileArchive {
            summary: summary.clone(),
            minutes: s.minutes.iter().map(compact_minute).collect(),
            spikes: s.spikes.clone(),
        };
        if let Ok(json) = serde_json::to_string(&archive) {
            let _ = fs::write(file, json);
        }
        summary
    }

    pub fn finalize(&mut self, reason: &str) -> SessionSummary {
        let summary = self.write_final(&self.s, reason);
        let _ = fs::remove_file(&self.current_file);
        summary
    }

    /// Running time of the current session in seconds (cheap, for per-frame labels).
    pub fn dur_s(&self) -> f64 {
        (self.s.last.saturating_sub(self.s.start)) as f64 / 1000.0
    }

    pub fn current_summary(&self) -> SessionSummary {
        self.summary_of(&self.s, "l\u{00e4}uft")
    }

    pub fn list_sessions(&self, limit: usize) -> Vec<SessionSummary> {
        let mut files: Vec<PathBuf> = fs::read_dir(&self.sessions_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        files
            .into_iter()
            .take(limit)
            .filter_map(|p| {
                let raw = fs::read_to_string(p).ok()?;
                let arch: SessionFileArchive = serde_json::from_str(&raw).ok()?;
                Some(arch.summary)
            })
            .collect()
    }

    pub fn load_session_detail(&self, id: &str) -> Option<SessionFileArchive> {
        let file = self.sessions_dir.join(format!("{}.json", id));
        let raw = fs::read_to_string(file).ok()?;
        serde_json::from_str(&raw).ok()
    }
}
