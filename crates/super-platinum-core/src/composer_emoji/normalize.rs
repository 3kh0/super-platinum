//! Slack hNDQ/J3F/ matching normalization, using the captured source's explicit
//! Latin, kana/width and Cyrillic tables rather than general Unicode NFKD.
#[path = "normalization_table.rs"]
mod table;

pub fn normalize(text: &str) -> String {
    let lowered = text.trim_matches(js_whitespace).to_lowercase();
    let mut output = String::with_capacity(lowered.len());
    let mut whitespace = false;
    for c in lowered.chars() {
        // lodash deburr removes these combining-mark blocks only.
        if matches!(c as u32, 0x0300..=0x036f | 0xfe20..=0xfe2f | 0x20d0..=0x20ff) {
            continue;
        }
        if let Some(mapped) = table::mapped(c) {
            for c in mapped.chars() {
                push(&mut output, &mut whitespace, c);
            }
        } else {
            push(&mut output, &mut whitespace, c);
        }
    }
    output
}

fn push(output: &mut String, whitespace: &mut bool, c: char) {
    if js_whitespace(c) {
        if !*whitespace {
            output.push(' ');
        }
        *whitespace = true;
    } else {
        output.push(c);
        *whitespace = false;
    }
}

fn js_whitespace(c: char) -> bool {
    matches!(c as u32, 9..=13 | 32 | 160 | 5760 | 8192..=8202 |
        8232 | 8233 | 8239 | 8287 | 12288 | 65279)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_source_normalization_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle-next.json")).unwrap();
        for row in oracle["normalization"].as_array().unwrap() {
            let text = row["text"].as_str().unwrap();
            assert_eq!(normalize(text), row["result"].as_str().unwrap(), "{row}");
        }
    }

    #[test]
    fn exact_table_and_unsupported_diacritics_are_preserved() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle-next.json")).unwrap();
        for row in oracle["map"].as_array().unwrap() {
            let c = char::from_u32(row["cp"].as_u64().unwrap() as u32).unwrap();
            assert_eq!(table::mapped(c), row["result"].as_str(), "{row}");
        }
        assert_eq!(normalize("ấ"), "ấ");
        assert_eq!(normalize("a\u{85}b"), "a\u{85}b");
        assert_eq!(normalize("\u{1f600}"), "\u{1f600}");
    }
}
