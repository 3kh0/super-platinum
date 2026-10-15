use super::*;

pub(super) fn context(
    op: &Op,
    emoji: &Value,
    strip: bool,
    pre: bool,
    expand: bool,
) -> Result<Vec<Value>, Unsupported> {
    if !op.insert.is_string() {
        return emoji_chunk(op, emoji, pre);
    }
    if op.attrs.contains_key("slackmention") {
        return mention(op, pre);
    }
    if let Some(url) = op.attrs.get("link").and_then(Value::as_str) {
        let insert = op.insert.as_str().unwrap();
        let pieces: Vec<_> = insert.split("[\u{2026}]").collect();
        let truncated = expand
            && pieces.len() == 2
            && !pieces[0].is_empty()
            && !pieces[1].is_empty()
            && url.starts_with(pieces[0])
            && url[pieces[0].len()..].contains(pieces[1]);
        let mut value = json!({"type":"link","url":url});
        if insert != url && !truncated {
            value["text"] = insert.into();
        }
        return Ok(vec![if pre {
            value
        } else {
            text::annotate(value, op)
        }]);
    }
    if pre {
        let value = op.insert.as_str().ok_or(Unsupported::Content)?;
        let value = if strip {
            value.trim_end_matches(space)
        } else {
            value
        };
        return Ok(if value.is_empty() {
            vec![]
        } else {
            vec![json!({"type":"text","text":value})]
        });
    }
    text::convert(op, strip, false)
}
fn mention(op: &Op, pre: bool) -> Result<Vec<Value>, Unsupported> {
    let meta = &op.attrs["slackmention"];
    let id = meta["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(Unsupported::Content)?;
    let label = meta["label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(Unsupported::Content)?;
    let insert = op.insert.as_str().unwrap();
    // Source may discard extra non-label suffixes on malformed mention ops.
    // Refuse such input instead; never silently discard caller-authored text.
    let count = insert.matches(label).count();
    if count == 0 || count > 512 || label.repeat(count) != insert {
        return Err(Unsupported::Content);
    }
    if pre || op.attrs.get("code") == Some(&Value::Bool(true)) {
        return text::convert(op, false, true);
    }
    let mut value = if matches!(id, "BKeveryone" | "BKchannel" | "BKhere" | "BKgroup") {
        json!({"type":"broadcast","range":&id[2..]})
    } else {
        if id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(Unsupported::Content);
        }
        match id.as_bytes()[0] {
            b'U' | b'W' => json!({"type":"user","user_id":id}),
            b'S' => json!({"type":"usergroup","usergroup_id":id}),
            b'C' | b'G' | b'D' | b'Z'
                if id.len() >= 9
                    && id[1..]
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) =>
            {
                json!({"type":"channel","channel_id":id})
            }
            _ => return Err(Unsupported::Content),
        }
    };
    if let Some(tab) = meta.get("tabId") {
        let tab = tab
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or(Unsupported::Content)?;
        if !matches!(id.as_bytes()[0], b'C' | b'G' | b'D' | b'Z') {
            return Err(Unsupported::Content);
        }
        value["tab_id"] = tab.into();
    }
    Ok(vec![text::annotate(value, op); count])
}
fn emoji_chunk(op: &Op, table: &Value, pre: bool) -> Result<Vec<Value>, Unsupported> {
    let wire = op.insert["slackemoji"]["text"]
        .as_str()
        .ok_or(Unsupported::Emoji)?;
    if pre || op.attrs.get("code") == Some(&Value::Bool(true)) {
        return text::convert(
            &Op {
                insert: wire.into(),
                attrs: Map::new(),
            },
            false,
            true,
        );
    }
    // Exact supported shortcode population. The original op survives separately.
    let lower = wire.to_ascii_lowercase();
    let lowered = lower.strip_prefix(':').unwrap_or(&lower);
    let lowered = lowered.strip_suffix(':').unwrap_or(lowered);
    let (name, tone) = match lowered.split_once("::skin-tone-") {
        Some((name, tone)) => {
            if tone.is_empty()
                || tone.split('-').count() > 2
                || !tone
                    .split('-')
                    .all(|s| s.len() == 1 && matches!(s.as_bytes()[0], b'2'..=b'6'))
            {
                return Err(Unsupported::Emoji);
            }
            (name, Some(tone))
        }
        None => (lowered, None),
    };
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
    {
        return Err(Unsupported::Emoji);
    }
    let mut result = if let Some(base) = table.get(name) {
        if !base.is_object() {
            return Err(Unsupported::Emoji);
        }
        let row = tone
            .and_then(|tone| base.get("skinVariations").and_then(|v| v.get(tone)))
            .unwrap_or(base);
        let name = row["name"]
            .as_str()
            .and_then(|s| s.split("::").next())
            .filter(|s| !s.is_empty())
            .ok_or(Unsupported::Emoji)?;
        let unicode = row["unicode"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(Unsupported::Emoji)?;
        json!({"type":"emoji","name":name,"unicode":unicode})
    } else {
        let mut value = json!({"type":"emoji","name":name});
        if let Some(tone) = tone {
            let number = tone.parse::<u64>().map_err(|_| Unsupported::Emoji)?;
            value["skin_tone"] = number.into();
        }
        value
    };
    result = text::annotate(result, op);
    Ok(vec![result])
}
