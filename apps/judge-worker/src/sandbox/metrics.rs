use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bollard::{Docker, query_parameters::StatsOptionsBuilder};
use futures_util::StreamExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GnuTimeMetrics {
    pub(super) cpu_time_ms: i32,
    pub(super) peak_memory_kb: i32,
}

pub(super) fn extract_gnu_time_metrics(logs: &str) -> (String, Option<GnuTimeMetrics>) {
    const PREFIX: &str = "__PROJECT_BALLOON_GNU_TIME__ ";
    let Some(marker_start) = logs.rfind(PREFIX) else {
        return (logs.to_owned(), None);
    };
    let fields_text = &logs[marker_start + PREFIX.len()..];
    let mut fields = fields_text.split_whitespace();
    let user_seconds = fields.next().and_then(|value| value.parse::<f64>().ok());
    let system_seconds = fields.next().and_then(|value| value.parse::<f64>().ok());
    let peak_memory_kb = fields.next().and_then(|value| value.parse::<i32>().ok());
    if fields.next().is_some() {
        return (logs.to_owned(), None);
    }
    let (Some(user_seconds), Some(system_seconds), Some(peak_memory_kb)) =
        (user_seconds, system_seconds, peak_memory_kb)
    else {
        return (logs.to_owned(), None);
    };
    let sanitized = logs[..marker_start].trim_end_matches(['\r', '\n']).to_owned();
    // GNU time 1.9 emits `%U` and `%S` with centisecond precision. Round each
    // field independently so floating-point addition cannot turn 120 + 30 ms
    // into a spurious 151 ms after applying a ceiling.
    let cpu_milliseconds = (user_seconds * 1_000.0).round() + (system_seconds * 1_000.0).round();
    if !cpu_milliseconds.is_finite()
        || cpu_milliseconds.is_sign_negative()
        || cpu_milliseconds > f64::from(i32::MAX)
        || peak_memory_kb < 0
    {
        return (logs.to_owned(), None);
    }
    (sanitized, Some(GnuTimeMetrics { cpu_time_ms: cpu_milliseconds as i32, peak_memory_kb }))
}

#[derive(Default)]
pub(super) struct ContainerResourceUsage {
    pub(super) peak_memory_bytes: AtomicU64,
    pub(super) cpu_time_ns: AtomicU64,
}

pub(super) async fn collect_resource_usage(
    docker: Docker,
    id: String,
    resource_usage: Arc<ContainerResourceUsage>,
) {
    let mut stats = docker
        .stats(&id, Some(StatsOptionsBuilder::default().stream(true).one_shot(false).build()));
    while let Some(sample) = stats.next().await {
        let Ok(sample) = sample else { return };
        if let Some(memory) = sample.memory_stats {
            let usage = memory.max_usage.or(memory.usage).unwrap_or(0);
            resource_usage.peak_memory_bytes.fetch_max(usage, Ordering::Relaxed);
        }
        if let Some(cpu_time_ns) =
            sample.cpu_stats.and_then(|stats| stats.cpu_usage).and_then(|usage| usage.total_usage)
        {
            resource_usage.cpu_time_ns.fetch_max(cpu_time_ns, Ordering::Relaxed);
        }
    }
}

pub(super) fn nonzero_milliseconds(nanoseconds: u64) -> Option<i32> {
    (nanoseconds > 0).then(|| {
        let rounded_up = nanoseconds.saturating_add(999_999) / 1_000_000;
        i32::try_from(rounded_up).unwrap_or(i32::MAX)
    })
}
