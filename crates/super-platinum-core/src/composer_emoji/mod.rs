//! Source-backed shortcode grammar. This is deliberately separate from reference
//! completion: an emoji is text/content, never a Slack reference token.
//! See README.md in the preparation for source provenance and remaining gates.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub user_solicited: bool,
    pub japanese: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// UTF-8 offsets into the unmodified composer wire text.
    pub start: usize,
    pub end: usize,
    pub query: String,
}

impl Trigger {
    pub fn at(text: &str, anchor: usize, focus: usize, options: Options) -> Option<Self> {
        if anchor != focus || !text.is_char_boundary(focus) {
            return None;
        }
        let before = text.get(..focus)?;
        let (start, marker) = before
            .char_indices()
            .rev()
            .find(|(_, c)| matches!(c, ':' | '\u{ff1a}' | '|') || !query_character(*c))?;
        if !matches!(marker, ':' | '\u{ff1a}' | '|') {
            return None;
        }
        let prefix = before[..start].chars().next_back();
        if prefix.is_some_and(|c| !allowed_prefix(c) && !matches!(c, '+' | '-')) {
            return None;
        }
        let matched = &before[start..];
        // UDJi's global JS RegExp retains lastIndex across calls. Do not copy
        // that accidental alternating emoticon acceptance into a pure helper.
        if matches!(
            matched,
            ":D" | ":-D" | ":p" | ":-p" | ":P" | ":-P" | ":b" | ":-b" | ":o" | ":-o" | ":O" | ":-O"
        ) {
            return None;
        }
        let initial_reaction = start == 1 && matches!(prefix, Some('+' | '-'));
        let minimum = if options.japanese || initial_reaction {
            2
        } else {
            3
        };
        if !options.user_solicited && matched.encode_utf16().count() < minimum {
            return None;
        }
        Some(Self {
            start,
            end: focus,
            // /Q7n strips both colon forms; the observed pipe trigger remains
            // part of the query and normally yields no matching emoji names.
            query: matched
                .chars()
                .filter(|c| !matches!(c, ':' | '\u{ff1a}'))
                .collect(),
        })
    }

    pub fn current(&self, text: &str, anchor: usize, focus: usize, options: Options) -> bool {
        Self::at(text, anchor, focus, options).as_ref() == Some(self)
    }
}

fn query_character(c: char) -> bool {
    !js_whitespace(c) && !"~@#$%^&*()=[]{}\\|!?`;:\u{ff1a}\",.<>/".contains(c)
}

fn allowed_prefix(c: char) -> bool {
    js_whitespace(c)
        || "{[(*_/\"\u{201c}'\u{2018}\u{2019}<:".contains(c)
        || matches!(c as u32,
            0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff |
            0xff00..=0xffef | 0xac00..=0xd7af | 0x1100..=0x11ff |
            0x3130..=0x318f | 0xa960..=0xa97f | 0xd7b0..=0xd7ff |
            0x3400..=0x4dbf | 0x4e00..=0x62ff | 0x6300..=0x77ff |
            0x7800..=0x8cff | 0x8d00..=0x9fff | 0x2e80..=0x2eff |
            0x2f00..=0x2fdf | 0x2ff0..=0x2fff | 0x3001..=0x303f |
            0x31c0..=0x31ef | 0x3200..=0x32ff | 0x3300..=0x33ff |
            0xf900..=0xfaff | 0xfe30..=0xfe4f)
}

fn js_whitespace(c: char) -> bool {
    matches!(c as u32, 9..=13 | 32 | 160 | 5760 | 8192..=8202 |
        8232 | 8233 | 8239 | 8287 | 12288 | 65279)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(text: &str) -> Option<Trigger> {
        Trigger::at(text, text.len(), text.len(), Options::default())
    }

    #[test]
    fn shortcode_boundaries_are_not_mention_boundaries() {
        for text in [
            ":sm", "(:sm", "[:sm", "/:sm", "::sm", "。:sm", "：sm", ":+1", ":-1",
        ] {
            assert!(matches(text).is_some(), "{text}");
        }
        for text in [
            ":", ":s", ":smile:", "word:sm", "\\:sm", "`:sm", "a.:sm", ":a.b",
        ] {
            assert!(matches(text).is_none(), "{text}");
        }
    }

    #[test]
    fn solicitation_locale_and_utf16_length_are_explicit() {
        assert!(matches("+:s").is_some());
        assert!(matches("x+:s").is_none());
        assert!(matches(":😀").is_some());
        assert!(
            Trigger::at(
                ":",
                1,
                1,
                Options {
                    user_solicited: true,
                    japanese: false
                }
            )
            .is_some()
        );
        assert!(
            Trigger::at(
                ":s",
                2,
                2,
                Options {
                    user_solicited: false,
                    japanese: true
                }
            )
            .is_some()
        );
    }

    #[test]
    fn known_emoticons_never_alternate_after_repeated_queries() {
        for _ in 0..4 {
            for text in [":D", ":-D", ":p", ":-p", ":O", ":-O"] {
                assert!(
                    Trigger::at(
                        text,
                        text.len(),
                        text.len(),
                        Options {
                            user_solicited: true,
                            japanese: false
                        }
                    )
                    .is_none()
                );
            }
        }
    }

    #[test]
    fn code_and_url_are_representation_context_not_blanket_grammar_refusals() {
        assert!(matches("` :sm").is_some());
        assert!(matches("https://example.invalid/:sm").is_some());
        assert!(matches("https:sm").is_none());
    }

    #[test]
    fn stale_caret_text_and_utf8_boundaries_refuse() {
        let text = "😀 :sm";
        let trigger = matches(text).unwrap();
        assert_eq!(trigger.start, 5);
        assert!(trigger.current(text, text.len(), text.len(), Options::default()));
        assert!(!trigger.current("😀 :sn", text.len(), text.len(), Options::default()));
        assert!(Trigger::at(text, 1, 1, Options::default()).is_none());
        assert!(Trigger::at(text, 0, text.len(), Options::default()).is_none());
    }

    #[test]
    fn independent_primary_source_grammar_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle.json")).unwrap();
        for row in oracle["rows"].as_array().unwrap() {
            let text = row["text"].as_str().unwrap();
            let options = Options {
                user_solicited: row["options"]["isUserSolicited"].as_bool().unwrap_or(false),
                japanese: row["options"]["locale"].as_str() == Some("ja-JP"),
            };
            let actual = Trigger::at(text, text.len(), text.len(), options);
            if row["result"].is_null() {
                assert!(actual.is_none(), "{row}");
            } else {
                let actual = actual.expect(&format!("{row}"));
                assert_eq!(
                    text[..actual.start].encode_utf16().count(),
                    row["result"]["index"].as_u64().unwrap() as usize,
                    "{row}"
                );
                assert_eq!(
                    &text[actual.start..actual.end],
                    row["result"]["match"].as_str().unwrap(),
                    "{row}"
                );
            }
        }
    }
}

pub mod admission;
pub mod api;
pub mod catalog;
pub mod changes;
pub mod context;
pub mod history;
pub mod history_ranking;
#[cfg(test)]
mod history_tests;
pub mod insertion;
pub mod keyword;
pub mod model;
pub mod normalize;
pub mod preferences;
pub mod ranking;
pub mod score;
pub mod score_details;
pub mod search;
pub mod standard;
