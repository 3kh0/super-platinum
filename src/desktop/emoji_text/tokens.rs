//! HOzI/pH4l shortcode grammar, without entity/HTML decoding. A char scanner
//! preserves the captured explicit ranges without adding a regex dependency.
fn name(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || "-_+'àáâäãèéêëíîïóöôõœúùûüÿçßñÀÁÂÄÃÈÉÊËÍÎÏÓÖÔÕŒÚÙÛÜŸÇÑ＿＋ー々".contains(c)
        || matches!(c,
            '\u{1100}'..='\u{11ff}' | '\u{a960}'..='\u{a97f}' |
            '\u{ac00}'..='\u{d7a3}' | '\u{d7b0}'..='\u{d7ff}' |
            '\u{3139}'..='\u{318f}' | '\u{3041}'..='\u{3096}' |
            '\u{30a0}'..='\u{30ff}' | '\u{ff61}'..='\u{ff9f}' |
            '\u{3400}'..='\u{4db5}' | '\u{4e00}'..='\u{9fcb}' |
            '\u{f900}'..='\u{fa6a}' | '\u{2e80}'..='\u{2fd5}' |
            '\u{ff41}'..='\u{ff5a}' | '\u{ff10}'..='\u{ff19}')
}

pub(crate) fn split(text: &str) -> Vec<(bool, &str)> {
    if text.is_empty() {
        return vec![(false, text)];
    }
    let mut result = Vec::new();
    let (mut plain, mut search) = (0, 0);
    while let Some(offset) = text[search..].find(':') {
        let start = search + offset;
        let body = &text[start + 1..];
        let length: usize = body
            .chars()
            .take_while(|c| name(*c))
            .map(char::len_utf8)
            .sum();
        if length == 0 || body.as_bytes().get(length) != Some(&b':') {
            search = start + 1;
            continue;
        }
        let mut end = start + length + 2;
        if let Some(tone) = text[end..].strip_prefix(":skin-tone-") {
            let bytes = tone.as_bytes();
            let valid = |b: Option<&u8>| b.is_some_and(|b| (b'2'..=b'6').contains(b));
            let tone_length = if valid(bytes.first()) && bytes.get(1) == Some(&b':') {
                Some(2)
            } else if valid(bytes.first())
                && bytes.get(1) == Some(&b'-')
                && valid(bytes.get(2))
                && bytes.get(3) == Some(&b':')
            {
                Some(4)
            } else {
                None
            };
            if let Some(length) = tone_length {
                end += ":skin-tone-".len() + length;
            }
        }
        if plain < start {
            result.push((false, &text[plain..start]));
        }
        result.push((true, &text[start..end]));
        plain = end;
        search = end;
    }
    if plain < text.len() {
        result.push((false, &text[plain..]));
    }
    result
}
