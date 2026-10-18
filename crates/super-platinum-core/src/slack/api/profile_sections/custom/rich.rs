use super::*;

// Bound editor input work and payload size while retaining opaque original nodes.
const MAX_NODES: usize = 10_000;
const MAX_BYTES: usize = 1_048_576;
const MAX_DEPTH: usize = 32;

pub(super) fn validate(blocks: &[Value], originals: &[Value]) -> Result<(), Error> {
    let mut budget = (MAX_NODES, MAX_BYTES);
    for block in blocks {
        bounded(block, 0, &mut budget)?;
    }
    let existing = std::ptr::eq(blocks, originals);
    for block in blocks {
        node(block, Position::Root, originals, existing)?;
    }
    if measured_length(blocks)? > 5000 {
        return Err(invalid(
            "Use 5,000 characters or fewer for this profile field.",
        ));
    }
    Ok(())
}
fn bounded(value: &Value, depth: usize, budget: &mut (usize, usize)) -> Result<(), Error> {
    if depth > MAX_DEPTH || budget.0 == 0 {
        return Err(invalid("This rich-text field is too complex to edit."));
    }
    budget.0 -= 1;
    match value {
        Value::String(text) => {
            budget.1 = budget
                .1
                .checked_sub(text.len())
                .ok_or(invalid("This rich-text field is too large to edit."))?
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
                    .ok_or(invalid("This rich-text field is too large to edit."))?;
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
) -> Result<(), Error> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(invalid("This rich-text node is malformed."))?;
    if matches!(position, Position::Section) && kind != "rich_text_section" {
        return Err(invalid("This rich-text list item cannot be safely edited."));
    }
    let expected = match kind {
        "rich_text" => {
            if !matches!(position, Position::Root) {
                return Err(invalid("This rich-text block is nested incorrectly."));
            }
            Some(Position::Block)
        }
        "rich_text_section" | "rich_text_quote" | "rich_text_preformatted" => {
            if !matches!(
                position,
                Position::Root | Position::Block | Position::Section
            ) {
                return Err(invalid("This rich-text section is nested incorrectly."));
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
                return Err(invalid("This rich-text list is malformed."));
            }
            Some(Position::Section)
        }
        _ => None,
    };
    if let Some(position) = expected {
        let children = value
            .get("elements")
            .and_then(Value::as_array)
            .ok_or(invalid("This rich-text block has malformed children."))?;
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
            return Err(invalid("This rich-text element is malformed."));
        }
        return Ok(());
    }
    // RichEditor stores original opaque tokens in a WeakMap; allow their exact
    // roundtrip (or explicit deletion), but never fabricate new opaque payloads.
    if existing || originals.iter().any(|original| contains(original, value)) {
        Ok(())
    } else {
        Err(invalid(
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

// B E1L/ converts the supported blocks to a Delta, then B3bvJ measures it.
// In particular an existing trailing newline must not be counted twice, and
// adjacent link runs with identical attributes count the URL only once.
pub(super) fn measured_length(blocks: &[Value]) -> Result<usize, Error> {
    // Retain adjacent attributed text before measuring, as Quill Delta.insert
    // merges adjacent string operations with identical attributes.
    #[derive(Default)]
    struct Ops {
        values: Vec<(String, Option<String>, [bool; 6])>,
    }
    impl Ops {
        fn insert(&mut self, text: String, url: Option<String>, style: [bool; 6]) {
            if text.is_empty() {
                return;
            }
            if let Some((old, link, attrs)) = self.values.last_mut()
                && *link == url
                && *attrs == style
            {
                old.push_str(&text);
                return;
            }
            self.values.push((text, url, style));
        }
        fn newline(&mut self) {
            self.insert("\n".into(), None, [false; 6]);
        }
        fn trailing(&self) -> bool {
            self.values.last().is_some_and(|v| v.0.ends_with('\n'))
        }
        fn inline(&mut self, value: &Value, list: bool, code: bool) -> Result<(), Error> {
            let text = |key: &str| value[key].as_str().unwrap_or("");
            let attrs = ["bold", "italic", "underline", "strike", "code", "unlink"]
                .map(|key| value["style"][key] == true);
            match text("type") {
                "text" => self.insert(
                    if list {
                        text("text").replace('\n', "")
                    } else {
                        text("text").into()
                    },
                    None,
                    attrs,
                ),
                "link" if value["truncated"] != true => {
                    let url = text("url");
                    let label = value["text"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(url);
                    self.insert(
                        label.into(),
                        (!code && !attrs[4]).then(|| url.into()),
                        attrs,
                    );
                }
                "user" | "channel" | "usergroup" if value.get("tab_id").is_none() => {
                    // Canonical mention IDs are measured independently of the
                    // renderer's display label. These have no link attribute.
                    let key = match text("type") {
                        "user" => "user_id",
                        "channel" => "channel_id",
                        _ => "usergroup_id",
                    };
                    self.insert(text(key).into(), None, attrs);
                }
                // Emoji conversion can depend on locale/workspace emoji
                // preference and unicode aliases. Preserve its original raw
                // shape, but do not invent a profile-length authority here.
                _ => {
                    return Err(invalid(
                        "This rich token's profile length cannot yet be verified.",
                    ));
                }
            }
            Ok(())
        }
        fn children(&mut self, block: &Value, list: bool, code: bool) -> Result<(), Error> {
            for child in block["elements"]
                .as_array()
                .ok_or_else(|| invalid("Malformed rich text."))?
            {
                self.inline(child, list, code)?;
            }
            Ok(())
        }
    }
    let mut elements = Vec::new();
    for block in blocks {
        if block["type"] != "rich_text" {
            return Err(invalid("This profile editor requires rich-text blocks."));
        }
        elements.extend(
            block["elements"]
                .as_array()
                .ok_or_else(|| invalid("Malformed rich text."))?,
        );
    }
    let mut ops = Ops::default();
    let mut previous = "";
    for (index, block) in elements.into_iter().enumerate() {
        let kind = block["type"].as_str().unwrap_or("");
        if index > 0
            && (matches!(
                kind,
                "rich_text_quote" | "rich_text_preformatted" | "rich_text_list"
            ) || (kind == "rich_text_section" && previous == kind))
            && !ops.trailing()
        {
            ops.newline();
        }
        match kind {
            "rich_text_section" => ops.children(block, false, false)?,
            "rich_text_quote" => {
                ops.children(block, false, false)?;
                if !ops.trailing() {
                    ops.newline();
                }
            }
            "rich_text_preformatted" => {
                ops.children(block, false, true)?;
                ops.newline();
            }
            "rich_text_list" => {
                for row in block["elements"]
                    .as_array()
                    .ok_or_else(|| invalid("Malformed rich list."))?
                {
                    ops.children(row, true, false)?;
                    ops.newline();
                }
            }
            _ => {
                return Err(invalid(
                    "This rich block's profile length cannot yet be verified.",
                ));
            }
        }
        previous = kind;
    }
    if !ops.trailing() {
        ops.newline();
    }
    Ok(ops
        .values
        .iter()
        .map(|(text, link, _)| {
            text.encode_utf16().count()
                + link
                    .as_ref()
                    .filter(|url| *url != text)
                    .map_or(0, |url| url.encode_utf16().count())
        })
        .sum::<usize>()
        .saturating_sub(1))
}
