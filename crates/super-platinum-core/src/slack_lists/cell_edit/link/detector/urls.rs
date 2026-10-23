use super::*;

pub(super) fn detect(p: &Patterns, input: &[u16]) -> Result<Vec<Hit>, ListError> {
    detect_with(p, input, false, false, true)
}
pub(super) fn detect_with(
    p: &Patterns,
    input: &[u16],
    ignore_schemeless: bool,
    ignore_unbalanced: bool,
    first_only: bool,
) -> Result<Vec<Hit>, ListError> {
    let mut hits = Vec::new();
    let Some(mut cursor) = earliest_possible_start(p, input) else {
        return Ok(hits);
    };
    while let Some(m) = p.find("url", input, cursor) {
        cursor = m.end().max(cursor + 1);
        let mut value = input[m.range()].to_vec();
        let mut offset = m.start();
        let mut scheme = group(&m, input, 1).unwrap_or_default().to_vec();
        let www = truthy(group(&m, input, 5));
        let bare = group(&m, input, 9).unwrap_or_default();
        let slash = group(&m, input, 6)
            .filter(|v| !v.is_empty())
            .or_else(|| group(&m, input, 10));
        if ignore_schemeless && scheme.is_empty()
            || !bare.is_empty() && p.test("suffix", &input[m.end()..])
        {
            continue;
        }
        let previous = offset.checked_sub(1).and_then(|i| input.get(i)).copied();
        if previous == Some(b'@' as u16)
            || (input.get(offset + value.len()) == Some(&(b'@' as u16))
                && scheme.is_empty()
                && !www
                && !bare.is_empty())
            || (previous == Some(b'/' as u16) && scheme.is_empty() && !www)
        {
            continue;
        }
        if !scheme.is_empty() {
            if let Some(found) = p.find("protocol", &scheme, 0) {
                if group(&found, &scheme, 1).is_some_and(|name| {
                    ["mailto", "javascript", "vbscript"]
                        .iter()
                        .any(|s| name == text(s))
                }) {
                    continue;
                }
            }
        }
        if scheme.is_empty() && !www {
            if let Some(prefix) = p.find("underscore", &value, 0) {
                let end = prefix.end();
                offset += end;
                value.drain(..end);
            }
        }
        let basic = !scheme.is_empty() && p.test("basic", &scheme);
        let spotify = p.test("spotify", &value);
        let colon = index(&value, ":").unwrap_or(0);
        if !spotify
            && (scheme.is_empty() || !basic)
            && !contains(
                if scheme.is_empty() {
                    &value
                } else {
                    &value[colon..]
                },
                ".",
            )
        {
            continue;
        }
        let host_start = if basic {
            index(&value, "://").map(|i| i + 3).unwrap_or(0)
        } else {
            0
        };
        let host = &value[host_start..];
        let host = &host[..index(host, "/").unwrap_or(host.len())];
        if contains(host, "..") {
            continue;
        }
        if !truthy(slash)
            && !bare.is_empty()
            && bare == value
            && p.test("extension", &input[offset + bare.len()..])
        {
            continue;
        }
        // A prefixed basic protocol embedded in a malformed larger scheme wins.
        let nested = ["http", "https", "sftp", "ftp"].iter().find_map(|name| {
            let prefix = format!("{name}://");
            (!scheme.is_empty() && contains(&scheme, &prefix)).then_some(*name)
        });
        if let Some(start) = nested
            .and_then(|name| index(&value, name))
            .filter(|i| *i > 0)
        {
            value.drain(..start);
            scheme.drain(..start);
            offset += start;
        }
        if spotify {
            if let Some(found) = p.find("spotify", &input[offset..], 0) {
                value = input[offset..offset + found.end()].to_vec();
            }
        }
        if previous == Some(b'|' as u16) {
            if let Some(pipe) = index(&value, "|").filter(|i| *i > 0) {
                cursor = offset + pipe;
                value.truncate(pipe);
                while value
                    .last()
                    .is_some_and(|c| text("!:,.;^\u{2026}\u{2014}").contains(c))
                {
                    value.pop();
                }
            }
        }
        if ignore_unbalanced {
            let mut balance = 0i64;
            if let Some(end) = value
                .iter()
                .position(|c| {
                    if *c == 40 {
                        balance += 1;
                    }
                    if *c == 41 {
                        balance -= 1;
                    }
                    balance < 0
                })
                .filter(|end| *end > 0)
            {
                value.truncate(end);
            }
        }
        if let Some((left, right)) = value.last().and_then(|c| match *c {
            41 => Some((40, 41)),
            125 => Some((123, 125)),
            93 => Some((91, 93)),
            _ => None,
        }) {
            let a = value.iter().filter(|c| **c == left).count();
            let b = value.iter().filter(|c| **c == right).count();
            if a + 1 == b {
                value.pop();
            }
        }
        if value.last() == Some(&(b'?' as u16)) {
            value.pop();
        }
        let mut url = if !truthy(slash) && scheme.is_empty() {
            text("http://")
        } else {
            Vec::new()
        };
        url.extend_from_slice(&value);
        let mut hit = Hit {
            offset,
            text: value,
            url,
        };
        if !scheme.is_empty() {
            for (prefix, key) in [
                ("hansoft://", "hansoft"),
                ("rdar://", "rdar"),
                ("daylite://", "daylite"),
            ] {
                if starts(&hit.url, prefix) {
                    if let Some(found) = p.find(key, &input[hit.offset..], 0) {
                        hit.text = input[hit.offset..hit.offset + found.end()].to_vec();
                        hit.url = hit.text.clone();
                    }
                }
            }
        }
        // maybeStripTrailingSingleQuote, then correctMatchContainingCJKChars.
        if hit.url.last() == Some(&(b'\'' as u16)) {
            let last = hit
                .url
                .rsplit(|c| *c == b'/' as u16)
                .next()
                .unwrap_or_default();
            if last.iter().filter(|c| **c == b'\'' as u16).count() % 2 != 0 {
                hit.url.pop();
                hit.text.pop();
            }
        }
        let previous = hit
            .offset
            .checked_sub(1)
            .and_then(|i| input.get(i))
            .copied();
        if p.cjk(previous) && p.cjk(hit.url.last().copied()) {
            let end = hit
                .url
                .iter()
                .rposition(|c| !p.cjk(Some(*c)))
                .map(|i| i + 1)
                .unwrap_or(0);
            let truncated = &hit.url[..end];
            if !truncated.is_empty() && !p.test("cjk", truncated) {
                hit.url.truncate(end);
                let end = hit
                    .text
                    .iter()
                    .rposition(|c| !p.cjk(Some(*c)))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                hit.text.truncate(end);
            }
        }
        cursor = cursor.max(hit.offset + hit.text.len());
        if !uncommon(p, &hit)? {
            hits.push(hit);
            // Link normalization consumes only the earliest final match. Later
            // URLs cannot win; an earlier email/phone spanning a later URL also
            // spans this first URL and is already excluded by mergeMatches.
            // Do not apply this optimization to a future full-match API.
            if first_only {
                break;
            }
        }
    }
    Ok(hits)
}

