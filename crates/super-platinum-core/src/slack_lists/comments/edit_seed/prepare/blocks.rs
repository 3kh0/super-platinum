//! k1tX list and fenced-code passes, preserving source UTF-16 positions.
use super::*;
use patterns::{Patterns, group};
use serde_json::Map;
use std::collections::BTreeMap;

pub(super) fn lists(document: &mut Document, mut long_quote: bool) -> Result<bool> {
    let p = Patterns::get()?;
    let text = regex_text(document)?;
    let code = p.all("SM/6:u_", &text);
    let quote = p
        .first("Y1wK:qv", &text)
        .or_else(|| p.first("Y1wK:K", &text));
    let quoted = quote.is_some();
    if quoted {
        long_quote = true;
    }
    let ordered_sequence = p.test("SM/6:Dk", &text);
    let mut previous_indent: Option<usize> = None;
    let mut previous_start = 0;
    let mut previous_length = 0;
    let mut counts = BTreeMap::<usize, usize>::new();
    let mut remove = vec![];
    for found in p.all("SM/6:AY", &text) {
        let start = found.range.start;
        let prefix_end = start
            + (1..=4)
                .map(|i| group(&found, &text, i).len())
                .sum::<usize>();
        if code.iter().any(|m| {
            start > m.range.start && prefix_end + group(&found, &text, 5).len() <= m.range.end
        }) {
            continue;
        }
        let symbol = group(&found, &text, 3);
        let ordered = p.test("SM/6:X$", symbol);
        let indent = (group(&found, &text, 2).len() / 4).min(4);
        let first_equal = text
            .windows(found.range.len())
            .position(|part| part == &text[found.range.clone()])
            .ok_or(SeedUnavailable::Patterns)?;
        let tail = &text[first_equal..];
        let before = &text[first_equal.saturating_sub(2)..first_equal];
        let one = symbol == [b'1' as u16, b'.' as u16];
        if ordered {
            if !ordered_sequence
                || indent == 0 && !counts.contains_key(&indent) && one && !p.test("SM/6:IK", tail)
            {
                continue;
            }
            if before.windows(2).any(|x| x == [10, 10]) && !one {
                counts.clear();
                continue;
            }
            if !counts.contains_key(&indent) && !one {
                continue;
            }
        }
        let adjacent = previous_start + previous_length == start;
        let nested = previous_indent.is_some_and(|old| old < indent);
        if !adjacent || nested {
            counts.insert(indent, 0);
        }
        let mut attributes = json!({"list":if ordered {"ordered"} else {"bullet"},"indent":indent})
            .as_object()
            .unwrap()
            .clone();
        if (!group(&found, &text, 1).is_empty() || quoted)
            && quote.as_ref().is_none_or(|m| start > m.range.start)
        {
            attributes.insert("border".into(), json!(1));
        }
        if previous_indent.is_some_and(|old| old != indent)
            && adjacent
            && counts.get(&indent).is_some_and(|v| *v > 0)
        {
            attributes.insert("offset".into(), json!(counts[&indent]));
        }
        *counts.entry(indent).or_default() += 1;
        previous_indent = Some(indent);
        previous_start = start;
        previous_length = found.range.len();
        let mut change = Change::default();
        change.retain(start);
        if !group(&found, &text, 6).is_empty() {
            change.retain(found.range.len() - 1);
            change.annotate(1, attributes);
        } else {
            change.retain(found.range.len());
            change.insert("\n", attributes);
        }
        document.apply(&change)?;
        remove.extend(start..prefix_end);
    }
    remove.sort_unstable_by(|a, b| b.cmp(a));
    for index in remove {
        let mut change = Change::default();
        change.retain(index);
        change.delete(1);
        document.apply(&change)?;
    }
    Ok(long_quote)
}

struct Fence {
    start: usize,
    tick_start: usize,
    tick_end: usize,
    prefix: Vec<u16>,
    body: Vec<u16>,
    suffix: Vec<u16>,
}
pub(super) fn code(document: &mut Document, mut long_quote: bool) -> Result<bool> {
    let p = Patterns::get()?;
    let text = regex_text(document)?;
    let quote = p
        .first("Y1wK:qv", &text)
        .or_else(|| p.first("Y1wK:K", &text));
    let tick = text
        .windows(3)
        .position(|s| s == [96, 96, 96] || s == [0xff40, 0xff40, 0xff40]);
    let quote_before = quote.is_some_and(|q| q.range.start < tick.unwrap_or(0));
    if !quote_before {
        long_quote = false;
    }
    let mut fences = vec![];
    for key in ["SM/6:u_", "SM/6:Xr"] {
        for found in p.all(key, &text) {
            let prefix = group(&found, &text, 1).to_vec();
            let body = group(&found, &text, 2).to_vec();
            if body.is_empty() {
                continue;
            }
            let mut suffix = group(&found, &text, 3).to_vec();
            if let Some(next) = text.get(found.range.end).copied().filter(|c| space(*c)) {
                suffix.push(next);
            }
            let start = found.range.start;
            fences.push(Fence {
                start,
                tick_start: start + prefix.len() - 3,
                tick_end: start + prefix.len() + body.len() + 3,
                prefix,
                body,
                suffix,
            });
        }
    }
    fences.sort_by_key(|f| f.start);
    let mut accepted: Vec<Fence> = vec![];
    for fence in fences {
        if accepted
            .last()
            .is_none_or(|old| old.tick_end <= fence.tick_start)
        {
            accepted.push(fence);
        }
    }
    accepted.reverse();
    for (index, fence) in accepted.iter().enumerate() {
        let mut change = Change::default();
        change.retain(fence.start);
        let border = p.test("Y1wK:zd", &fence.prefix) || quote_before || long_quote;
        let mut attributes = Map::from_iter([("code-block".into(), json!(true))]);
        if border {
            attributes.insert("border".into(), json!(1));
        }
        let starts_newline = fence.prefix.first() == Some(&10);
        if starts_newline {
            change.retain(1);
            delete_ticks(&fence.prefix[1..], &mut change);
        } else {
            delete_ticks(&fence.prefix, &mut change);
        }
        if !starts_newline && fence.start > 0 {
            change.insert("\n", Map::new());
        }
        if accepted
            .get(index + 1)
            .is_some_and(|next| fence.start == next.tick_end)
        {
            change.insert("\n", Map::new());
        }
        let lines: Vec<_> = fence.body.split(|c| *c == 10).collect();
        for (index, line) in lines.iter().enumerate() {
            change.retain(line.len());
            if index == 0 && line.is_empty() {
                change.delete(1);
            } else if index != lines.len() - 1 {
                change.annotate(1, attributes.clone());
            }
        }
        if lines.last().is_some_and(|line| !line.is_empty()) {
            change.insert("\n", attributes);
        }
        delete_ticks(&fence.suffix, &mut change);
        document.apply(&change)?;
    }
    Ok(long_quote)
}
fn delete_ticks(text: &[u16], change: &mut Change) {
    for &unit in text {
        if matches!(unit, 96 | 0xff40 | 62 | 0xff1e) || space(unit) {
            change.delete(1);
        } else {
            change.retain(1);
        }
    }
}
fn space(c: u16) -> bool {
    matches!(c,9..=13|0x20|0xa0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
