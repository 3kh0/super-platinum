//! Ordinary own-profile text fields; permissions and mutation ownership belong
//! to the caller. No title/section, status, photo, or custom-field writes.
//!
//! Captured Slack: `ehbb` (shared bundle 4886732), `ZOnG` (limits 753866,
//! reserved values 757099), and `rguE` (boot async 15164190, NFC on change).
use icu_normalizer::ComposingNormalizerBorrowed;

mod emoji;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    FullName,
    DisplayName,
    Pronouns,
}
impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Self::FullName => "real_name",
            Self::DisplayName => "display_name",
            Self::Pronouns => "pronouns",
        }
    }
    pub fn max_utf16(self) -> usize {
        80
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationError {
    Required,
    TooLong { max_utf16: usize },
    LeadingAt,
    RestrictedCharacter,
    ReservedName,
    PunctuationOnly,
    Emoji,
}

/// Slack's text-field `value.normalize()` uses NFC, without trimming the draft.
pub fn normalize(value: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(value)
        .into_owned()
}

pub fn normalize_and_validate(field: Field, value: &str) -> Result<String, ValidationError> {
    let normalized = normalize(value);
    validate(field, &normalized)?;
    Ok(normalized)
}

/// Validate the mounted field value (normally already NFC). Length and reserved
/// names use JS trim; punctuation-only and leading-@ checks use the original
/// value. Preserve that distinction, including Slack's space-padded punctuation.
/// Returns the first error in the captured validator's order.
pub fn validate(field: Field, value: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim_matches(js_whitespace);
    if field == Field::FullName && trimmed.is_empty() {
        return Err(ValidationError::Required);
    }
    if trimmed.encode_utf16().count() > field.max_utf16() {
        return Err(ValidationError::TooLong {
            max_utf16: field.max_utf16(),
        });
    }
    if field == Field::Pronouns || value.is_empty() {
        return Ok(());
    }
    if value.starts_with('@') {
        return Err(ValidationError::LeadingAt);
    }
    if trimmed.contains(|c| r#"`~!@#$%^&*+=\:"<>?|"#.contains(c)) {
        return Err(ValidationError::RestrictedCharacter);
    }
    let lowercase = trimmed.to_lowercase();
    if reserved().iter().any(|word| word == &lowercase) {
        return Err(ValidationError::ReservedName);
    }
    if value.chars().all(|c| "-_.'‘’\",(){}[]/;".contains(c)) {
        return Err(ValidationError::PunctuationOnly);
    }
    if emoji::contains(trimmed) {
        return Err(ValidationError::Emoji);
    }
    Ok(())
}

fn reserved() -> &'static [String] {
    static WORDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let words: Vec<String> = serde_json::from_str(include_str!("profile_text/reserved.json"))
            .expect("captured profile reserved-name data");
        // Slack compares both the trimmed input and each reserved entry after
        // toLowerCase(); some captured entries themselves contain capitals.
        words.into_iter().map(|word| word.to_lowercase()).collect()
    })
}

// ECMAScript/Lodash trim, deliberately not Rust's Unicode White_Space set:
// FEFF is stripped; NEL (0085), ZWSP (200B), and MVS (180E) are not.
fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
