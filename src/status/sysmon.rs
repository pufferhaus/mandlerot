//! Cheap, periodic CPU / RAM / temperature sampler for the status panel.
//!
//! Lives in the status worker thread because that's the only consumer.
//! Reading `/proc/stat`, `/proc/meminfo`, and the thermal-zone sysfs file
//! costs ~50 µs total on a Pi (tmpfs, no disk I/O) — we cap at 1 Hz so
//! even that vanishes from the per-tick budget.
//!
//! All fields are `Option<f32>` so the panel can render `---` when a
//! source isn't available (e.g. desktop dev runs without a thermal zone).

use std::time::{Duration, Instant};

pub struct SysMon {
    sample_interval: Duration,
    last_sampled: Option<Instant>,
    /// Previous /proc/stat aggregate, for delta-based CPU%.
    prev_cpu: Option<CpuTotals>,
    pub cpu_pct: Option<f32>,
    pub mem_pct: Option<f32>,
    pub temp_c: Option<f32>,
}

#[derive(Clone, Copy)]
struct CpuTotals {
    total: u64,
    idle: u64,
}

impl SysMon {
    pub fn new() -> Self {
        Self {
            sample_interval: Duration::from_millis(1000),
            last_sampled: None,
            prev_cpu: None,
            cpu_pct: None,
            mem_pct: None,
            temp_c: None,
        }
    }

    /// Refresh the readings if at least `sample_interval` has elapsed
    /// since the last sample. Safe to call every status tick; no-ops
    /// inside the interval.
    pub fn maybe_sample(&mut self, now: Instant) {
        if let Some(prev) = self.last_sampled {
            if now.duration_since(prev) < self.sample_interval {
                return;
            }
        }
        self.last_sampled = Some(now);
        self.sample_cpu();
        self.sample_mem();
        self.sample_temp();
    }

    fn sample_cpu(&mut self) {
        let Some(totals) = read_cpu_totals() else {
            self.cpu_pct = None;
            return;
        };
        if let Some(prev) = self.prev_cpu {
            let dt_total = totals.total.saturating_sub(prev.total);
            let dt_idle = totals.idle.saturating_sub(prev.idle);
            self.cpu_pct = if dt_total > 0 {
                let busy = dt_total.saturating_sub(dt_idle) as f32;
                Some((busy / dt_total as f32) * 100.0)
            } else {
                None
            };
        }
        self.prev_cpu = Some(totals);
    }

    fn sample_mem(&mut self) {
        self.mem_pct = read_mem_pct();
    }

    fn sample_temp(&mut self) {
        self.temp_c = read_temp_c();
    }
}

impl Default for SysMon {
    fn default() -> Self {
        Self::new()
    }
}

/// Sum CPU times from /proc/stat's first `cpu` line. Returns total ticks
/// and idle-equivalent ticks (idle + iowait, the standard "free" buckets).
fn read_cpu_totals() -> Option<CpuTotals> {
    let s = std::fs::read_to_string("/proc/stat").ok()?;
    let first = s.lines().next()?;
    // Format: `cpu user nice system idle iowait irq softirq steal guest guest_nice`
    let mut parts = first.split_ascii_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let mut nums = [0u64; 8];
    for slot in &mut nums {
        match parts.next() {
            Some(t) => *slot = t.parse().ok()?,
            None => break, // some kernels emit fewer fields; partial sum still works
        }
    }
    let idle = nums[3].saturating_add(nums[4]); // idle + iowait
    let total = nums.iter().sum();
    Some(CpuTotals { total, idle })
}

fn read_mem_pct() -> Option<f32> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    for line in s.lines() {
        let mut parts = line.split_ascii_whitespace();
        let (Some(key), Some(val)) = (parts.next(), parts.next()) else {
            continue;
        };
        let parsed = val.parse::<u64>().ok();
        match key {
            "MemTotal:" => total_kb = parsed,
            "MemAvailable:" => avail_kb = parsed,
            _ => {}
        }
        if total_kb.is_some() && avail_kb.is_some() {
            break;
        }
    }
    let (t, a) = (total_kb?, avail_kb?);
    if t == 0 {
        return None;
    }
    let used = t.saturating_sub(a) as f32;
    Some((used / t as f32) * 100.0)
}

fn read_temp_c() -> Option<f32> {
    // Pi's standard thermal zone. millidegrees Celsius.
    let s = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    let n: f32 = s.trim().parse().ok()?;
    Some(n / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sysmon_starts_empty() {
        let s = SysMon::new();
        assert!(s.cpu_pct.is_none());
        assert!(s.mem_pct.is_none());
        assert!(s.temp_c.is_none());
    }

    #[test]
    fn maybe_sample_respects_interval() {
        // Two rapid samples should be a no-op past the first.
        let mut s = SysMon::new();
        let t0 = Instant::now();
        s.maybe_sample(t0);
        let first_stamp = s.last_sampled;
        s.maybe_sample(t0 + Duration::from_millis(10));
        // Stamp should not have moved.
        assert_eq!(s.last_sampled, first_stamp);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_sample_populates_at_least_one_source() {
        let mut s = SysMon::new();
        // Two samples 1.1s apart to get a CPU delta.
        let t0 = Instant::now();
        s.maybe_sample(t0);
        std::thread::sleep(Duration::from_millis(20));
        s.maybe_sample(t0 + Duration::from_millis(1100));
        // /proc/stat and /proc/meminfo always exist on Linux. Thermal
        // zone may not (CI containers); only require one of the three.
        assert!(s.cpu_pct.is_some() || s.mem_pct.is_some());
    }
}
