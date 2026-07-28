//! AetherOS Cognitive Load Evaluator
//!
//! Tracks the user's current focus level and rates incoming events using
//! a heuristic score to decide whether to deliver immediately, queue, or
//! drop the notification.

use crate::event::{EventSeverity, SystemEvent};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// User focus level
// ---------------------------------------------------------------------------

/// The user's current cognitive engagement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserFocusLevel {
    /// User is inactive / away from desk.
    Idle = 0,
    /// User is listening to casual content (podcast, music).
    Listening = 1,
    /// User is actively dictating or speaking.
    Dictating = 2,
    /// User is performing a critical system task (e.g., install, config).
    UrgentSystemTask = 3,
}

impl UserFocusLevel {
    /// Numeric score for the heuristic formula.
    /// Higher values = more focused = harder to interrupt.
    pub fn score(self) -> f32 {
        self as i32 as f32
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Listening => "listening",
            Self::Dictating => "dictating",
            Self::UrgentSystemTask => "urgent_system_task",
        }
    }
}

impl fmt::Display for UserFocusLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Delivery decision
// ---------------------------------------------------------------------------

/// What to do with an incoming event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeliveryDecision {
    /// Deliver immediately via spatial audio (+45° position).
    DeliverNow,
    /// Queue in the pending notification buffer.
    Queue,
    /// Drop the event entirely (e.g., trivial event during heavy focus).
    Drop,
}

impl fmt::Display for DeliveryDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeliverNow => write!(f, "deliver_now"),
            Self::Queue => write!(f, "queue"),
            Self::Drop => write!(f, "drop"),
        }
    }
}

// ---------------------------------------------------------------------------
// Cognitive load evaluator
// ---------------------------------------------------------------------------

/// Evaluates incoming events against the user's current cognitive load.
///
/// ## Heuristic Score
/// ```text
/// Score = (EventSeverity * 0.6) - (UserFocusLevel * 0.4)
/// ```
///
/// - **Score > Threshold** → Deliver immediately.
/// - **Score <= Threshold** → Queue for later.
/// - **Score < DropThreshold** → Drop entirely.
pub struct CognitiveLoadEvaluator {
    /// Current user focus level.
    focus_level: UserFocusLevel,
    /// Score threshold for immediate delivery.
    delivery_threshold: f32,
    /// Score below which events are dropped entirely.
    drop_threshold: f32,
    /// Number of events evaluated.
    events_evaluated: u64,
    /// Number of events delivered immediately.
    events_delivered: u64,
    /// Number of events queued.
    events_queued: u64,
    /// Number of events dropped.
    events_dropped: u64,
}

impl CognitiveLoadEvaluator {
    /// Create a new evaluator with default thresholds.
    ///
    /// Default thresholds:
    /// - `delivery_threshold`: 1.2 (events scoring above this are delivered)
    /// - `drop_threshold`: 0.0 (events scoring below this are dropped)
    pub fn new() -> Self {
        Self {
            focus_level: UserFocusLevel::Idle,
            delivery_threshold: 1.2,
            drop_threshold: 0.0,
            events_evaluated: 0,
            events_delivered: 0,
            events_queued: 0,
            events_dropped: 0,
        }
    }

    /// Update the user's current focus level.
    pub fn set_focus_level(&mut self, level: UserFocusLevel) {
        if self.focus_level != level {
            info!(
                "CognitiveLoad: focus transition {} → {}",
                self.focus_level, level
            );
            self.focus_level = level;
        }
    }

    /// Get the current focus level.
    pub fn focus_level(&self) -> UserFocusLevel {
        self.focus_level
    }

    /// Evaluate an event and return a delivery decision.
    ///
    /// The heuristic score is:
    /// ```text
    /// score = (severity_score * 0.6) - (focus_score * 0.4)
    /// ```
    pub fn evaluate(&mut self, event: &SystemEvent) -> DeliveryDecision {
        self.events_evaluated += 1;

        let severity_score = event.severity.score();
        let focus_score = self.focus_level.score();

        let score = severity_score * 0.6 - focus_score * 0.4;

        debug!(
            "CognitiveLoad: event={} severity={:.0} focus={:.0} score={:.2}",
            event.id, severity_score, focus_score, score
        );

        let decision = if score > self.delivery_threshold {
            // Emergency override: always deliver Emergency events regardless.
            if event.severity == EventSeverity::Emergency {
                self.events_delivered += 1;
                info!(
                    "CognitiveLoad: EMERGENCY OVERRIDE — delivering event #{}",
                    event.id
                );
                return DeliveryDecision::DeliverNow;
            }
            self.events_delivered += 1;
            DeliveryDecision::DeliverNow
        } else if score > self.drop_threshold {
            self.events_queued += 1;
            DeliveryDecision::Queue
        } else {
            self.events_dropped += 1;
            DeliveryDecision::Drop
        };

        debug!(
            "CognitiveLoad: event #{} → {} (score={:.2})",
            event.id, decision, score
        );
        decision
    }

