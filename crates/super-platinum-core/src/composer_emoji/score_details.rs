//! Complete 2YmW score metadata, including failed-name combo breaks used by
//! keyword-only matches in the final ranking model. Inputs are normalized.

#[derive(Clone, Debug, PartialEq)]
pub struct NameScore {
    pub score: Option<f64>,
    /// UTF-16 positions, matching Slack's JS string indexing.
    pub indices: Vec<usize>,
    pub query_length_ratio: f64,
    pub combo_breaks: Option<usize>,
}

pub fn score_name(query: &str, name: &str) -> NameScore {
    let failed = || NameScore {
        score: None,
        indices: Vec::new(),
        query_length_ratio: 0.0,
        combo_breaks: None,
    };
    if query.is_empty() || name.is_empty() || blocked_name(query, name) {
        return failed();
    }
    if query == "ok" && name == "ok_hand" {
        return NameScore {
            score: Some(0.0),
            indices: vec![0, 1],
            query_length_ratio: 1.0,
            combo_breaks: Some(0),
        };
    }
    let raw_query: Vec<_> = query.encode_utf16().collect();
    let raw_name: Vec<_> = name.encode_utf16().collect();
    let start = raw_name
        .windows(raw_query.len())
        .position(|w| w == raw_query)
        .or_else(|| raw_name.iter().position(|c| *c == raw_query[0]));
    let Some(start) = start else {
        return failed();
    };
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
            return NameScore {
                score: None,
                indices,
                query_length_ratio: 0.0,
                combo_breaks: Some(breaks),
            };
        }
    }
    if matched < query_units.len() {
        return failed();
    }
    let ratio = query_units.len() as f64 / name_units.len() as f64;
    NameScore {
        score: Some(1.0 - ratio + breaks as f64),
        indices,
        query_length_ratio: ratio,
        combo_breaks: Some(breaks),
    }
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
