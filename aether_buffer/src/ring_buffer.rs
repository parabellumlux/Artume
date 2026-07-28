//! AetherOS Rolling Ring-Buffer Transcript
//!
//! A lock-free, bounded circular queue storing timestamped audio transcript
//! tokens. Maintains the last 5 minutes of conversation by default.

use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Transcript entry
// ---------------------------------------------------------------------------

/// A single entry in the rolling transcript buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// The text content of this transcript segment.
    pub text: String,
    /// When this segment was recorded.
    pub timestamp: DateTime<Utc>,
    /// Whether this came from the user (STT) or the system (TTS).
    pub source: TranscriptSource,
    /// Optional entity tags extracted from this segment.
    pub entities: Vec<TaggedEntity>,
}

/// Source of a transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TranscriptSource {
    /// User speech (Speech-to-Text).
    User,
    /// System speech (Text-to-Speech).
    System,
}

/// A tagged entity found within a transcript entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedEntity {
    /// The entity type (e.g., "PhoneNumber", "Address").
    pub entity_type: String,
    /// The raw text of the entity.
    pub value: String,
    /// Start character offset within the entry text.
    pub start: usize,
    /// End character offset within the entry text.
    pub end: usize,
}

// ---------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------

/// A bounded, rolling ring buffer of timestamped transcript entries.
///
/// The buffer maintains a configurable time window (default: 5 minutes)
/// and a maximum entry count. Old entries are evicted when either limit
/// is exceeded.
pub struct TranscriptRingBuffer {
    /// Internal deque of transcript entries (newest at the back).
    buffer: VecDeque<TranscriptEntry>,
    /// Maximum number of entries to store.
    max_entries: usize,
    /// Maximum age of entries in seconds (default: 300 = 5 min).
    max_age_seconds: i64,
    /// Total entries ever added (for stats).
    total_entries_added: u64,
    /// Total entries evicted (for stats).
    total_entries_evicted: u64,
}

impl TranscriptRingBuffer {
    /// Create a new ring buffer with default settings.
    ///
    /// Defaults:
    /// - Max entries: 10,000
    /// - Max age: 300 seconds (5 minutes)
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(10_000),
            max_entries: 10_000,
            max_age_seconds: 300,
            total_entries_added: 0,
            total_entries_evicted: 0,
        }
    }

    /// Create a ring buffer with custom limits.
    pub fn with_limits(max_entries: usize, max_age_seconds: i64) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_entries),
            max_entries,
            max_age_seconds,
            total_entries_added: 0,
            total_entries_evicted: 0,
        }
    }

    /// Push a new transcript entry into the buffer.
    ///
    /// Automatically evicts old entries that exceed the time window or
    /// the maximum count.
    pub fn push(&mut self, text: impl Into<String>, source: TranscriptSource) {
        let text = text.into();
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(self.max_age_seconds);

        // Evict entries that are too old.
        while let Some(front) = self.buffer.front() {
            if front.timestamp < cutoff {
                self.buffer.pop_front();
                self.total_entries_evicted += 1;
            } else {
                break;
            }
        }

        // Evict oldest entries if we're at capacity.
        while self.buffer.len() >= self.max_entries {
            self.buffer.pop_front();
            self.total_entries_evicted += 1;
        }

        self.buffer.push_back(TranscriptEntry {
            text,
            timestamp: now,
            source,
            entities: Vec::new(),
        });
        self.total_entries_added += 1;
    }

    /// Push an entry with pre-tagged entities.
    pub fn push_with_entities(
        &mut self,
        text: impl Into<String>,
        source: TranscriptSource,
        entities: Vec<TaggedEntity>,
    ) {
        let text = text.into();
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(self.max_age_seconds);

        while let Some(front) = self.buffer.front() {
            if front.timestamp < cutoff {
                self.buffer.pop_front();
                self.total_entries_evicted += 1;
            } else {
                break;
            }
        }

        while self.buffer.len() >= self.max_entries {
            self.buffer.pop_front();
            self.total_entries_evicted += 1;
        }

        self.buffer.push_back(TranscriptEntry {
            text,
            timestamp: now,
            source,
            entities,
        });
        self.total_entries_added += 1;
    }

    /// Get all entries within the time window, newest first.
    pub fn entries(&self) -> Vec<&TranscriptEntry> {
        self.buffer.iter().rev().collect()
    }

    /// Get all entries as a single concatenated string, newest first.
    pub fn as_string(&self) -> String {
        self.buffer
            .iter()
            .rev()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Get entries newer than `seconds_ago`, newest first.
    pub fn entries_since(&self, seconds_ago: i64) -> Vec<&TranscriptEntry> {
        let cutoff = Utc::now() - chrono::Duration::seconds(seconds_ago);
        self.buffer
            .iter()
            .rev()
            .filter(|e| e.timestamp >= cutoff)
            .collect()
    }

    /// Search the buffer for text matching a query (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<&TranscriptEntry> {
        let query_lower = query.to_lowercase();
        self.buffer
            .iter()
            .rev()
            .filter(|e| e.text.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Get all entities tagged in the buffer, newest first.
    pub fn all_entities(&self) -> Vec<&TaggedEntity> {
        let mut entities = Vec::new();
        for entry in self.buffer.iter().rev() {
            for entity in &entry.entities {
                entities.push(entity);
            }
        }
        entities
    }

    /// Get entities of a specific type, newest first.
    pub fn entities_of_type(&self, entity_type: &str) -> Vec<&TaggedEntity> {
        let mut entities = Vec::new();
        for entry in self.buffer.iter().rev() {
            for entity in &entry.entities {
                if entity.entity_type == entity_type {
                    entities.push(entity);
                }
            }
        }
        entities
    }

    /// Current number of entries in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.buffer.clear();
        debug!("TranscriptRingBuffer: cleared");
    }

    /// Statistics about the buffer.
    pub fn stats(&self) -> BufferStats {
        BufferStats {
            current_entries: self.buffer.len(),
            max_entries: self.max_entries,
            max_age_seconds: self.max_age_seconds,
            total_entries_added: self.total_entries_added,
            total_entries_evicted: self.total_entries_evicted,
            oldest_entry: self.buffer.front().map(|e| e.timestamp),
            newest_entry: self.buffer.back().map(|e| e.timestamp),
        }
    }
}

