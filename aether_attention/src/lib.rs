//! Artume Intelligent Attention Manager
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

pub mod evaluator;
pub mod event;
pub mod queue;

pub use evaluator::{CognitiveLoadEvaluator, DeliveryDecision, UserFocusLevel};
pub use event::{
    event_channel, EventCategory, EventReceiver, EventSender, EventSeverity, SystemEvent,
};
pub use queue::PendingNotificationQueue;
