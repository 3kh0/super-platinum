//! jytj word-boundary matcher over JavaScript UTF-16 code units.
//! Inputs are already normalized by the retained hNDQ producer contract.

#[derive(Clone, Copy, Debug)]
pub struct FuzzyOptions {
    pub limit: u32,
    pub jump_limit: usize,
    pub cjk: bool,
    pub forward_only: bool,
    pub substrings_only: bool,
    /// Actual `matchBeyondWordBoundaries && Intl.Segmenter(query).length >= 4`.
    /// The caller supplies this observation; default workflow options leave it false.
    pub can_match_beyond_word_boundaries: bool,
}
impl Default for FuzzyOptions {
    fn default() -> Self {
        Self {
            limit: 3,
            jump_limit: 20,
            cjk: true,
            forward_only: false,
            substrings_only: false,
            can_match_beyond_word_boundaries: false,
        }
    }
}
fn separator(c: u16) -> bool {
    matches!(
        c,
        0x2d | 0x20 | 0x5f | 0x27 | 0x2e | 0x2019 | 0x200b | 0x28 | 0x29 | 0x2f | 0xff0f
    )
}
// Exact ZOnG.kw ranges consumed by CwFm; deliberately UTF-16 rather than Unicode
// scalar classification (supplementary-plane surrogate halves are not CJK here).
fn cjk(c: u16) -> bool {
    matches!(c,
        0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff | 0xff00..=0xffef |
        0xac00..=0xd7af | 0x1100..=0x11ff | 0x3130..=0x318f | 0xa960..=0xa97f |
        0xd7b0..=0xd7ff | 0x3400..=0x4dbf | 0x4e00..=0x62ff | 0x6300..=0x77ff |
        0x7800..=0x8cff | 0x8d00..=0x9fff | 0x2e80..=0x2eff | 0x2f00..=0x2fdf |
        0x2ff0..=0x2fff | 0x3001..=0x303f | 0x31c0..=0x31ef | 0x3200..=0x32ff |
        0x3300..=0x33ff | 0xf900..=0xfaff | 0xfe30..=0xfe4f)
}
#[derive(Clone)]
pub struct FuzzyMatcher {
    query: Vec<u16>,
    no_separators: Vec<u16>,
    options: FuzzyOptions,
}
impl FuzzyMatcher {
    pub fn new(normalized_query: &str, options: FuzzyOptions) -> Self {
        let all: Vec<_> = normalized_query.encode_utf16().collect();
        Self {
            query: all.iter().take(1024).copied().collect(),
            no_separators: all.into_iter().filter(|c| !separator(*c)).collect(),
            options,
        }
    }
    fn query_separator(&self, index: usize) -> bool {
        separator(self.query[index]) && (index != 0 || self.query.len() > 1)
    }
    fn beyond(&self, text: &[u16]) -> Option<u32> {
        let query = &self.no_separators;
        (self.options.can_match_beyond_word_boundaries
            && (query.is_empty() || text.windows(query.len()).any(|w| w == query)))
        .then_some(2.min(self.options.limit))
    }
    pub fn score(&self, normalized_text: &str) -> Option<u32> {
        let text: Vec<_> = normalized_text.encode_utf16().collect();
        if text.is_empty() || self.query.is_empty() {
            return None;
        }
        let compact: Vec<_> = text.iter().copied().filter(|c| !separator(*c)).collect();
        if self.no_separators.len() > compact.len()
            || self
                .query
                .iter()
                .enumerate()
                .any(|(i, c)| !self.query_separator(i) && !text.contains(c))
        {
            return None;
        }
        let cap = if self.options.jump_limit == 0 {
            5
        } else {
            self.options.jump_limit
        };
        let jumps: Vec<_> = text
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                (!separator(c)
                    && (i == 0
                        || separator(text[i - 1])
                        || (self.options.cjk && (cjk(text[i - 1]) != cjk(c) || cjk(c)))))
                .then_some(i)
            })
            .take(cap)
            .collect();
        let mut visited = vec![false; text.len()];
        let mut found = self.walk(&text, &jumps, &mut visited, 0, 0, 0);
        if found.is_none() {
            for &jump in &jumps {
                if jump != 0 {
                    found = self.walk(&text, &jumps, &mut visited, jump, 0, 1);
                    if found.is_some() {
                        break;
                    }
                }
            }
        }
        match found {
            None => self.beyond(&compact),
            Some(n) if n > self.options.limit => self.beyond(&compact),
            Some(n) if n > 2 => Some(self.beyond(&compact).map_or(n, |b| n.min(b))),
            Some(n) => Some(n),
        }
    }
    fn walk(
        &self,
        text: &[u16],
        jumps: &[usize],
        visited: &mut [bool],
        at: usize,
        query: usize,
        count: u32,
    ) -> Option<u32> {
        if at >= text.len()
            || visited[at]
            || !(text[at] == self.query[query]
                || separator(text[at]) && self.query_separator(query))
        {
            return None;
        }
        if query + 1 == self.query.len() {
            return Some(count);
        }
        visited[at] = true;
        let mut next = self.walk(text, jumps, visited, at + 1, query + 1, 0);
        let mut skip = query + 1;
        while self.query_separator(skip) && skip + 1 < self.query.len() {
            skip += 1;
        }
        if next.is_none() {
            for &jump in jumps {
                if visited[jump]
                    || jump == at
                    || ((self.options.forward_only || self.options.substrings_only) && jump < at)
                    || (self.options.substrings_only && jump > at + 2)
                {
                    continue;
                }
                next = self.walk(text, jumps, visited, jump, skip, 1);
                if next.is_some() {
                    break;
                }
            }
        }
        visited[at] = false;
        next.map(|n| count + n)
    }
}
