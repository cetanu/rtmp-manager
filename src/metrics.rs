use parking_lot::RwLock;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::watch;

pub struct Metrics {
    ingest_bytes: AtomicU64,
    ingest_bps: AtomicU64,
    egress_bytes: AtomicU64,
    chat_messages_received: AtomicU64,
    last_sample_ingest_bytes: AtomicU64,
    last_sample_timestamp_ms: AtomicU64,
    target_bitrates: RwLock<HashMap<String, Arc<TargetBitrate>>>,
    history: RwLock<VecDeque<MetricsSample>>,
    history_window: Duration,
    samples: watch::Sender<Option<MetricsSample>>,
}

const HISTORY_WINDOW: Duration = Duration::from_secs(300);

#[derive(Default)]
pub struct TargetBitrate {
    outbound_bps: AtomicU64,
    outbound_bytes: AtomicU64,
    last_total_size: AtomicU64,
}

#[derive(Clone, Serialize)]
pub struct TargetBitrateSample {
    pub name: String,
    pub outbound_bps: u64,
}

#[derive(Clone, Serialize)]
pub struct MetricsSample {
    pub timestamp_ms: u128,
    pub ingest_bps: u64,
    pub ingest_bytes: u64,
    pub egress_bytes: u64,
    pub chat_messages_received: u64,
    pub targets: Vec<TargetBitrateSample>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::with_history_window(HISTORY_WINDOW)
    }
}

impl Metrics {
    pub fn with_history_window(history_window: Duration) -> Self {
        let (samples, _) = watch::channel(None);
        Self {
            ingest_bytes: AtomicU64::new(0),
            ingest_bps: AtomicU64::new(0),
            egress_bytes: AtomicU64::new(0),
            chat_messages_received: AtomicU64::new(0),
            last_sample_ingest_bytes: AtomicU64::new(0),
            last_sample_timestamp_ms: AtomicU64::new(0),
            target_bitrates: RwLock::new(HashMap::new()),
            history: RwLock::new(VecDeque::new()),
            history_window,
            samples,
        }
    }

    pub fn register_target(&self, name: String) -> Arc<TargetBitrate> {
        let bitrate = Arc::new(TargetBitrate::default());
        self.target_bitrates
            .write()
            .insert(name, Arc::clone(&bitrate));
        bitrate
    }

    pub fn unregister_target(&self, name: &str) {
        self.target_bitrates.write().remove(name);
    }

    pub fn current_target_bitrates(&self) -> Vec<TargetBitrateSample> {
        let mut samples = self
            .target_bitrates
            .read()
            .iter()
            .map(|(name, bitrate)| TargetBitrateSample {
                name: name.clone(),
                outbound_bps: bitrate.outbound_bps.load(Ordering::Relaxed),
            })
            .collect::<Vec<_>>();
        samples.sort_by(|left, right| left.name.cmp(&right.name));
        samples
    }

    pub fn record_sample(&self) {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let bytes = self.ingest_bytes.load(Ordering::Relaxed);
        let previous = self.last_sample_ingest_bytes.swap(bytes, Ordering::Relaxed);
        let now_ms: u64 = timestamp_ms
            .try_into()
            .unwrap_or_else(|_| crate::util::now_unix_ms());
        let previous_timestamp_ms = self
            .last_sample_timestamp_ms
            .swap(now_ms, Ordering::Relaxed);
        let ingest_bps = calculate_ingest_bps(bytes, previous, previous_timestamp_ms, now_ms);
        self.ingest_bps.store(ingest_bps, Ordering::Relaxed);
        let sample = MetricsSample {
            timestamp_ms,
            ingest_bps,
            ingest_bytes: bytes,
            egress_bytes: self.current_egress_bytes(),
            chat_messages_received: self.current_chat_messages_received(),
            targets: self.current_target_bitrates(),
        };
        {
            let mut history = self.history.write();
            let oldest_timestamp = timestamp_ms.saturating_sub(self.history_window.as_millis());
            while history
                .front()
                .is_some_and(|sample| sample.timestamp_ms < oldest_timestamp)
            {
                history.pop_front();
            }
            history.push_back(sample.clone());
        }
        self.samples.send_replace(Some(sample));
    }

