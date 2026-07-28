//! AetherOS Named Entity Recognition & Contextual Query Resolver
//!
//! Runs a light local NER parser (regex patterns) across incoming buffer
//! text to continuously tag entities: PhoneNumbers, Addresses, Dates/Times,
//! URLs, FinancialAmounts, TrackingCodes.
//!
//! The `resolve_reference()` function enables natural-language queries like
//! "Copy that tracking number" by scanning the buffer backwards from the
//! current timestamp.

use crate::ring_buffer::{TaggedEntity, TranscriptRingBuffer};
use log::{debug, info, warn};
use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Compiled regex patterns
// ---------------------------------------------------------------------------

/// Phone number patterns (US-centric with international support).
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:\+?1?[-.\s]?\(?[2-9]\d{2}\)?[-.\s]?\d{3}[-.\s]?\d{4})|(?:\+\d{1,3}[-.\s]\d{1,4}[-.\s]\d{1,4}[-.\s]\d{1,9})|(?:\d{3}[-.\s]\d{4})"
    )
    .expect("PHONE_RE")
});

/// Email address pattern.
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}")
        .expect("EMAIL_RE")
});

/// URL pattern.
static URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://[^\s,;)]+").expect("URL_RE")
});

/// US street address pattern (simplified).
static ADDRESS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\d{1,5}\s+[A-Za-z0-9\s.]+(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Lane|Ln|Drive|Dr|Court|Ct|Place|Pl|Way|Circle|Cir|Parkway|Pkwy)(?:,\s*[A-Za-z\s]+,\s*[A-Z]{2}\s*\d{5})?"
    )
    .expect("ADDRESS_RE")
});

/// Date/time patterns (multiple formats).
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:\d{1,2}[/-]\d{1,2}[/-]\d{2,4})|(?:(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:tember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\s+\d{1,2}(?:st|nd|rd|th)?,?\s+\d{4})|(?:\d{1,2}:\d{2}\s*(?:AM|PM|am|pm)?)|(?:(?:today|tomorrow|yesterday|next\s+\w+|this\s+\w+))"
    )
    .expect("DATE_RE")
});

/// Financial amount pattern.
static AMOUNT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\d{1,3}(?:,\d{3})*(?:\.\d{2})?").expect("AMOUNT_RE")
});

/// Tracking number pattern (UPS, FedEx, USPS).
static TRACKING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:\b1Z\w{16}\b)|(?:\b\d{12,15}\b)|(?:[A-Z]{2}\d{9,10}(?:US)?\b)"
    )
    .expect("TRACKING_RE")
});

// ---------------------------------------------------------------------------
// Entity type constants
// ---------------------------------------------------------------------------

pub const ENTITY_PHONE: &str = "PhoneNumber";
pub const ENTITY_EMAIL: &str = "Email";
pub const ENTITY_URL: &str = "URL";
pub const ENTITY_ADDRESS: &str = "Address";
pub const ENTITY_DATE: &str = "DateTime";
pub const ENTITY_AMOUNT: &str = "FinancialAmount";
pub const ENTITY_TRACKING: &str = "TrackingNumber";

// ---------------------------------------------------------------------------
// NER engine
// ---------------------------------------------------------------------------

/// A lightweight NER engine that uses regex patterns to tag entities in
/// transcript text.
pub struct NerEngine;

impl NerEngine {
    /// Run all NER patterns against a text string and return found entities.
    pub fn extract_entities(text: &str) -> Vec<TaggedEntity> {
        let mut entities = Vec::new();

        // Phone numbers.
        for m in PHONE_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_PHONE.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // Emails.
        for m in EMAIL_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_EMAIL.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // URLs.
        for m in URL_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_URL.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // Addresses.
        for m in ADDRESS_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_ADDRESS.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // Dates/times.
        for m in DATE_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_DATE.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // Financial amounts.
        for m in AMOUNT_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_AMOUNT.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        // Tracking numbers.
        for m in TRACKING_RE.find_iter(text) {
            entities.push(TaggedEntity {
                entity_type: ENTITY_TRACKING.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }

        entities
    }

    /// Process a text string through the NER engine and return it with
    /// tagged entities.
    pub fn tag_text(text: &str) -> (String, Vec<TaggedEntity>) {
        let entities = Self::extract_entities(text);
        (text.to_string(), entities)
    }
}

// ---------------------------------------------------------------------------
// Contextual query resolver
// ---------------------------------------------------------------------------

/// Resolves natural-language references against the transcript buffer.
///
/// When the user says "Copy that tracking number" or "What was that address?",
/// this resolver scans the buffer backwards from the current timestamp to
/// find the most recent entity matching the requested type.
pub struct ContextResolver<'a> {
    buffer: &'a TranscriptRingBuffer,
}

impl<'a> ContextResolver<'a> {
    /// Create a new resolver backed by the given transcript buffer.
    pub fn new(buffer: &'a TranscriptRingBuffer) -> Self {
        Self { buffer }
    }

