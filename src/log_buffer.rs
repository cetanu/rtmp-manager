use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::broadcast;
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

const CAPACITY: usize = 500;
static LOGS: OnceLock<Arc<LogBuffer>> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub id: u64,
    pub timestamp_ms: u128,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct LogBuffer {
    entries: Mutex<VecDeque<LogEntry>>,
    next_id: AtomicU64,
    sender: broadcast::Sender<LogEntry>,
}

pub fn init() -> (Arc<LogBuffer>, LogLayer) {
    let buffer = Arc::new(LogBuffer::new());
    let _ = LOGS.set(Arc::clone(&buffer));
    (Arc::clone(&buffer), LogLayer { buffer })
}

pub fn global() -> anyhow::Result<Arc<LogBuffer>> {
    LOGS.get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("log buffer is not initialized"))
}

impl LogBuffer {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self {
            entries: Mutex::new(VecDeque::with_capacity(CAPACITY)),
            next_id: AtomicU64::new(1),
            sender,
        }
    }

    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries.lock().iter().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }

    fn push(&self, mut entry: LogEntry) {
        entry.id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.entries.lock();
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry.clone());
        drop(entries);
        let _ = self.sender.send(entry);
    }
}

pub struct LogLayer {
    buffer: Arc<LogBuffer>,
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        self.buffer.push(LogEntry {
            id: 0,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            level: metadata.level().to_string(),
            target: metadata.target().to_string(),
            message: visitor.finish(),
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl FieldVisitor {
    fn finish(self) -> String {
        match (self.message, self.fields.is_empty()) {
            (Some(message), false) => format!("{message} {}", self.fields.join(" ")),
            (Some(message), true) => message,
            (None, _) => self.fields.join(" "),
        }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_display(field, value);
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_display(field, value);
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_display(field, value);
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_display(field, value);
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.record_display(field, value);
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        let value = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

impl FieldVisitor {
    fn record_display(&mut self, field: &tracing::field::Field, value: impl fmt::Display) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_retains_only_the_latest_entries() {
        let buffer = LogBuffer::new();
        for number in 0..CAPACITY + 2 {
            buffer.push(LogEntry {
                id: 0,
                timestamp_ms: 0,
                level: "INFO".into(),
                target: "test".into(),
                message: number.to_string(),
            });
        }
        let entries = buffer.snapshot();
        assert_eq!(entries.len(), CAPACITY);
        assert_eq!(entries.first().unwrap().message, "2");
    }
}
