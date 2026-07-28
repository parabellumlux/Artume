//! AetherOS Event Ingestion Pipeline
//!
//! Typed system events flowing through Tokio MPSC channels.

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

/// Severity level for system events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EventSeverity {
    /// Trivial — background task completion, file sync done.
    Trivial = 1,
    /// Normal — email notification, calendar reminder.
    Normal = 2,
    /// Important — message from contact, build failure.
    Important = 3,
    /// Critical — battery dying, disk full, security alert.
    Critical = 4,
    /// Emergency — system crash imminent, hardware failure.
    Emergency = 5,
}

impl EventSeverity {
    /// Numeric score for the urgency matrix.
    pub fn score(self) -> f32 {
        self as i32 as f32
    }
}

impl fmt::Display for EventSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trivial => write!(f, "trivial"),
            Self::Normal => write!(f, "normal"),
            Self::Important => write!(f, "important"),
            Self::Critical => write!(f, "critical"),
            Self::Emergency => write!(f, "emergency"),
        }
    }
}

/// Categories of system events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventCategory {
    SystemAlert,
    MessageNotification,
    BackgroundTaskCompletion,
    Email,
    CalendarReminder,
    HardwareEvent,
    SecurityAlert,
}

impl fmt::Display for EventCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SystemAlert => write!(f, "system_alert"),
            Self::MessageNotification => write!(f, "message"),
            Self::BackgroundTaskCompletion => write!(f, "task_completion"),
            Self::Email => write!(f, "email"),
            Self::CalendarReminder => write!(f, "calendar"),
            Self::HardwareEvent => write!(f, "hardware"),
            Self::SecurityAlert => write!(f, "security"),
        }
    }
}

/// A typed system event with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    /// Unique event ID.
    pub id: u64,
    /// When the event was created (Unix timestamp ms).
    pub timestamp_ms: i64,
    /// Event category.
    pub category: EventCategory,
    /// Severity level.
    pub severity: EventSeverity,
    /// Human-readable summary.
    pub summary: String,
    /// Optional structured payload (JSON).
    pub payload: Option<serde_json::Value>,
}

impl SystemEvent {
    /// Create a new event with an auto-generated ID.
    pub fn new(
        category: EventCategory,
        severity: EventSeverity,
        summary: impl Into<String>,
    ) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);

        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            category,
            severity,
            summary: summary.into(),
            payload: None,
        }
    }

    /// Attach a structured payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

impl fmt::Display for SystemEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[#{}] {} | {} | {}",
            self.id, self.timestamp_ms, self.severity, self.summary
        )
    }
}

// ---------------------------------------------------------------------------
// Channel types
// ---------------------------------------------------------------------------

/// Shorthand for the MPSC sender used by event producers.
pub type EventSender = tokio::sync::mpsc::UnboundedSender<SystemEvent>;

/// Shorthand for the MPSC receiver used by the evaluator.
pub type EventReceiver = tokio::sync::mpsc::UnboundedReceiver<SystemEvent>;

/// Create a new event channel pair.
pub fn event_channel() -> (EventSender, EventReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let ev = SystemEvent::new(
            EventCategory::SystemAlert,
            EventSeverity::Critical,
            "Battery at 5%",
        );
        assert!(ev.id > 0);
        assert_eq!(ev.category, EventCategory::SystemAlert);
        assert_eq!(ev.severity, EventSeverity::Critical);
        assert_eq!(ev.summary, "Battery at 5%");
    }

    #[test]
    fn test_event_severity_ordering() {
        assert!(EventSeverity::Trivial < EventSeverity::Normal);
        assert!(EventSeverity::Normal < EventSeverity::Important);
        assert!(EventSeverity::Important < EventSeverity::Critical);
        assert!(EventSeverity::Critical < EventSeverity::Emergency);
    }

    #[test]
    fn test_event_channel_send_recv() {
        let (tx, mut rx) = event_channel();
        let ev = SystemEvent::new(
            EventCategory::MessageNotification,
            EventSeverity::Normal,
            "New message from Diana",
        );
        tx.send(ev.clone()).unwrap();
        let received = rx.try_recv().unwrap();
        assert_eq!(received.id, ev.id);
        assert_eq!(received.summary, "New message from Diana");
    }
}
