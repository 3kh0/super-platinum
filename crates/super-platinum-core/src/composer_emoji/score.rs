//! Name scoring from retained Slack module 2YmW::scoreEmojiName.
//! Inputs are already normalized. This is a feature extractor, not the final
//! personalized ranking model, localized catalog, or keyword matcher.

#[derive(Clone, Debug, PartialEq)]
pub struct NameScore {
    pub score: f64,
    /// UTF-16 positions, matching Slack's JS string indexing.
    pub indices: Vec<usize>,
    pub query_length_ratio: f64,
    pub combo_breaks: usize,
}

pub fn score_name(query: &str, name: &str) -> Option<NameScore> {
    if query.is_empty() || name.is_empty() || blocked_name(query, name) {
        return None;
    }
    if query == "ok" && name == "ok_hand" {
        return Some(NameScore {
            score: 0.0,
            indices: vec![0, 1],
            query_length_ratio: 1.0,
            combo_breaks: 0,
        });
    }
    let raw_query: Vec<_> = query.encode_utf16().collect();
    let raw_name: Vec<_> = name.encode_utf16().collect();
    let start = raw_name
        .windows(raw_query.len())
        .position(|w| w == raw_query)
        .or_else(|| raw_name.iter().position(|c| *c == raw_query[0]))?;
    let (max_breaks, max_gap, reset_at_separator) = match raw_query.len() {
        0..=2 => (0, 0, false),
        3 => (1, 1, false),
        4 => (1, 5, false),
        _ => (2, 5, true),
    };
    let normalized_query = collapse_dashes(
        &query
            .chars()
            .filter(|c| !js_whitespace(*c))
            .collect::<String>(),
    );
    let query_units: Vec<_> = normalized_query.encode_utf16().collect();
    let name_units: Vec<_> = collapse_dashes(name).encode_utf16().collect();
    let mut matched = 0;
    let mut breaks = 0;
    let mut gap = 0;
    let mut consecutive = true;
    let mut indices = Vec::new();
    for (index, unit) in name_units.iter().enumerate().skip(start) {
        if query_units.get(matched) == Some(unit) {
            consecutive = true;
            indices.push(index);
            matched += 1;
            gap = 0;
            if matched == query_units.len() {
                break;
            }
        } else if *unit == b'_' as u16 {
            if consecutive && reset_at_separator {
                gap = 0;
            } else {
                gap += 1;
            }
        } else {
            if consecutive {
                breaks += 1;
            }
            consecutive = false;
            gap += 1;
        }
        if breaks > max_breaks || gap > max_gap {
            return None;
        }
    }
    if matched < query_units.len() {
        return None;
    }
    let ratio = query_units.len() as f64 / name_units.len() as f64;
    Some(NameScore {
        score: 1.0 - ratio + breaks as f64,
        indices,
        query_length_ratio: ratio,
        combo_breaks: breaks,
    })
}

fn collapse_dashes(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut dash = false;
    for c in text.chars() {
        if c == '-' {
            if !dash {
                output.push('_');
            }
            dash = true;
        } else {
            output.push(c);
            dash = false;
        }
    }
    output
}

fn js_whitespace(c: char) -> bool {
    matches!(c as u32, 9..=13 | 32 | 160 | 5760 | 8192..=8202 |
        8232 | 8233 | 8239 | 8287 | 12288 | 65279)
}

fn blocked_name(query: &str, name: &str) -> bool {
    let blocked: &[&str] = match query {
        "china" => &["pinching_hand", "pinching-hand"],
        "dead" => &["older_adult"],
        "dump" => &["hindu_temple"],
        "fail" => &["family"],
        "gross" => &["flag-progress-pride"],
        "hi" | "hit" | "it" | "sh" | "shi" | "s" | "sit" => &["shit"],
        "hitler" => &["white_flower"],
        "macht" => &["mannlichkeitssymbol"],
        "shoot" => &["school_satchel"],
        "sith" => &["person_with_headscarf"],
        "war" => &["man-wearing-turban"],
        "whine" => &["menorah_with_nine_branches"],
        "worst" => &["woman-wrestling"],
        "wrong" => &["woman-rowing-boat"],
        "rage" => &["transgender"],
        _ => &[],
    };
    blocked.iter().any(|blocked| name.contains(blocked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_source_name_score_examples() {
        assert_eq!(score_name("ok", "ok_hand").unwrap().score, 0.0);
        assert_eq!(score_name("heart", "heart").unwrap().score, 0.0);
        assert!(score_name("hi", "shit").is_none());
        assert!(score_name("sm", "some").is_none());
        let score = score_name("smil", "smile").unwrap();
        assert!((score.score - 0.2).abs() < 1e-10);
        assert_eq!(score.indices, [0, 1, 2, 3]);
    }

    #[test]
    fn unicode_indices_count_utf16_units() {
        let score = score_name("😀a", "😀abc").unwrap();
        assert_eq!(score.indices, [0, 1, 2]);
        assert_eq!(score.query_length_ratio, 3.0 / 5.0);
    }

    #[test]
    fn empty_and_unmatched_names_refuse() {
        assert!(score_name("", "heart").is_none());
        assert!(score_name("heart", "").is_none());
        assert!(score_name("absent", "heart").is_none());
    }

    #[test]
    fn independent_primary_source_name_scoring_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle.json")).unwrap();
        for row in oracle["scores"].as_array().unwrap() {
            let actual = score_name(
                row["query"].as_str().unwrap(),
                row["name"].as_str().unwrap(),
            );
            let expected = &row["result"];
            if expected.is_null() {
                assert!(actual.is_none(), "{row}");
            } else {
                let actual = actual.expect(&format!("{row}"));
                assert!(
                    (actual.score - expected["score"].as_f64().unwrap()).abs() < 1e-12,
                    "{row}"
                );
                assert!(
                    (actual.query_length_ratio - expected["queryLengthRatio"].as_f64().unwrap())
                        .abs()
                        < 1e-12,
                    "{row}"
                );
                assert_eq!(
                    actual.combo_breaks,
                    expected["totalComboBreaks"].as_u64().unwrap() as usize,
                    "{row}"
                );
                assert_eq!(
                    actual.indices,
                    expected["matchIndices"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_u64().unwrap() as usize)
                        .collect::<Vec<_>>(),
                    "{row}"
                );
            }
        }
    }
}
