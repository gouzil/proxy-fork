use std::{
    collections::VecDeque,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

#[derive(Clone)]
pub struct LogRecord {
    pub level: tracing::Level,
    pub line: String,
}
#[derive(Default)]
pub struct LogBuffer {
    pub records: Mutex<VecDeque<LogRecord>>,
    pub dropped: AtomicU64,
    pub changed: tokio::sync::Notify,
}
impl LogBuffer {
    pub fn push(&self, level: tracing::Level, message: String) {
        if let Ok(mut records) = self.records.try_lock() {
            if records.len() >= 2000 {
                records.pop_front();
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                % 86400;
            let message: String = message
                .chars()
                .take(2048)
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            records.push_back(LogRecord {
                level,
                line: format!(
                    "{:02}:{:02}:{:02} {level:5} {message}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60
                ),
            });
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        self.changed.notify_one();
    }
    pub fn clear(&self) {
        if let Ok(mut records) = self.records.lock() {
            records.clear();
            self.dropped.store(0, Ordering::Relaxed);
        }
    }
}
pub struct GuiLayer(pub Arc<LogBuffer>);
#[derive(Default)]
struct Message(String);
impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}
impl<S: Subscriber> Layer<S> for GuiLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let target = event.metadata().target();
        // Only application lifecycle messages enter the GUI; proxy request diagnostics may contain secrets.
        if target.starts_with("proxy_fork_gui") || target == "proxy_fork_core::proxy_service" {
            let mut message = Message::default();
            event.record(&mut message);
            self.0.push(*event.metadata().level(), message.0);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_history_discards_oldest_and_does_not_grow() {
        let log = LogBuffer::default();
        for i in 0..2500 {
            log.push(tracing::Level::INFO, format!("entry {i}"));
        }
        let records = log.records.lock().unwrap();
        assert_eq!(records.len(), 2000);
        assert!(records.front().unwrap().line.ends_with("entry 500"));
        assert_eq!(log.dropped.load(Ordering::Relaxed), 500);
    }
}
