//! Exact retained JU63 Lodash words, used only by yL3i's file query tail.
use regress::Regex;
use std::sync::OnceLock;
struct Patterns {
    ascii: Regex,
    unicode_test: Regex,
    unicode: Regex,
}
pub(super) fn count(value: &str) -> usize {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        let raw: serde_json::Value =
            serde_json::from_str(include_str!("file-words.json")).expect("retained JU63 patterns");
        let compile = |key: &str| {
            Regex::new(raw[key].as_str().expect("retained expression"))
                .expect("original JU63 expression")
        };
        Patterns {
            ascii: compile("ascii"),
            unicode_test: compile("unicodeTest"),
            unicode: compile("unicode"),
        }
    });
    let units: Vec<_> = value.encode_utf16().collect();
    let pattern = if patterns
        .unicode_test
        .find_from_ucs2(&units, 0)
        .next()
        .is_some()
    {
        &patterns.unicode
    } else {
        &patterns.ascii
    };
    pattern.find_from_ucs2(&units, 0).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_words_not_space_split() {
        let raw: serde_json::Value = serde_json::from_str(include_str!("file-words.json")).unwrap();
        for case in raw["cases"].as_array().unwrap() {
            assert_eq!(
                count(case["input"].as_str().unwrap()),
                case["count"].as_u64().unwrap() as usize,
                "{case}"
            );
        }
    }
}
