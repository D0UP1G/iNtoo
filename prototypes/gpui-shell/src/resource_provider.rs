//! Bounded Linux `/proc` resource snapshot adapter for the ii vertical bar.
//! Reads only procfs and keeps no unbounded history; GPU metrics require a
//! separate supported provider and are intentionally reported unavailable.

use std::fs;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceSnapshot {
    pub memory_percent: Option<f64>,
    pub swap_percent: Option<f64>,
    pub cpu_percent: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct LinuxResourceProvider {
    previous_cpu: Option<(u64, u64)>,
}

impl LinuxResourceProvider {
    pub fn sample(&mut self) -> ResourceSnapshot {
        let cpu_current = read_cpu_totals();
        let cpu_percent = match (self.previous_cpu, cpu_current) {
            (Some((old_total, old_idle)), Some((total, idle))) => {
                let total_delta = total.saturating_sub(old_total);
                let idle_delta = idle.saturating_sub(old_idle);
                (total_delta > 0).then(|| {
                    (100.0 * total_delta.saturating_sub(idle_delta) as f64 / total_delta as f64)
                        .clamp(0.0, 100.0)
                })
            }
            _ => None,
        };
        if let Some(cpu) = cpu_current {
            self.previous_cpu = Some(cpu);
        }
        let (memory_percent, swap_percent) = read_memory();
        ResourceSnapshot {
            memory_percent,
            swap_percent,
            cpu_percent,
        }
    }

    pub fn recommended_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3)
    }
}

fn read_cpu_totals() -> Option<(u64, u64)> {
    let line = fs::read_to_string("/proc/stat").ok()?;
    let cpu = line.lines().find(|line| line.starts_with("cpu "))?;
    let values = cpu
        .split_ascii_whitespace()
        .skip(1)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() < 4 {
        return None;
    }
    // Linux appends guest and guest_nice to /proc/stat even though they are
    // already included in user/nice. Exclude them to avoid double counting.
    let total = values.iter().take(8).copied().sum();
    let idle = values[3].saturating_add(values.get(4).copied().unwrap_or(0));
    Some((total, idle))
}

fn read_memory() -> (Option<f64>, Option<f64>) {
    let Ok(text) = fs::read_to_string("/proc/meminfo") else {
        return (None, None);
    };
    let mut values = std::collections::HashMap::new();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        if let Some(value) = rest
            .split_ascii_whitespace()
            .next()
            .and_then(|v| v.parse::<u64>().ok())
        {
            values.insert(key, value);
        }
    }
    let percentage = |total: Option<&u64>, available: Option<&u64>| {
        let total = *total?;
        let available = *available?;
        (total > 0).then(|| {
            (100.0 * total.saturating_sub(available) as f64 / total as f64).clamp(0.0, 100.0)
        })
    };
    let memory = percentage(values.get("MemTotal"), values.get("MemAvailable"));
    let swap = percentage(values.get("SwapTotal"), values.get("SwapFree"));
    (memory, swap)
}

pub fn format_percent(value: Option<f64>) -> Option<String> {
    value.map(|value| format!("{value:.0}%"))
}
