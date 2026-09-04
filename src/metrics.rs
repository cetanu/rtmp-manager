use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::watch;

pub struct Metrics {
    ingest_bytes: AtomicU64,
    ingest_bps: AtomicU64,
    last_sample_ingest_bytes: AtomicU64,
    target_bitrates: RwLock<HashMap<String, Arc<TargetBitrate>>>,
    history: RwLock<VecDeque<MetricsSample>>,
    samples: watch::Sender<Option<MetricsSample>>,
}

const HISTORY_SECONDS: usize = 300;

#[derive(Default)]
pub struct TargetBitrate {
    outbound_bps: AtomicU64,
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
    pub targets: Vec<TargetBitrateSample>,
}

impl Default for Metrics {
    fn default() -> Self {
        let (samples, _) = watch::channel(None);
        Self {
            ingest_bytes: AtomicU64::new(0),
            ingest_bps: AtomicU64::new(0),
            last_sample_ingest_bytes: AtomicU64::new(0),
            target_bitrates: RwLock::new(HashMap::new()),
            history: RwLock::new(VecDeque::with_capacity(HISTORY_SECONDS)),
            samples,
        }
    }
}

impl Metrics {
    pub fn register_target(&self, name: String) -> Arc<TargetBitrate> {
        let bitrate = Arc::new(TargetBitrate::default());
        self.target_bitrates
            .write()
            .unwrap()
            .insert(name, Arc::clone(&bitrate));
        bitrate
    }

    pub fn unregister_target(&self, name: &str) {
        self.target_bitrates.write().unwrap().remove(name);
    }

    pub fn current_target_bitrates(&self) -> Vec<TargetBitrateSample> {
        let mut samples = self
            .target_bitrates
            .read()
            .unwrap()
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
        let ingest_bps = bytes.saturating_sub(previous).saturating_mul(8);
        self.ingest_bps.store(ingest_bps, Ordering::Relaxed);
        let sample = MetricsSample {
            timestamp_ms,
            ingest_bps,
            targets: self.current_target_bitrates(),
        };
        {
            let mut history = self.history.write().unwrap();
            if history.len() == HISTORY_SECONDS {
                history.pop_front();
            }
            history.push_back(sample.clone());
        }
        self.samples.send_replace(Some(sample));
    }

    pub fn history(&self) -> Vec<MetricsSample> {
        self.history.read().unwrap().iter().cloned().collect()
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
}

impl TargetBitrate {
    pub fn update_from_ffmpeg(&self, bits_per_second: u64) {
        self.outbound_bps.store(bits_per_second, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::Metrics;

    #[test]
    fn ingest_sample_is_the_byte_delta_in_bits_per_second() {
        let metrics = Metrics::default();
        metrics.add_ingest_bytes(125);
        metrics.record_sample();
        assert_eq!(metrics.history()[0].ingest_bps, 1_000);

        metrics.record_sample();
        assert_eq!(metrics.history()[1].ingest_bps, 0);
    }
}
