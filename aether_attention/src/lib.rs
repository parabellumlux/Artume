//! AetherOS Intelligent Attention Manager
//!
//! Acts as a cognitive load governor — intercepts all incoming system
//! alerts and evaluates whether to drop, queue, or speak them based on
//! the user's current engagement level.
//!
//! ## Architecture
//! ```text
//! Incoming Event → [Urgency Matrix] → [Cognitive Load Evaluator]
//!                    ├── Idle?      → Speak Immediately
//!                    ├── Focused?   → Queue in Buffer
//!                    └── Critical?  → Gentle Spatial Interruption
//! ```

pub mod event;
pub mod evaluator;
pub mod queue;

pub use event::{EventCategory, EventSeverity, SystemEvent, event_channel, EventSender, EventReceiver};
pub use evaluator::{CognitiveLoadEvaluator, DeliveryDecision, UserFocusLevel};
pub use queue::PendingNotificationQueue;
