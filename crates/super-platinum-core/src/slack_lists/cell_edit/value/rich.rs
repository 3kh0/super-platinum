use super::*;

// Bound editor input work and payload size while retaining opaque original nodes.
const MAX_NODES: usize = 10_000;
const MAX_BYTES: usize = 1_048_576;
const MAX_DEPTH: usize = 32;

pub(super) fn validate(blocks: &[Value], originals: &[Value]) -> Result<(), ListError> {
    let mut budget = (MAX_NODES, MAX_BYTES);
    for block in blocks {
        bounded(block, 0, &mut budget)?;
    }
    let existing = std::ptr::eq(blocks, originals);
    for block in blocks {
        node(block, Position::Root, originals, existing)?;
    }
    Ok(())
}
fn bounded(value: &Value, depth: usize, budget: &mut (usize, usize)) -> Result<(), ListError> {
    if depth > MAX_DEPTH || budget.0 == 0 {
        return Err(ListError("This rich-text field is too complex to edit."));
    }
    budget.0 -= 1;
    match value {
        Value::String(text) => {
            budget.1 = budget
                .1
                .checked_sub(text.len())
                .ok_or(ListError("This rich-text field is too large to edit."))?
        }
        Value::Array(values) => {
            for value in values {
                bounded(value, depth + 1, budget)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                budget.1 = budget
                    .1
                    .checked_sub(key.len())
                    .ok_or(ListError("This rich-text field is too large to edit."))?;
                bounded(value, depth + 1, budget)?;
            }
        }
        _ => {}
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Position {
    Root,
    Block,
    Section,
    Inline,
}
fn node(
    value: &Value,
    position: Position,
    originals: &[Value],
    existing: bool,
) -> Result<(), ListError> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ListError("This rich-text node is malformed."))?;
    if matches!(position, Position::Section) && kind != "rich_text_section" {
        return Err(ListError(
            "This rich-text list item cannot be safely edited.",
        ));
    }
    let expected = match kind {
        "rich_text" => {
            if !matches!(position, Position::Root) {
                return Err(ListError("This rich-text block is nested incorrectly."));
            }
            Some(Position::Block)
        }
        "rich_text_section" | "rich_text_quote" | "rich_text_preformatted" => {
            if !matches!(
                position,
                Position::Root | Position::Block | Position::Section
            ) {
                return Err(ListError("This rich-text section is nested incorrectly."));
            }
            Some(Position::Inline)
        }
        "rich_text_list" => {
            if !matches!(position, Position::Root | Position::Block)
                || !matches!(
                    value.get("style").and_then(Value::as_str),
                    Some("bullet" | "ordered")
                )
            {
                return Err(ListError("This rich-text list is malformed."));
            }
            Some(Position::Section)
        }
        _ => None,
    };
    if let Some(position) = expected {
        let children = value
            .get("elements")
            .and_then(Value::as_array)
            .ok_or(ListError("This rich-text block has malformed children."))?;
        for child in children {
            node(child, position, originals, existing)?;
        }
        return Ok(());
    }
    let required = match kind {
        "text" => Some("text"),
        "link" => Some("url"),
        "emoji" => Some("name"),
        "user" => Some("user_id"),
        "channel" => Some("channel_id"),
        "usergroup" => Some("usergroup_id"),
        "broadcast" => Some("range"),
        _ => None,
    };
    if let Some(required) = required {
        if !matches!(position, Position::Inline)
            || value.get(required).and_then(Value::as_str).is_none()
            || value.get("style").is_some_and(|style| !style.is_object())
        {
            return Err(ListError("This rich-text element is malformed."));
        }
        return Ok(());
    }
    // RichEditor stores original opaque tokens in a WeakMap; allow their exact
    // roundtrip (or explicit deletion), but never fabricate new opaque payloads.
    if existing || originals.iter().any(|original| contains(original, value)) {
        Ok(())
    } else {
        Err(ListError(
            "An unsupported rich-text element cannot be created or changed.",
        ))
    }
}
fn contains(original: &Value, wanted: &Value) -> bool {
    original == wanted
        || original
            .get("elements")
            .and_then(Value::as_array)
            .is_some_and(|children| children.iter().any(|child| contains(child, wanted)))
}
