//! Slack jytj keyword matcher on already normalized strings. Graph traversal
//! order matters: the captured client takes its first successful boundary path.

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub fuzzy_limit: usize,
    pub jump_limit: usize,
    pub fuzzy_cjk: bool,
    /// The source enables this only when its feature is on AND Intl.Segmenter
    /// finds at least four query graphemes. The adapter supplies that decision.
    pub beyond_word_boundaries: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fuzzy_limit: 3,
            jump_limit: 20,
            fuzzy_cjk: true,
            beyond_word_boundaries: false,
        }
    }
}

pub fn score(query: &str, name: &str, options: Options) -> Option<usize> {
    if query == name {
        return Some(0);
    }
    let query: Vec<_> = query.encode_utf16().collect();
    let name: Vec<_> = name.encode_utf16().collect();
    if name.is_empty() || query.is_empty() {
        return None;
    }
    let query_count = query.iter().filter(|c| !separator(**c)).count();
    if query_count > name.iter().filter(|c| !separator(**c)).count() {
        return None;
    }
    let original_query = query;
    let query = &original_query[..original_query.len().min(1024)];
    let ignored_query = |i: usize| separator(query[i]) && (i != 0 || query.len() > 1);
    if query
        .iter()
        .enumerate()
        .any(|(i, c)| !ignored_query(i) && !name.contains(c))
    {
        return None;
    }
    let mut jumps = Vec::new();
    for i in 0..name.len() {
        if !separator(name[i])
            && (i == 0
                || separator(name[i - 1])
                || options.fuzzy_cjk && (cjk(name[i - 1]) != cjk(name[i]) || cjk(name[i])))
        {
            if jumps.len()
                < (if options.jump_limit == 0 {
                    5
                } else {
                    options.jump_limit
                })
            {
                jumps.push(i);
            }
        }
    }
    let mut visited = vec![false; name.len()];
    let mut matched = walk(&name, query, &jumps, &mut visited, 0, 0, 0);
    if matched.is_none() {
        for &jump in &jumps {
            if jump != 0 {
                matched = walk(&name, query, &jumps, &mut visited, jump, 0, 1);
                if matched.is_some() {
                    break;
                }
            }
        }
    }
    let beyond = || {
        (options.beyond_word_boundaries && contains_query(&name, &original_query, query_count))
            .then_some(2.min(options.fuzzy_limit))
    };
    match matched {
        None => beyond(),
        Some(score) if score > options.fuzzy_limit => beyond(),
        Some(score) if score > 2 => Some(beyond().map_or(score, |b| score.min(b))),
        score => score,
    }
}

fn walk(
    name: &[u16],
    query: &[u16],
    jumps: &[usize],
    visited: &mut [bool],
    ni: usize,
    qi: usize,
    cost: usize,
) -> Option<usize> {
    if ni >= name.len() || qi >= query.len() || visited[ni] {
        return None;
    }
    let q_separator = separator(query[qi]) && (qi != 0 || query.len() > 1);
    if name[ni] != query[qi] && !(separator(name[ni]) && q_separator) {
        return None;
    }
    if qi + 1 == query.len() {
        return Some(cost);
    }
    visited[ni] = true;
    let mut found = walk(name, query, jumps, visited, ni + 1, qi + 1, 0);
    if found.is_none() {
        let mut next = qi + 1;
        while separator(query[next]) && next + 1 < query.len() {
            next += 1;
        }
        for &jump in jumps {
            if jump == ni || visited[jump] {
                continue;
            }
            found = walk(name, query, jumps, visited, jump, next, 1);
            if found.is_some() {
                break;
            }
        }
    }
    visited[ni] = false;
    found.map(|next| next + cost)
}

fn contains_query(name: &[u16], query: &[u16], query_count: usize) -> bool {
    for start in 0..=name.len().saturating_sub(query_count) {
        let (mut ni, mut qi) = (start, 0);
        while ni < name.len() && qi < query.len() {
            if separator(name[ni]) && separator(query[qi]) {
                while ni < name.len() && separator(name[ni]) {
                    ni += 1;
                }
                while qi < query.len() && separator(query[qi]) {
                    qi += 1;
                }
            } else if name[ni] == query[qi] {
                ni += 1;
                qi += 1;
            } else {
                break;
            }
        }
        if qi == query.len() {
            return true;
        }
    }
    false
}

fn separator(c: u16) -> bool {
    matches!(
        c,
        32 | 45 | 95 | 39 | 46 | 8217 | 8203 | 40 | 41 | 47 | 65295
    )
}

fn cjk(c: u16) -> bool {
    matches!(c,
        0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff |
        0xff00..=0xffef | 0xac00..=0xd7af | 0x1100..=0x11ff |
        0x3130..=0x318f | 0xa960..=0xa97f | 0xd7b0..=0xd7ff |
        0x3400..=0x4dbf | 0x4e00..=0x62ff | 0x6300..=0x77ff |
        0x7800..=0x8cff | 0x8d00..=0x9fff | 0x2e80..=0x2eff |
        0x2f00..=0x2fdf | 0x2ff0..=0x2fff | 0x3001..=0x303f |
        0x31c0..=0x31ef | 0x3200..=0x32ff | 0x3300..=0x33ff |
        0xf900..=0xfaff | 0xfe30..=0xfe4f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_keyword_graph_oracle() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("source-oracle-next.json")).unwrap();
        for row in oracle["keyword"].as_array().unwrap() {
            let query = row["query"].as_str().unwrap();
            // These retained corpus strings have no combining/ZWJ graphemes;
            // their scalar count is exactly their Intl.Segmenter count.
            let options = Options {
                beyond_word_boundaries: row["options"]["matchBeyondWordBoundaries"]
                    .as_bool()
                    .unwrap_or(false)
                    && query.chars().count() >= 4,
                ..Options::default()
            };
            assert_eq!(
                score(query, row["name"].as_str().unwrap(), options),
                row["result"].as_u64().map(|n| n as usize),
                "{row}"
            );
        }
    }

    #[test]
    fn default_keyword_matching_is_not_substring_matching() {
        assert_eq!(score("star", "superstar", Options::default()), None);
        assert_eq!(score("star", "shooting_star", Options::default()), Some(1));
        assert_eq!(
            score(
                "star",
                "superstar",
                Options {
                    beyond_word_boundaries: true,
                    ..Options::default()
                }
            ),
            Some(2)
        );
    }
}