    /// Resolve a natural-language reference to an entity.
    ///
    /// ## Reference Mapping
    /// - "tracking number" / "tracking" / "track" → `TrackingNumber`
    /// - "phone" / "number" / "call" / "digits" → `PhoneNumber`
    /// - "address" / "place" / "location" → `Address`
    /// - "email" / "e-mail" / "mail" → `Email`
    /// - "url" / "link" / "website" / "site" → `URL`
    /// - "date" / "time" / "when" → `DateTime`
    /// - "amount" / "price" / "cost" / "dollars" / "$" → `FinancialAmount`
    ///
    /// Returns the most recent matching entity, or `None` if no match found.
    pub fn resolve_reference(&self, query: &str) -> Option<TaggedEntity> {
        let query_lower = query.to_lowercase();
        let entity_type = self.classify_reference(&query_lower)?;

        debug!(
            "ContextResolver: query='{}' → entity_type='{}'",
            query, entity_type
        );

        // Search the buffer backwards (newest first) for the requested type.
        self.buffer
            .entries()
            .iter()
            .flat_map(|entry| &entry.entities)
            .find(|e| e.entity_type == entity_type)
            .cloned()
    }

    /// Resolve a reference and return just the value string.
    pub fn resolve_value(&self, query: &str) -> Option<String> {
        self.resolve_reference(query).map(|e| e.value)
    }

    /// Classify a query string into an entity type.
    fn classify_reference(&self, query: &str) -> Option<String> {
        if query.contains("tracking") || query.contains("track") {
            Some(ENTITY_TRACKING.to_string())
        } else if query.contains("phone")
            || query.contains("number")
            || query.contains("call")
            || query.contains("digits")
        {
            Some(ENTITY_PHONE.to_string())
        } else if query.contains("address")
            || query.contains("place")
            || query.contains("location")
        {
            Some(ENTITY_ADDRESS.to_string())
        } else if query.contains("email")
            || query.contains("e-mail")
            || query.contains("mail")
        {
            Some(ENTITY_EMAIL.to_string())
        } else if query.contains("url")
            || query.contains("link")
            || query.contains("website")
            || query.contains("site")
        {
            Some(ENTITY_URL.to_string())
        } else if query.contains("date")
            || query.contains("time")
            || query.contains("when")
        {
            Some(ENTITY_DATE.to_string())
        } else if query.contains("amount")
            || query.contains("price")
            || query.contains("cost")
            || query.contains("dollar")
            || query.contains("$")
        {
            Some(ENTITY_AMOUNT.to_string())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring_buffer::*;

    /// Test NER extraction of phone numbers.
    #[test]
    fn test_extract_phone() {
        let entities = NerEngine::extract_entities("Call me at 555-0199");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_PHONE));
    }

