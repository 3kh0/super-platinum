//! Exact inverse of the existing whole-selection Code Block toolbar wire.
//! This is a native encoding subset, not Slack editor normalization.
use super::*;

pub(super) fn block(wire: &str) -> Option<Value> {
    let body = wire.strip_prefix("```\n")?.strip_suffix("\n```")?;
    if body.is_empty() || body.contains("```") {
        return None;
    }
    Some(json!({"type":"rich_text","elements":[{
        "type":"rich_text_preformatted","elements":[{"type":"text","text":body}]
    }]}))
}

pub(super) fn project(
    key: &DraftKey,
    snapshot: &Snapshot,
    section: &Value,
    destination: Value,
    broadcast: bool,
) -> Result<Projection, Unsupported> {
    if !exact_keys(section, &["type", "elements"]) {
        return Err(Unsupported::Shape);
    }
    let runs = section["elements"].as_array().ok_or(Unsupported::Shape)?;
    let [run] = runs.as_slice() else {
        return Err(Unsupported::Shape);
    };
    if !exact_keys(run, &["type", "text"]) || run["type"] != "text" {
        return Err(Unsupported::Shape);
    }
    let body = run["text"].as_str().ok_or(Unsupported::Shape)?;
    let text = format!("```\n{body}\n```");
    let mut encoded = encode(key, &text)?;
    encoded.destinations = vec![destination];
    if encoded != *snapshot {
        return Err(Unsupported::NotReversible);
    }
    Ok(Projection {
        key: key.clone(),
        composer: ComposerState {
            reply_broadcast: broadcast,
            ..ComposerState::with_text(text)
        },
        raw: snapshot.clone(),
    })
}