// Captured default w at S4600411 has exactly three alternatives: an ASCII
// scheme, optional // + www., or optional // + a bounded domain and known TLD.
// Compute only a necessary lower bound; keep the full original input so regex
// assertions, capture offsets and the source postfilters remain unchanged.
// In non-u /i, ASCII pattern letters cannot case-fold from non-ASCII units.
pub(super) fn earliest_possible_start(p: &Patterns, input: &[u16]) -> Option<usize> {
    let mut earliest: Option<usize> = None;
    let mut dots = std::collections::VecDeque::with_capacity(127);
    let mut prefix = String::with_capacity(p.common_max);
    for (index, unit) in input.iter().enumerate() {
        if earliest == Some(0) {
            return earliest;
        }
        let mut candidate = None;
        if *unit == b':' as u16 && index + 1 < input.len() {
            // [A-Za-z][-.+A-Za-z0-9]{0,63}: requires a nonempty host after
            // the colon. Scan only its maximum 64-unit scheme prefix.
            for start in (index.saturating_sub(64)..index).rev() {
                let Ok(byte) = u8::try_from(input[start]) else {
                    break;
                };
                if !(byte.is_ascii_alphanumeric() || b"-.+".contains(&byte)) {
                    break;
                }
                if byte.is_ascii_alphabetic() {
                    candidate = Some(start);
                }
            }
        }
        if input.get(index..index + 4).is_some_and(|units| {
            units.iter().zip(b"www.").all(|(unit, byte)| {
                u8::try_from(*unit).is_ok_and(|u| u.to_ascii_lowercase() == *byte)
            })
        }) {
            candidate = Some(candidate.map_or(index.saturating_sub(2), |old| {
                old.min(index.saturating_sub(2))
            }));
        }
        if *unit == b'.' as u16 {
            prefix.clear();
            let mut tld = false;
            for unit in input[index + 1..].iter().take(p.common_max) {
                let Ok(byte) = u8::try_from(*unit) else { break };
                if !byte.is_ascii_alphabetic() {
                    break;
                }
                prefix.push(char::from(byte.to_ascii_lowercase()));
                if prefix == "now" || p.common.contains(prefix.as_str()) {
                    tld = true;
                    break;
                }
            }
            if tld {
                // Domain: one 1..63-unit label plus <=126 dot/label pairs.
                // Its classes exclude '.', so no domain can start before the
                // 127th preceding dot. IPv4 is shorter and obeys both bounds.
                let separator_bound = if dots.len() == 127 {
                    dots.front().copied().unwrap() + 1
                } else {
                    0
                };
                let start = index
                    .saturating_sub(127 * 63 + 126)
                    .max(separator_bound)
                    .saturating_sub(2); // optional //
                candidate = Some(candidate.map_or(start, |old| old.min(start)));
            }
            if dots.len() == 127 {
                dots.pop_front();
            }
            dots.push_back(index);
        }
        if let Some(candidate) = candidate {
            earliest = Some(earliest.map_or(candidate, |old| old.min(candidate)));
        }
    }
    earliest
}

