use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const CAPACITY: usize = 10;
const MAX_PAYLOAD_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug)]
pub struct WebhookAuditEntry {
    pub id: u64,
    pub timestamp_ms: u128,
    pub platform: String,
    pub content_type: Option<String>,
    pub payload: String,
    pub body_bytes: usize,
}

pub struct WebhookAudit {
    entries: Mutex<VecDeque<WebhookAuditEntry>>,
    next_id: AtomicU64,
}

impl WebhookAudit {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(CAPACITY)),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn record(&self, platform: &str, content_type: Option<&str>, body: &[u8]) {
        let entry = WebhookAuditEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            platform: platform.to_owned(),
            content_type: content_type.map(str::to_owned),
            payload: String::from_utf8_lossy(&body[..body.len().min(MAX_PAYLOAD_BYTES)])
                .into_owned(),
            body_bytes: body.len(),
        };
        let mut entries = self.entries.lock().expect("webhook audit lock poisoned");
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    pub fn snapshot(&self) -> Vec<WebhookAuditEntry> {
        self.entries
            .lock()
            .expect("webhook audit lock poisoned")
            .iter()
            .rev()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_the_latest_ten_webhooks_in_newest_first_order() {
        let audit = WebhookAudit::new();
        for number in 0..12 {
            audit.record("x", Some("application/json"), number.to_string().as_bytes());
        }
        let entries = audit.snapshot();
        assert_eq!(entries.len(), 10);
        assert_eq!(entries.first().unwrap().payload, "11");
        assert_eq!(entries.last().unwrap().payload, "2");
    }
}