    /// Test NER extraction of email addresses.
    #[test]
    fn test_extract_email() {
        let entities = NerEngine::extract_entities("Email: test@example.com");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_EMAIL));
    }

    /// Test NER extraction of URLs.
    #[test]
    fn test_extract_url() {
        let entities = NerEngine::extract_entities("Visit https://example.com/page");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_URL));
    }

    /// Test NER extraction of addresses.
    #[test]
    fn test_extract_address() {
        let entities = NerEngine::extract_entities("Ship to 123 Main Street, Springfield, IL 62701");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_ADDRESS));
    }

    /// Test NER extraction of dates.
    #[test]
    fn test_extract_date() {
        let entities = NerEngine::extract_entities("Due by March 15, 2026");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_DATE));
    }

    /// Test NER extraction of financial amounts.
    #[test]
    fn test_extract_amount() {
        let entities = NerEngine::extract_entities("Total: $1,299.99");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_AMOUNT));
    }

    /// Test NER extraction of tracking numbers.
    #[test]
    fn test_extract_tracking() {
        let entities = NerEngine::extract_entities("Your UPS tracking number is 1Z999AA10123456784");
        assert!(entities.iter().any(|e| e.entity_type == ENTITY_TRACKING));
    }

    /// Test resolving "Copy that tracking number" from buffer.
    #[test]
    fn test_resolve_tracking_reference() {
        let mut buffer = TranscriptRingBuffer::new();

        // Populate buffer with some transcript entries.
        buffer.push_with_entities(
            "The package is on its way",
            TranscriptSource::System,
            vec![],
        );
        buffer.push_with_entities(
            "Your tracking number is 1Z999AA10123456784",
            TranscriptSource::System,
            NerEngine::extract_entities("Your tracking number is 1Z999AA10123456784"),
        );
        buffer.push_with_entities(
            "Thanks for the update",
            TranscriptSource::User,
            vec![],
        );

        let resolver = ContextResolver::new(&buffer);
        let result = resolver.resolve_reference("Copy that tracking number");
        assert!(result.is_some());
        assert_eq!(result.unwrap().entity_type, ENTITY_TRACKING);
    }

    /// Test resolving "What was that address?" from buffer.
    #[test]
    fn test_resolve_address_reference() {
        let mut buffer = TranscriptRingBuffer::new();

        buffer.push_with_entities(
            "The pharmacy on 4th street is open until 9 PM",
            TranscriptSource::System,
            vec![],
        );
        buffer.push_with_entities(
            "Our office is at 742 Evergreen Avenue, Springfield",
            TranscriptSource::System,
            NerEngine::extract_entities("Our office is at 742 Evergreen Avenue, Springfield"),
        );

        let resolver = ContextResolver::new(&buffer);
        let result = resolver.resolve_reference("What was that address?");
        assert!(result.is_some());
        assert_eq!(result.unwrap().entity_type, ENTITY_ADDRESS);
    }

    /// Test resolving "Save that phone number" from buffer.
    #[test]
    fn test_resolve_phone_reference() {
        let mut buffer = TranscriptRingBuffer::new();

        buffer.push_with_entities(
            "Call me back at 555-0199",
            TranscriptSource::User,
            NerEngine::extract_entities("Call me back at 555-0199"),
        );

        let resolver = ContextResolver::new(&buffer);
        let result = resolver.resolve_reference("Save that phone number");
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, "555-0199");
    }

    /// Test that the resolver returns the most recent matching entity.
    #[test]
    fn test_most_recent_entity() {
        let mut buffer = TranscriptRingBuffer::new();

        buffer.push_with_entities(
            "Old number: 555-1111",
            TranscriptSource::User,
            NerEngine::extract_entities("Old number: 555-1111"),
        );
        buffer.push_with_entities(
            "New number: 555-2222",
            TranscriptSource::User,
            NerEngine::extract_entities("New number: 555-2222"),
        );

        let resolver = ContextResolver::new(&buffer);
        let result = resolver.resolve_reference("What's the number?");
        assert!(result.is_some());
        // Should return the most recent (newest) phone number.
        assert_eq!(result.unwrap().value, "555-2222");
    }

    /// Test that unknown references return None.
    #[test]
    fn test_unknown_reference() {
        let buffer = TranscriptRingBuffer::new();
        let resolver = ContextResolver::new(&buffer);
        let result = resolver.resolve_reference("What was the weather like?");
        assert!(result.is_none());
    }

    /// Test resolve_value convenience method.
    #[test]
    fn test_resolve_value() {
        let mut buffer = TranscriptRingBuffer::new();
        buffer.push_with_entities(
            "Email me at alice@example.com",
            TranscriptSource::User,
            NerEngine::extract_entities("Email me at alice@example.com"),
        );

        let resolver = ContextResolver::new(&buffer);
        let value = resolver.resolve_value("What's the email?");
        assert_eq!(value, Some("alice@example.com".to_string()));
    }
}