fn uncommon(p: &Patterns, hit: &Hit) -> Result<bool, ListError> {
    let url = string(&hit.url)?;
    let label = string(&hit.text)?;
    if url == format!("http://{label}") && p.test("uncommon", &hit.text) {
        return Ok(true);
    }
    // PSzk uses WHATWG URL. Most candidates are absolute; the synthetic base
    // matters only to malformed/relative candidates, never to outgoing text.
    let parsed = url::Url::parse(&url).or_else(|_| {
        url::Url::parse("https://example.invalid/oracle/")
            .unwrap()
            .join(&url)
    });
    let parsed = parsed.ok();
    if parsed
        .as_ref()
        .is_some_and(|u| matches!(u.scheme(), "hansoft" | "spotify"))
    {
        return Ok(false);
    }
    let hostname = parsed.as_ref().and_then(|u| u.host_str()).unwrap_or("");
    if hostname.is_empty() {
        return Ok(!label.is_empty() && !label.contains("://"));
    }
    if !hostname.contains('.') {
        return Ok(false);
    }
    let tld = hostname.rsplit('.').next().unwrap_or("");
    let www = hostname.starts_with("www.") && hostname.split('.').count() >= 3;
    if p.common.contains(tld) || hostname == "ms.now" {
        if p.country.contains(tld)
            && !www
            && parsed
                .as_ref()
                .is_none_or(|u| u.path().is_empty() || u.path() == "/")
            && url == format!("http://{label}")
        {
            let original = label
                .split(['/', ':', '?', '#'])
                .next()
                .unwrap_or("")
                .rsplit('.')
                .next()
                .unwrap_or("");
            if original != tld && original != tld.to_uppercase() {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    Ok(!(www || p.test("basic", &hit.text)))
}
