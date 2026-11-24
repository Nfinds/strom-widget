//! The screenshot self-test (`--self-test-screenshots <dir>`) and the example data for it
//! (`--demo-data`).

use crate::{sensors, ActiveTab, StromWidgetApp};
use eframe::egui::{self, ViewportCommand};
use std::sync::Arc;
use std::time::{Duration, Instant};

impl StromWidgetApp {
    pub(crate) fn handle_self_test(&mut self, ctx: &egui::Context) {
        let Some(out_dir) = self.self_test_dir.clone() else {
            return;
        };
        const SHOTS: [&str; 9] = [
            "compact",
            "live",
            "spikes",
            "history",
            "history-bottom",
            "network",
            "settings",
            "settings-bottom",
            "system",
        ];

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
                4 => self.self_test_scroll = Some(420.0),
                5 => {
                    self.self_test_scroll = Some(0.0);
                    self.active_tab = ActiveTab::Net;
                }
                6 => self.active_tab = ActiveTab::Settings,
                7 => self.self_test_scroll = Some(260.0),
                8 => {
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
}

/// Example system data for screenshots that must not show the real machine's ports and programs.
pub(crate) fn apply_demo_system_data(v: &mut sensors::SysMonView) {
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