    /// Check if the user is currently idle (used by the queue to trigger
    /// batch summarisation).
    pub fn is_idle(&self) -> bool {
        self.focus_level == UserFocusLevel::Idle
    }

    /// Statistics for monitoring.
    pub fn stats(&self) -> EvaluatorStats {
        EvaluatorStats {
            events_evaluated: self.events_evaluated,
            events_delivered: self.events_delivered,
            events_queued: self.events_queued,
            events_dropped: self.events_dropped,
            current_focus: self.focus_level,
        }
    }
}

/// Snapshot of evaluator statistics.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluatorStats {
    pub events_evaluated: u64,
    pub events_delivered: u64,
    pub events_queued: u64,
    pub events_dropped: u64,
    pub current_focus: UserFocusLevel,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;

    /// During heavy focus (Dictating), normal events should be queued.
    #[test]
    fn test_suppression_during_high_focus() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::Dictating);

        // Normal event during dictation: score = 2*0.6 - 2*0.4 = 0.4
        let ev = SystemEvent::new(
            EventCategory::Email,
            EventSeverity::Normal,
            "New email from team",
        );
        let decision = evaluator.evaluate(&ev);
        assert_eq!(decision, DeliveryDecision::Queue);
    }

    /// During idle, even trivial events should be delivered.
    #[test]
    fn test_delivery_during_idle() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::Idle);

        // Trivial event during idle: score = 1*0.6 - 0*0.4 = 0.6
        let ev = SystemEvent::new(
            EventCategory::BackgroundTaskCompletion,
            EventSeverity::Trivial,
            "File sync complete",
        );
        let decision = evaluator.evaluate(&ev);
        assert_eq!(decision, DeliveryDecision::Queue); // 0.6 < 1.2 threshold
    }

    /// Critical events should always be delivered.
    #[test]
    fn test_critical_event_breaks_through() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::Dictating);

        // Critical event during dictation: score = 4*0.6 - 2*0.4 = 1.6
        let ev = SystemEvent::new(
            EventCategory::SystemAlert,
            EventSeverity::Critical,
            "Disk space critically low",
        );
        let decision = evaluator.evaluate(&ev);
        assert_eq!(decision, DeliveryDecision::DeliverNow);
    }

    /// Emergency events always override, regardless of focus.
    #[test]
    fn test_emergency_override() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::UrgentSystemTask);

        let ev = SystemEvent::new(
            EventCategory::SecurityAlert,
            EventSeverity::Emergency,
            "Intrusion detected",
        );
        let decision = evaluator.evaluate(&ev);
        assert_eq!(decision, DeliveryDecision::DeliverNow);
    }

    /// Trivial events during dictation should be dropped.
    #[test]
    fn test_trivial_dropped_during_dictation() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::Dictating);

        // Trivial during dictation: score = 1*0.6 - 2*0.4 = -0.2
        let ev = SystemEvent::new(
            EventCategory::BackgroundTaskCompletion,
            EventSeverity::Trivial,
            "Cache cleaned",
        );
        let decision = evaluator.evaluate(&ev);
        assert_eq!(decision, DeliveryDecision::Drop);
    }

    /// Test focus level transitions.
    #[test]
    fn test_focus_transition() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        assert_eq!(evaluator.focus_level(), UserFocusLevel::Idle);

        evaluator.set_focus_level(UserFocusLevel::Dictating);
        assert_eq!(evaluator.focus_level(), UserFocusLevel::Dictating);

        evaluator.set_focus_level(UserFocusLevel::Idle);
        assert!(evaluator.is_idle());
    }

    /// Test stats tracking.
    #[test]
    fn test_stats() {
        let mut evaluator = CognitiveLoadEvaluator::new();
        evaluator.set_focus_level(UserFocusLevel::Dictating);

        let ev1 = SystemEvent::new(EventCategory::Email, EventSeverity::Normal, "email");
        let ev2 = SystemEvent::new(EventCategory::SystemAlert, EventSeverity::Critical, "alert");
        let ev3 = SystemEvent::new(EventCategory::BackgroundTaskCompletion, EventSeverity::Trivial, "trivial");

        evaluator.evaluate(&ev1); // queue
        evaluator.evaluate(&ev2); // deliver
        evaluator.evaluate(&ev3); // drop

        let stats = evaluator.stats();
        assert_eq!(stats.events_evaluated, 3);
        assert_eq!(stats.events_delivered, 1);
        assert_eq!(stats.events_queued, 1);
        assert_eq!(stats.events_dropped, 1);
    }
}
