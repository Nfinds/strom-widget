// Native Windows Hardware Sensors + System Add-on (Ports, HTTP Web UI probing, RAM, Network).
// Zero PowerShell subprocesses, zero nvidia-smi loops!

use crate::power::{RawSensorSample, TopProcSample};
use nvml_wrapper::Nvml;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::procs::ProcTable;
use sysinfo::{MemoryRefreshKind, Networks, System};

/// Ticks (~1.8 s each) between port scans while nobody looks at the System tab; often enough
/// to keep the port count on the tab roughly current.
const PORT_SCAN_IDLE_TICKS: u64 = 32;
/// Ticks between port scans while the System tab is open.
const PORT_SCAN_OPEN_TICKS: u64 = 4;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID, MIB_TCP_STATE_LISTEN,
    TCP_TABLE_OWNER_PID_LISTENER,
};
use windows::Win32::Networking::WinSock::AF_INET;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamGroup {
    pub n: String,
    pub title: String,
    pub count: usize,
    pub mem: u64,
    pub cpu: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebServiceInfo {
    pub port: u16,
    pub pid: u32,
    pub name: String,
    pub title: String,
    pub url: String,
    pub lan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningPortEntry {
    pub port: u16,
    pub pid: u32,
    pub name: String,
    pub addr: String,
    pub is_web: bool,
    pub web_title: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SysMonView {
    pub cpu: f64,
    pub ram: f64,
    pub down: f64,
    pub up: f64,
    /// Name of the network interface with the most traffic right now.
    #[serde(default)]
    pub net_if: String,
    pub top: Vec<RamGroup>,
    pub web: Vec<WebServiceInfo>,
    pub all_ports: Vec<ListeningPortEntry>,
}

struct CachedHttpProbe {
    at: Instant,
    res: Option<(u16, String)>, // (status, title)
    pending: bool,
}

pub struct HardwareSampler {
    sys: System,
    /// Processes come from one system call instead of sysinfo (see procs.rs).
    procs: ProcTable,
    networks: Networks,
    nvml: Option<Nvml>,
    pdh_query: isize,
    rapl_counter: Option<isize>,
    util_counter: Option<isize>,
    last_net_time: Instant,
    tick_count: u64,
    /// Whether the previous tick had the System tab open (scan at once when it opens).
    ports_were_wanted: bool,
    http_cache: Arc<Mutex<HashMap<(u32, u16), CachedHttpProbe>>>,
    pub last_sys_view: SysMonView,
}

unsafe impl Send for HardwareSampler {}

impl HardwareSampler {
    pub fn new() -> Self {
        // sysinfo only for CPU load, core count and total memory; processes come from ProcTable.
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory_specifics(MemoryRefreshKind::new().with_ram());
        let networks = Networks::new_with_refreshed_list();
        let nvml = Nvml::init().ok();

        let mut pdh_query: isize = 0;
        let mut rapl_counter: Option<isize> = None;
        let mut util_counter: Option<isize> = None;

        unsafe {
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut pdh_query) == 0 {
                let mut c_rapl: isize = 0;
                if PdhAddEnglishCounterW(
                    pdh_query,
                    w!("\\Energy Meter(rapl_package0_pkg)\\Power"),
                    0,
                    &mut c_rapl,
                ) == 0
                {
                    rapl_counter = Some(c_rapl);
                }
                let mut c_util: isize = 0;
                if PdhAddEnglishCounterW(
                    pdh_query,
                    w!("\\Processor Information(_Total)\\% Processor Utility"),
                    0,
                    &mut c_util,
                ) == 0
                {
                    util_counter = Some(c_util);
                }
                let _ = PdhCollectQueryData(pdh_query);
            }
        }

        Self {
            sys,
            procs: ProcTable::new(),
            networks,
            nvml,
            pdh_query,
            rapl_counter,
            util_counter,
            last_net_time: Instant::now(),
            tick_count: 0,
            ports_were_wanted: false,
            http_cache: Arc::new(Mutex::new(HashMap::new())),
            last_sys_view: SysMonView::default(),
        }
    }

    /// One measurement. `ports_wanted`: the System tab is open, so scan ports more often.
    pub fn sample(&mut self, sys_addon: bool, ports_wanted: bool) -> RawSensorSample {
        // 1. CPU load and memory; the processes follow in step 4
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory_specifics(MemoryRefreshKind::new().with_ram());

        // 2. Query PDH (RAPL Power in mW -> W, and Processor Utility %)
        let mut cpu_w: Option<f64> = None;
        let mut pdh_util: Option<f64> = None;

        if self.pdh_query != 0 {
            unsafe {
                if PdhCollectQueryData(self.pdh_query) == 0 {
                    if let Some(c_rapl) = self.rapl_counter {
                        let mut val = PDH_FMT_COUNTERVALUE::default();
                        if PdhGetFormattedCounterValue(c_rapl, PDH_FMT_DOUBLE, None, &mut val) == 0
                        {
                            let mw = val.Anonymous.doubleValue;
                            if mw > 0.1 && mw < 500_000.0 {
                                cpu_w = Some((mw / 1000.0 * 100.0).round() / 100.0);
                            }
                        }
                    }
                    if let Some(c_util) = self.util_counter {
                        let mut val = PDH_FMT_COUNTERVALUE::default();
                        if PdhGetFormattedCounterValue(c_util, PDH_FMT_DOUBLE, None, &mut val) == 0
                        {
                            let u = val.Anonymous.doubleValue;
                            if u >= 0.0 && u.is_finite() {
                                pdh_util = Some((u * 10.0).round() / 10.0);
                            }
                        }
                    }
                }
            }
        }

        let sys_cpu_util = self.sys.global_cpu_usage() as f64;
        let util = pdh_util.unwrap_or(sys_cpu_util).clamp(0.0, 100.0);

        // 3. Query GPU via NVML
        let mut gpu_w: Option<f64> = None;
        let mut gpu_util: Option<f64> = None;
        let mut gpu_temp: Option<f64> = None;
        let nv_available = self.nvml.is_some();

        if let Some(ref nvml) = self.nvml {
            if let Ok(dev) = nvml.device_by_index(0) {
                if let Ok(mw) = dev.power_usage() {
                    gpu_w = Some((mw as f64) / 1000.0);
                }
                if let Ok(rates) = dev.utilization_rates() {
                    gpu_util = Some(rates.gpu as f64);
                }
                if let Ok(temp) =
                    dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                {
                    gpu_temp = Some(temp as f64);
                }
            }
        }

        // 4. Aggregate processes by name for CPU & RAM
        let num_cores = self.sys.cpus().len().max(1) as f64;
        let skip_names: HashSet<&str> = [
            "Idle",
            "System",
            "_Total",
            "Memory Compression",
            "Registry",
            "svchost",
            "csrss",
            "smss",
            "wininit",
            "services",
            "lsass",
        ]
        .into_iter()
        .collect();

        let mut cpu_by_name: HashMap<String, f64> = HashMap::new();
        let mut mem_by_name: HashMap<String, (usize, u64)> = HashMap::new();
        let mut pid_to_name: HashMap<u32, String> = HashMap::new();

        for proc_info in self.procs.sample() {
            if proc_info.name.is_empty() {
                continue; // the idle process
            }
            let raw_name = proc_info.name;
            let clean_name = raw_name
                .strip_suffix(".exe")
                .or_else(|| raw_name.strip_suffix(".EXE"))
                .unwrap_or(&raw_name)
                .to_string();
            pid_to_name.insert(proc_info.pid, clean_name.clone());

            if skip_names.contains(clean_name.as_str()) {
                continue;
            }
            let c = proc_info.cpu / num_cores;
            if c > 0.05 {
                *cpu_by_name.entry(clean_name.clone()).or_insert(0.0) += c;
            }
            let m = proc_info.mem;
            if m > 0 {
                let entry = mem_by_name.entry(clean_name).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += m;
            }
        }

        let mut top_vec: Vec<TopProcSample> = cpu_by_name
            .iter()
            .filter(|(_, &v)| v > 0.2)
            .map(|(k, &v)| TopProcSample {
                n: k.clone(),
                c: (v * 10.0).round() / 10.0,
            })
            .collect();
        top_vec.sort_by(|a, b| b.c.partial_cmp(&a.c).unwrap_or(std::cmp::Ordering::Equal));
        top_vec.truncate(4);

        // 5. Foreground window process
        let fg = unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                None
            } else {
                let mut fpid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut fpid));
                if fpid != 0 {
                    pid_to_name.get(&fpid).cloned()
                } else {
                    None
                }
            }
        };

        // 6. System Add-on (RAM, Network, Listening TCP Ports & Web UIs)
        if sys_addon {
            let now = Instant::now();
            let dt = now
                .duration_since(self.last_net_time)
                .as_secs_f64()
                .max(0.2);
            self.last_net_time = now;
            self.networks.refresh();

            let mut best_down = 0.0;
            let mut best_up = 0.0;
            let mut best_if = None;
            for (if_name, net_data) in &self.networks {
                let d = (net_data.received() as f64) / dt;
                let u = (net_data.transmitted() as f64) / dt;
                if d + u > best_down + best_up {
                    best_down = d;
                    best_up = u;
                    best_if = Some(if_name);
                }
            }
            // Keep the last name while idle, so the label does not flicker.
            if let Some(name) = best_if {
                if self.last_sys_view.net_if != *name {
                    self.last_sys_view.net_if = name.clone();
                }
            }

            let total_mem = self.sys.total_memory().max(1) as f64;
            let used_mem = self.sys.used_memory() as f64;
            let ram_pct = ((used_mem / total_mem) * 100.0).clamp(0.0, 100.0);

            self.last_sys_view.cpu = util;
            self.last_sys_view.ram = ram_pct;
            self.last_sys_view.down = best_down;
            self.last_sys_view.up = best_up;

            // Every 2 ticks (~4s), update top RAM consumers
            if self.tick_count.is_multiple_of(2) {
                let mut ram_groups: Vec<RamGroup> = mem_by_name
                    .into_iter()
                    .map(|(n, (count, mem))| {
                        let c = cpu_by_name.get(&n).copied().unwrap_or(0.0);
                        RamGroup {
                            title: friendly_process_title(&n),
                            n,
                            count,
                            mem,
                            cpu: (c * 10.0).round() / 10.0,
                        }
                    })
                    .collect();
                ram_groups.sort_by_key(|g| std::cmp::Reverse(g.mem));
                ram_groups.truncate(8);
                self.last_sys_view.top = ram_groups;
            }

            // Listening TCP ports & HTTP web UI probes: every ~7 s while the System tab is
            // open, otherwise about once a minute.
            let every = if ports_wanted { PORT_SCAN_OPEN_TICKS } else { PORT_SCAN_IDLE_TICKS };
            if self.tick_count.is_multiple_of(every) || (ports_wanted && !self.ports_were_wanted) {
                self.refresh_ports_and_web(&pid_to_name);
            }
        }

        self.tick_count += 1;
        self.ports_were_wanted = ports_wanted;

        RawSensorSample {
            cpu_w,
            util,
            gpu_w,
            gpu_util,
            gpu_temp,
            rapl_available: self.rapl_counter.is_some(),
            nv_available,
            top: top_vec,
            fg,
        }
    }

    fn refresh_ports_and_web(&mut self, pid_to_name: &HashMap<u32, String>) {
        let raw_listeners = get_listening_tcp_ports();
        let no_probe_ports: HashSet<u16> = [
            135, 139, 445, 3389, 5040, 7680, 49664, 49665, 49666, 49667, 49668, 49669,
        ]
        .into_iter()
        .collect();
        let no_probe_names: HashSet<&str> =
            ["svchost", "lsass", "wininit", "services", "spoolsv", "system"]
                .into_iter()
                .collect();

        let my_pid = std::process::id();
        let mut dedup: HashMap<(u32, u16), (Ipv4Addr, String)> = HashMap::new();

        for (addr, port, pid) in raw_listeners {
            if pid <= 4 || pid == my_pid || no_probe_ports.contains(&port) {
                continue;
            }
            let pname = pid_to_name
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| format!("PID {}", pid));
            if no_probe_names.contains(pname.to_lowercase().as_str()) {
                continue;
            }
            let key = (pid, port);
            let replace = match dedup.get(&key) {
                None => true,
                Some((prev_addr, _)) => {
                    !prev_addr.is_unspecified() && !prev_addr.is_loopback()
                        && (addr.is_unspecified() || addr.is_loopback())
                }
            };
            if replace {
                dedup.insert(key, (addr, pname));
            }
        }

        let now = Instant::now();
        let hit_ttl = Duration::from_secs(180);
        let miss_ttl = Duration::from_secs(45);

        {
            let mut cache = self.http_cache.lock().unwrap();
            for (&(pid, port), &(addr, _)) in &dedup {
                let key = (pid, port);
                let should_probe = match cache.get(&key) {
                    None => true,
                    Some(entry) => {
                        if entry.pending {
                            false
                        } else {
                            let ttl = if entry.res.is_some() { hit_ttl } else { miss_ttl };
                            now.duration_since(entry.at) >= ttl
                        }
                    }
                };
                if should_probe {
                    let prev_res = cache.get(&key).and_then(|e| e.res.clone());
                    cache.insert(
                        key,
                        CachedHttpProbe {
                            at: now,
                            res: prev_res,
                            pending: true,
                        },
                    );
                    let cache_clone = Arc::clone(&self.http_cache);
                    let probe_ip = if addr.is_unspecified() || addr.is_loopback() {
                        Ipv4Addr::new(127, 0, 0, 1)
                    } else {
                        addr
                    };
                    std::thread::spawn(move || {
                        let res = probe_http_port(probe_ip, port);
                        if let Ok(mut c) = cache_clone.lock() {
                            c.insert(
                                key,
                                CachedHttpProbe {
                                    at: Instant::now(),
                                    res,
                                    pending: false,
                                },
                            );
                        }
                    });
                }
            }
            cache.retain(|k, _| dedup.contains_key(k));
        }

        // Rebuild web list & all_ports list
        let cache = self.http_cache.lock().unwrap();
        let mut web_list = Vec::new();
        let mut all_ports = Vec::new();

        for (&(pid, port), (addr, pname)) in &dedup {
            let probe_res = cache.get(&(pid, port)).and_then(|c| c.res.clone());
            let lan = !addr.is_loopback();
            let host_str = if addr.is_unspecified() || addr.is_loopback() {
                "localhost".to_string()
            } else {
                addr.to_string()
            };

            let mut is_web = false;
            let mut web_title = None;
            let mut web_url = None;

            if let Some((status, title)) = probe_res {
                if status < 400 {
                    let url = format!("http://{}:{}/", host_str, port);
                    is_web = true;
                    web_title = Some(title.clone());
                    web_url = Some(url.clone());
                    web_list.push(WebServiceInfo {
                        port,
                        pid,
                        name: pname.clone(),
                        title,
                        url,
                        lan,
                    });
                }
            }

            all_ports.push(ListeningPortEntry {
                port,
                pid,
                name: pname.clone(),
                addr: addr.to_string(),
                is_web,
                web_title,
                web_url,
            });
        }

        web_list.sort_by(|a, b| {
            let has_t_b = !b.title.is_empty();
            let has_t_a = !a.title.is_empty();
            has_t_b.cmp(&has_t_a).then_with(|| a.port.cmp(&b.port))
        });
        all_ports.sort_by(|a, b| b.is_web.cmp(&a.is_web).then_with(|| a.port.cmp(&b.port)));

        self.last_sys_view.web = web_list;
        self.last_sys_view.all_ports = all_ports;
    }
}