/// Statistics snapshot of the ring buffer.
#[derive(Debug, Clone, Serialize)]
pub struct BufferStats {
    pub current_entries: usize,
    pub max_entries: usize,
    pub max_age_seconds: i64,
    pub total_entries_added: u64,
    pub total_entries_evicted: u64,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_retrieve() {
        let mut buffer = TranscriptRingBuffer::new();
        assert!(buffer.is_empty());

        buffer.push("Hello, this is a test", TranscriptSource::User);
        buffer.push("System response here", TranscriptSource::System);

        assert_eq!(buffer.len(), 2);
        let entries = buffer.entries();
        assert_eq!(entries.len(), 2);
        // Newest first.
        assert_eq!(entries[0].text, "System response here");
        assert_eq!(entries[1].text, "Hello, this is a test");
    }

    #[test]
    fn test_as_string() {
        let mut buffer = TranscriptRingBuffer::new();
        buffer.push("First", TranscriptSource::User);
        buffer.push("Second", TranscriptSource::System);
        buffer.push("Third", TranscriptSource::User);

        let s = buffer.as_string();
        assert_eq!(s, "Third Second First");
    }

    #[test]
    fn test_max_entries_eviction() {
        let mut buffer = TranscriptRingBuffer::with_limits(3, 3600);
        buffer.push("A", TranscriptSource::User);
        buffer.push("B", TranscriptSource::User);
        buffer.push("C", TranscriptSource::User);
        assert_eq!(buffer.len(), 3);

        buffer.push("D", TranscriptSource::User);
        assert_eq!(buffer.len(), 3);
        // "A" should have been evicted.
        let entries = buffer.entries();
        assert_eq!(entries[2].text, "B");
    }

    #[test]
    fn test_search() {
        let mut buffer = TranscriptRingBuffer::new();
        buffer.push("Call me back at 555-0199", TranscriptSource::User);
        buffer.push("The address is 123 Main St", TranscriptSource::User);
        buffer.push("System ready", TranscriptSource::System);

        let results = buffer.search("address");
        assert_eq!(results.len(), 1);
        assert!(results[0].text.contains("123 Main St"));

        let results = buffer.search("555");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_entities_of_type() {
        let mut buffer = TranscriptRingBuffer::new();
        buffer.push_with_entities(
            "Call 555-0199",
            TranscriptSource::User,
            vec![TaggedEntity {
                entity_type: "PhoneNumber".to_string(),
                value: "555-0199".to_string(),
                start: 5,
                end: 13,
            }],
        );
        buffer.push_with_entities(
            "Email me at test@example.com",
            TranscriptSource::User,
            vec![TaggedEntity {
                entity_type: "Email".to_string(),
                value: "test@example.com".to_string(),
                start: 11,
                end: 28,
            }],
        );

        let phones = buffer.entities_of_type("PhoneNumber");
        assert_eq!(phones.len(), 1);
        assert_eq!(phones[0].value, "555-0199");

        let emails = buffer.entities_of_type("Email");
        assert_eq!(emails.len(), 1);
        assert_eq!(emails[0].value, "test@example.com");
    }

    #[test]
    fn test_clear() {
        let mut buffer = TranscriptRingBuffer::new();
        buffer.push("Test", TranscriptSource::User);
        assert_eq!(buffer.len(), 1);
        buffer.clear();
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_stats() {
        let mut buffer = TranscriptRingBuffer::with_limits(5, 300);
        buffer.push("A", TranscriptSource::User);
        buffer.push("B", TranscriptSource::User);
        buffer.push("C", TranscriptSource::User);

        let stats = buffer.stats();
        assert_eq!(stats.current_entries, 3);
        assert_eq!(stats.total_entries_added, 3);
        assert_eq!(stats.total_entries_evicted, 0);
        assert!(stats.oldest_entry.is_some());
        assert!(stats.newest_entry.is_some());
    }
}
