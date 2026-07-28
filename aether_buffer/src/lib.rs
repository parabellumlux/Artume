//! AetherOS Audio Context Buffer (Scratchpad)
//!
//! Enables speech referencing like "Copy that tracking number" or "Save
//! the phone number she just said."
//!
//! ## Architecture
//! ```text
//! Rolling Speech Stream → [Named Entity Recognition] → Memory Slots
//! User: "Save that phone number" → Saved directly to Contacts
//! ```

pub mod ring_buffer;
pub mod ner_resolver;

pub use ring_buffer::{
    BufferStats, TaggedEntity, TranscriptEntry, TranscriptRingBuffer, TranscriptSource,
};
pub use ner_resolver::{ContextResolver, NerEngine};
