//! All running processes with CPU time and working set from one system call
//! (`NtQuerySystemInformation(SystemProcessInformation)`, what Task Manager uses).
//!
//! sysinfo opens every process on its own for the same numbers. With ~450 processes that cost
//! about 900 thread wakeups a second at one sample per 1.8 s; this costs about 3.

use std::collections::HashMap;
use std::time::Instant;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};

// The offsets below are those of the 64-bit SYSTEM_PROCESS_INFORMATION.
const _: () = assert!(std::mem::size_of::<usize>() == 8, "procs.rs reads the 64-bit layout");
const OFF_CREATE_TIME: usize = 32;
const OFF_USER_TIME: usize = 40;
const OFF_KERNEL_TIME: usize = 48;
const OFF_NAME_LEN: usize = 56; // UNICODE_STRING.Length in bytes
const OFF_NAME_BUF: usize = 64; // UNICODE_STRING.Buffer
const OFF_PID: usize = 80;
const OFF_WORKING_SET: usize = 144;

pub struct ProcInfo {
    pub pid: u32,
    /// Image name as Windows reports it, e.g. "chrome.exe"; empty for the idle process.
    pub name: String,
    /// CPU since the last sample in percent of ONE core (like sysinfo's `cpu_usage`).
    pub cpu: f64,
    /// Working set in bytes (like sysinfo's `memory`).
    pub mem: u64,
}

pub struct ProcTable {
    buf: Vec<u8>,
    /// (pid, creation time) -> CPU time in 100 ns units. The creation time keeps a reused pid
    /// from inheriting the CPU time of the process that had it before.
    last: HashMap<(u32, u64), u64>,
    last_at: Instant,
}

impl ProcTable {
    pub fn new() -> Self {
        let mut t = ProcTable { buf: vec![0u8; 512 * 1024], last: HashMap::new(), last_at: Instant::now() };
        let _ = t.sample(); // baseline, so the first real sample has CPU deltas
        t
    }

    pub fn sample(&mut self) -> Vec<ProcInfo> {
        if !self.query() {
            return Vec::new();
        }
        let now = Instant::now();
        let wall_100ns = now.duration_since(self.last_at).as_secs_f64() * 1e7;
        self.last_at = now;

        let mut out = Vec::with_capacity(self.last.len().max(256));
        let mut seen = HashMap::with_capacity(self.last.len().max(256));
        let mut off = 0usize;
        // SAFETY: `query` filled `buf` with a chain of SYSTEM_PROCESS_INFORMATION entries;
        // each NextEntryOffset stays inside the buffer and the name points into it.
        unsafe {
            loop {
                let p = self.buf.as_ptr().add(off);
                let u64_at = |o: usize| (p.add(o) as *const u64).read_unaligned();
                let next = (p as *const u32).read_unaligned() as usize;
                let name_len = (p.add(OFF_NAME_LEN) as *const u16).read_unaligned() as usize / 2;
                let name_ptr = u64_at(OFF_NAME_BUF) as *const u16;
                let name = if name_ptr.is_null() || name_len == 0 {
                    String::new()
                } else {
                    String::from_utf16_lossy(std::slice::from_raw_parts(name_ptr, name_len))
                };
                let pid = u64_at(OFF_PID) as u32;
                let key = (pid, u64_at(OFF_CREATE_TIME));
                let cpu_time = u64_at(OFF_USER_TIME) + u64_at(OFF_KERNEL_TIME);
                let cpu = match self.last.get(&key) {
                    Some(&before) if wall_100ns > 0.0 => cpu_time.saturating_sub(before) as f64 / wall_100ns * 100.0,
                    _ => 0.0,
                };
                seen.insert(key, cpu_time);
                out.push(ProcInfo { pid, name, cpu, mem: u64_at(OFF_WORKING_SET) });
                if next == 0 || off + next >= self.buf.len() {
                    break;
                }
                off += next;
            }
        }
        // Only processes still alive stay in the table.
        self.last = seen;
        out
    }

    /// Fills `buf`, growing it when the process list no longer fits.
    fn query(&mut self) -> bool {
        for _ in 0..4 {
            let mut need = 0u32;
            let status = unsafe {
                NtQuerySystemInformation(SystemProcessInformation, self.buf.as_mut_ptr().cast(), self.buf.len() as u32, &mut need)
            };
            if status.is_ok() {
                return true;
            }
            // STATUS_INFO_LENGTH_MISMATCH: room for what is needed now plus a margin for new processes.
            self.buf.resize((need as usize).max(self.buf.len()) + 128 * 1024, 0);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_itself_with_memory_and_cpu() {
        let mut t = ProcTable::new();
        let start = Instant::now();
        let mut x = 0u64;
        while start.elapsed().as_millis() < 300 {
            x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
        }
        let me = std::process::id();
        let list = t.sample();
        let own = list.iter().find(|p| p.pid == me).expect("own process in the list");
        assert!(own.name.to_lowercase().ends_with(".exe"), "name: {}", own.name);
        assert!(own.mem > 1024 * 1024, "working set: {}", own.mem);
        // 300 ms busy on one core is close to 100 % of a core; allow for scheduling noise.
        assert!(own.cpu > 30.0 && own.cpu < 400.0, "cpu: {}", own.cpu);
        assert!(list.len() > 20);
    }
}
