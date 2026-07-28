//! AetherOS Pending Notification Queue
//!
//! A bounded queue that holds notifications suppressed during high-focus
//! periods. When the user returns to `Idle`, `batch_summarize()` generates
//! a concise spoken overview of all missed notifications.

use crate::event::SystemEvent;
use crate::evaluator::{CognitiveLoadEvaluator, DeliveryDecision};
use log::{debug, info, warn};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Pending notification queue
// ---------------------------------------------------------------------------

/// A bounded queue of pending notifications.
///
/// Notifications are enqueued when the cognitive load evaluator decides
/// they should be suppressed. When the user transitions to `Idle`, the
/// queue can be drained and summarised into a single spoken sentence.
pub struct PendingNotificationQueue {
    /// Internal ring buffer of pending events.
    buffer: VecDeque<SystemEvent>,
    /// Maximum number of events to hold.
    capacity: usize,
    /// Whether the user was idle on the last check (for edge detection).
    was_idle: bool,
}

impl PendingNotificationQueue {
    /// Create a new queue with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            was_idle: false,
        }
    }

    /// Push an event into the queue.
    ///
    /// If the queue is full, the oldest event is dropped.
    pub fn push(&mut self, event: SystemEvent) {
        if self.buffer.len() >= self.capacity {
            let dropped = self.buffer.pop_front();
            warn!(
                "NotificationQueue: full, dropping oldest event #{}",
                dropped.map(|e| e.id).unwrap_or(0)
            );
        }
        debug!(
            "NotificationQueue: enqueue event #{} ({})",
            event.id, event.summary
        );
        self.buffer.push_back(event);
    }

    /// Drain all pending events and return them.
    pub fn drain(&mut self) -> Vec<SystemEvent> {
        let drained: Vec<SystemEvent> = self.buffer.drain(..).collect();
        if !drained.is_empty() {
            info!("NotificationQueue: drained {} events", drained.len());
        }
        drained
    }

    /// Peek at the oldest event without removing it.
    pub fn peek_oldest(&self) -> Option<&SystemEvent> {
        self.buffer.front()
    }

    /// Number of events currently in the queue.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Generate a single-sentence spoken overview of all queued notifications.
    ///
    /// This is called when the user returns to `Idle` state. It groups
    /// events by category and produces a concise summary suitable for TTS.
    ///
    /// ## Example Output
    /// "You have 3 notifications: 2 emails and 1 system alert. The most
    /// recent was 'Disk space at 15%'."
    pub fn batch_summarize(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }

        let total = self.buffer.len();

        // Count by category.
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for ev in &self.buffer {
            *counts.entry(ev.category.to_string()).or_insert(0) += 1;
        }

        // Build category summary.
        let mut cat_parts: Vec<String> = Vec::new();
        for (cat, count) in &counts {
            cat_parts.push(format!("{} {}", count, cat));
        }
        let cat_summary = cat_parts.join(", ");

        // Find the most recent event.
        let most_recent = self
            .buffer
            .iter()
            .max_by_key(|e| e.timestamp_ms)
            .map(|e| e.summary.as_str())
            .unwrap_or("");

        let summary = format!(
            "You have {} notification{}. {}. The most recent was '{}'.",
            total,
            if total == 1 { "" } else { "s" },
            cat_summary,
            most_recent,
        );

        info!("NotificationQueue: batch summary generated: {}", summary);
        Some(summary)
    }

    /// Tick the queue — call this periodically to check for idle transitions.
    ///
    /// Returns `Some(summary)` if the user just transitioned to idle and
    /// there are pending notifications to summarise.
    pub fn tick(&mut self, evaluator: &CognitiveLoadEvaluator) -> Option<String> {
        let is_idle = evaluator.is_idle();
        let just_became_idle = is_idle && !self.was_idle;
        self.was_idle = is_idle;

        if just_became_idle && !self.buffer.is_empty() {
            let summary = self.batch_summarize();
            self.buffer.clear();
            return summary;
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;
    use crate::evaluator::*;

    /// Test basic enqueue and drain.
    #[test]
    fn test_enqueue_drain() {
        let mut queue = PendingNotificationQueue::new(10);
        assert!(queue.is_empty());

        queue.push(SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "Email from boss",
        ));
        queue.push(SystemEvent::new(
            EventCategory::SystemAlert,
            EventSeverity::Important,
            "Disk at 20%",
        ));

        assert_eq!(queue.len(), 2);

        let drained = queue.drain();
        assert_eq!(drained.len(), 2);
        assert!(queue.is_empty());
    }

    /// Test that the queue drops oldest events when full.
    #[test]
    fn test_queue_capacity() {
        let mut queue = PendingNotificationQueue::new(3);
        for i in 0..5 {
            queue.push(SystemEvent::new(
                EventCategory::BackgroundTaskCompletion,
                EventSeverity::Trivial,
                format!("Event {}", i),
            ));
        }
        // Only the last 3 should remain.
        assert_eq!(queue.len(), 3);
        let oldest = queue.peek_oldest().unwrap();
        assert_eq!(oldest.summary, "Event 2");
    }

    /// Test batch summarisation.
    #[test]
    fn test_batch_summarize() {
        let mut queue = PendingNotificationQueue::new(10);

        queue.push(SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "New email from Diana",
        ));
        queue.push(SystemEvent::new(
            EventCategory::SystemAlert,
            EventSeverity::Important,
            "Disk space at 15%",
        ));
        queue.push(SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "Meeting reminder",
        ));

        let summary = queue.batch_summarize().unwrap();
        assert!(summary.contains("3 notifications"));
        assert!(summary.contains("2 email"));
        assert!(summary.contains("1 system_alert"));
        // The most recent event is the last one pushed.
        assert!(summary.contains("Meeting reminder"));
    }

    /// Test that empty queue returns None for batch_summarize.
    #[test]
    fn test_empty_summarize() {
        let mut queue = PendingNotificationQueue::new(10);
        assert!(queue.batch_summarize().is_none());
    }

    /// Test the full pipeline: suppression during high focus → batch on idle.
    #[test]
    fn test_suppression_then_batch_on_idle() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        let mut queue = PendingNotificationQueue::new(10);

        // Start in dictating mode.
        evaluator.set_focus_level(UserFocusLevel::Dictating);

        // Events come in while focused.
        let ev1 = SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "Email from team",
        );
        let ev2 = SystemEvent::new(
            EventCategory::SystemAlert,
            EventSeverity::Important,
            "Build complete",
        );

        // Both should be queued (not delivered).
        assert_eq!(evaluator.evaluate(&ev1), DeliveryDecision::Queue);
        assert_eq!(evaluator.evaluate(&ev2), DeliveryDecision::Queue);

        queue.push(ev1);
        queue.push(ev2);
        assert_eq!(queue.len(), 2);

        // Now user transitions to idle.
        evaluator.set_focus_level(UserFocusLevel::Idle);

        // Tick should detect the transition and generate a summary.
        let summary = queue.tick(&evaluator);
        assert!(summary.is_some());
        let text = summary.unwrap();
        assert!(text.contains("2 notifications"));
        assert!(text.contains("email"));
        assert!(text.contains("system_alert"));

        // Queue should be empty after tick.
        assert!(queue.is_empty());
    }

    /// Test that tick does NOT generate a summary if already idle.
    #[test]
    fn test_tick_no_summary_when_already_idle() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        let mut queue = PendingNotificationQueue::new(10);

        // Already idle.
        evaluator.set_focus_level(UserFocusLevel::Idle);

        queue.push(SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "Test",
        ));

        // First tick: was_idle is false, is_idle is true → transition detected.
        let summary = queue.tick(&evaluator);
        assert!(summary.is_some());

        // Second tick: both was_idle and is_idle are true → no transition.
        let summary = queue.tick(&evaluator);
        assert!(summary.is_none());
    }
}