impl Drop for HardwareSampler {
    fn drop(&mut self) {
        if self.pdh_query != 0 {
            unsafe {
                let _ = PdhCloseQuery(self.pdh_query);
            }
        }
    }
}

fn friendly_process_title(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "brave" => "Brave Browser".to_string(),
        "chrome" => "Google Chrome".to_string(),
        "msedge" => "Microsoft Edge".to_string(),
        "msedgewebview2" => "Edge WebView2".to_string(),
        "firefox" => "Mozilla Firefox".to_string(),
        "code" | "vscodium" => "VS Code".to_string(),
        "explorer" => "Windows Explorer".to_string(),
        "discord" => "Discord".to_string(),
        "telegram" => "Telegram Desktop".to_string(),
        "steam" | "steamwebhelper" => "Steam".to_string(),
        "claude" => "Claude CLI".to_string(),
        "ollama" => "Ollama AI Server".to_string(),
        _ => name.to_string(),
    }
}

fn get_listening_tcp_ports() -> Vec<(Ipv4Addr, u16, u32)> {
    let mut results = Vec::new();
    unsafe {
        let mut size: u32 = 0;
        let _ = GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if size == 0 {
            return results;
        }
        let mut buf = vec![0u8; size as usize];
        let ret = GetExtendedTcpTable(
            Some(buf.as_mut_ptr() as *mut _),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
        if ret != 0 {
            return results;
        }
        let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
        let num_entries = table.dwNumEntries as usize;
        let rows_ptr = table.table.as_ptr();
        for i in 0..num_entries {
            let row: &MIB_TCPROW_OWNER_PID = &*rows_ptr.add(i);
            if row.dwState == MIB_TCP_STATE_LISTEN.0 as u32 {
                let ip = Ipv4Addr::from(u32::from_be(row.dwLocalAddr));
                let port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
                let pid = row.dwOwningPid;
                results.push((ip, port, pid));
            }
        }
    }
    results
}

fn probe_http_port(ip: Ipv4Addr, port: u16) -> Option<(u16, String)> {
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(650)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(650)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(650)));

    let req = format!(
        "GET / HTTP/1.0\r\nHost: localhost:{}\r\nUser-Agent: Strom-Widget-Rust\r\nAccept: */*\r\n\r\n",
        port
    );
    stream.write_all(req.as_bytes()).ok()?;

    let mut buf = vec![0u8; 16384];
    let mut total = 0;
    while total < buf.len() {
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                let s = String::from_utf8_lossy(&buf[..total]);
                if s.to_lowercase().contains("</title>") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if total == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&buf[..total]);
    let first_line = text.lines().next()?;
    if !first_line.starts_with("HTTP/") {
        return None;
    }
    let status_str = first_line.split_whitespace().nth(1)?;
    let status: u16 = status_str.parse().ok()?;

    let mut title = String::new();
    let lower = text.to_lowercase();
    if let Some(start_tag) = lower.find("<title") {
        if let Some(gt) = lower[start_tag..].find('>') {
            let content_start = start_tag + gt + 1;
            if let Some(end_tag) = lower[content_start..].find("</title>") {
                let raw_t = &text[content_start..content_start + end_tag];
                title = raw_t
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(100)
                    .collect();
            }
        }
    }
    Some((status, title))
}