    pub fn history(&self) -> Vec<MetricsSample> {
        self.history.read().iter().cloned().collect()
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<MetricsSample>> {
        self.samples.subscribe()
    }

    pub fn add_ingest_bytes(&self, bytes: u64) {
        self.ingest_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn current_ingest_bps(&self) -> u64 {
        self.ingest_bps.load(Ordering::Relaxed)
    }

    pub fn current_ingest_bytes(&self) -> u64 {
        self.ingest_bytes.load(Ordering::Relaxed)
    }

    pub fn add_egress_bytes(&self, bytes: u64) {
        self.egress_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn current_egress_bytes(&self) -> u64 {
        self.egress_bytes.load(Ordering::Relaxed)
    }

    pub fn add_chat_messages_received(&self, count: u64) {
        self.chat_messages_received
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn current_chat_messages_received(&self) -> u64 {
        self.chat_messages_received.load(Ordering::Relaxed)
    }
}

fn calculate_ingest_bps(
    bytes: u64,
    previous_bytes: u64,
    previous_timestamp_ms: u64,
    timestamp_ms: u64,
) -> u64 {
    if previous_timestamp_ms == 0 {
        return 0;
    }

    let elapsed_ms = timestamp_ms.saturating_sub(previous_timestamp_ms).max(1);
    let bits_per_second = u128::from(bytes.saturating_sub(previous_bytes)).saturating_mul(8_000)
        / u128::from(elapsed_ms);
    u64::try_from(bits_per_second).unwrap_or(u64::MAX)
}

impl TargetBitrate {
    pub fn update_from_ffmpeg(&self, bits_per_second: u64) {
        self.outbound_bps.store(bits_per_second, Ordering::Relaxed);
    }

    /// Records an absolute `total_size` value reported by FFmpeg and returns
    /// the delta to add to the global egress counter.
    ///
    /// FFmpeg reports a per-process cumulative byte count, so a restart resets
    /// it to zero. When the reported value goes backwards we treat it as a
    /// fresh process and count the full value as new bytes.
    pub fn update_total_bytes(&self, total_size: u64) -> u64 {
        let previous = self.last_total_size.swap(total_size, Ordering::Relaxed);
        let delta = if total_size >= previous {
            total_size - previous
        } else {
            total_size
        };
        self.outbound_bytes.fetch_add(delta, Ordering::Relaxed);
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::{Metrics, calculate_ingest_bps};
    use std::sync::atomic::Ordering;

    #[test]
    fn ingest_sample_is_the_byte_delta_in_bits_per_second() {
        let metrics = Metrics::default();
        metrics.add_chat_messages_received(3);
        metrics.add_ingest_bytes(125);
        metrics.record_sample();
        assert_eq!(metrics.history()[0].ingest_bps, 0);
        assert_eq!(metrics.history()[0].chat_messages_received, 3);

        metrics
            .last_sample_timestamp_ms
            .fetch_sub(1_000, Ordering::Relaxed);
        metrics.add_ingest_bytes(125);
        metrics.record_sample();
        assert_eq!(metrics.history()[1].ingest_bps, 1_000);
    }

    #[test]
    fn ingest_sample_uses_actual_elapsed_time() {
        assert_eq!(calculate_ingest_bps(250, 125, 1_000, 2_500), 666);
    }

    #[test]
    fn egress_totals_accumulate_and_survive_ffmpeg_restarts() {
        let metrics = Metrics::default();
        let target = metrics.register_target("twitch".to_string());
        metrics.add_ingest_bytes(1_000);
        metrics.add_egress_bytes(target.update_total_bytes(500));
        metrics.add_egress_bytes(target.update_total_bytes(800));
        // Restart resets FFmpeg's counter; the new bytes still count.
        metrics.add_egress_bytes(target.update_total_bytes(100));
        assert_eq!(metrics.current_egress_bytes(), 900);
        metrics.record_sample();
        let sample = metrics.history().pop().expect("one sample");
        assert_eq!(sample.ingest_bytes, 1_000);
        assert_eq!(sample.egress_bytes, 900);
    }
}
