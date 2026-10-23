use super::*;
use input::{elements, truthy};

pub(super) fn convert(
    original: &MessageWitness,
    parent: &Value,
    path: &[usize],
    kind: &str,
    observations: &PlainObservations,
) -> Result<Delta, SeedUnavailable> {
    // Xc/Q reads both selectors for every chunk group, including empty groups.
    let as_text = observations
        .seed
        .emoji_as_text
        .ok_or(SeedUnavailable::UnknownEmojiPreference)?;
    observations
        .highlight_words
        .as_ref()
        .ok_or(SeedUnavailable::MissingHighlights)?;
    let mut out = Delta::default();
    let mut styles: Vec<String> = vec![];
    for (ci, chunk) in elements(parent)?.iter().enumerate() {
        let mut chunk_path = path.to_vec();
        chunk_path.push(ci);
        let ty = required(chunk, "type")?;
        let unknown = !KNOWN_TYPES.contains(&ty);
        let unknown_url = unknown && chunk.get("url").is_some_and(Value::is_string);
        if (ty == "text" || unknown && chunk.get("text").is_some_and(Value::is_string))
            && !unknown_url
        {
            let mut text = required(chunk, "text")?.to_owned();
            if kind == "rich_text_quote" {
                text = text.replace('\n', "\n> ");
            }
            // Source strips only leading space/newline/quote markers while no
            // style is open. It does not trim the literal message contents.
            let characters: Vec<_> = text.chars().collect();
            let mut prefix_units = 0;
            let mut emitted = String::new();
            for (index, character) in characters.iter().enumerate() {
                let quote = kind == "rich_text_quote"
                    && (index > 0 && *character == '>' && characters[index - 1] == '\n'
                        || characters.first() == Some(&'>') && !truthy(chunk.get("style")));
                if styles.is_empty() && (*character == '\n' || *character == ' ' || quote) {
                    emitted.push(*character);
                    // Retained source emits one code point but calls substring(1):
                    // preserve that UTF-16 behavior even when it duplicates a tail.
                    prefix_units += 1;
                } else {
                    break;
                }
            }
            let units: Vec<_> = text.encode_utf16().collect();
            let remainder = String::from_utf16(&units[prefix_units..])
                .map_err(|_| SeedUnavailable::InvalidUnicode)?;
            out.text(&emitted);
            styled(
                &mut out,
                &mut styles,
                original,
                &chunk_path,
                chunk,
                json!(remainder),
                Value::Null,
            )?;
        } else if matches!(ty, "user" | "usergroup" | "channel" | "broadcast") {
            let (id, label, highlights) =
                match ty {
                    "user" => {
                        let id = required(chunk, "user_id")?;
                        (
                            id.to_owned(),
                            observations
                                .seed
                                .users
                                .get(id)
                                .ok_or_else(|| SeedUnavailable::MissingUser(id.to_owned()))?,
                            true,
                        )
                    }
                    "usergroup" => {
                        let id = required(chunk, "usergroup_id")?;
                        (
                            id.to_owned(),
                            observations
                                .seed
                                .usergroups
                                .get(id)
                                .ok_or_else(|| SeedUnavailable::MissingUsergroup(id.to_owned()))?,
                            true,
                        )
                    }
                    "channel" => {
                        let id = required(chunk, "channel_id")?;
                        let mut found = observations
                            .channel_labels
                            .iter()
                            .filter(|(raw, _)| raw == chunk);
                        let label = &found
                            .next()
                            .ok_or_else(|| SeedUnavailable::MissingChannelToken(chunk.clone()))?
                            .1;
                        if found.next().is_some() {
                            return Err(SeedUnavailable::InvalidObservation);
                        }
                        (id.to_owned(), label, false)
                    }
                    _ => {
                        let range = required(chunk, "range")?;
                        (
                            format!("BK{range}"),
                            observations.seed.broadcasts.get(range).ok_or_else(|| {
                                SeedUnavailable::MissingBroadcast(range.to_owned())
                            })?,
                            true,
                        )
                    }
                };
            let mut attributes = mention(&id, label, observations, highlights)?;
            if ty == "channel" && truthy(chunk.get("tab_id")) {
                attributes["slackmention"]["tabId"] = chunk["tab_id"].clone();
            }
            styled(
                &mut out,
                &mut styles,
                original,
                &chunk_path,
                chunk,
                json!(label),
                attributes,
            )?;
        } else if ty == "emoji" {
            // This branch deliberately neither opens nor closes literal styles.
            if as_text {
                let mut observed = observations
                    .emoji_text
                    .iter()
                    .filter(|(raw, _)| raw == chunk);
                let text = &observed
                    .next()
                    .ok_or_else(|| SeedUnavailable::MissingEmojiText(chunk.clone()))?
                    .1;
                if observed.next().is_some() {
                    return Err(SeedUnavailable::InvalidObservation);
                }
                out.text(text);
            } else {
                let mut text = format!(":{}:", required(chunk, "name")?);
                if truthy(chunk.get("skin_tone")) {
                    let tone = chunk["skin_tone"]
                        .as_u64()
                        .filter(|v| (2..=6).contains(v))
                        .ok_or(SeedUnavailable::MalformedMessage)?;
                    text.push_str(&format!(":skin-tone-{tone}:"));
                }
                out.insert(json!({"slackemoji":{"text":text}}), Value::Null);
            }
        } else if matches!(ty, "color" | "tag" | "work_object_mention" | "date") {
            let key = match ty {
                "color" => "value",
                "date" => "fallback",
                _ => "text",
            };
            if ty == "date" && !truthy(chunk.get(key)) {
                continue;
            }
            let text = match chunk.get(key) {
                None | Some(Value::Null) if ty != "color" => "",
                Some(Value::String(v)) => v,
                _ => return Err(SeedUnavailable::MalformedMessage),
            };
            styled(
                &mut out,
                &mut styles,
                original,
                &chunk_path,
                chunk,
                json!(text),
                Value::Null,
            )?;
        } else if ty == "link" || unknown_url {
            let url = required(chunk, "url")?;
            let text = match chunk.get("text") {
                None | Some(Value::Null) => "",
                Some(Value::String(s)) => s,
                _ => return Err(SeedUnavailable::MalformedMessage),
            };
            let value = if !text.is_empty()
                && (truthy(chunk.get("style").and_then(|s| s.get("code")))
                    || kind == "rich_text_preformatted")
            {
                text.to_owned()
            } else if !text.is_empty() {
                format!("[{text}]({url})")
            } else {
                url.to_owned()
            };
            styled(
                &mut out,
                &mut styles,
                original,
                &chunk_path,
                chunk,
                json!(value),
                Value::Null,
            )?;
        } else if ty == "workflowtoken" {
            let insert = json!({"workflowtoken":{"id":required(chunk,"id")?,"type":required(chunk,"data_type")?,"label":"","sourceLabel":"","propertyLabel":"","isAvailable":false}});
            styled(
                &mut out,
                &mut styles,
                original,
                &chunk_path,
                chunk,
                insert,
                Value::Null,
            )?;
        } else if slug_shape(chunk, ty) {
            let mut observed = observations.slugs.iter().filter(|(raw, _)| raw == chunk);
            let slug = &observed
                .next()
                .ok_or_else(|| SeedUnavailable::MissingSlug(chunk.clone()))?
                .1;
            if observed.next().is_some() {
                return Err(SeedUnavailable::InvalidObservation);
            }
            if let Some(slug) = slug {
                let label = required(slug, "label")?;
                styled(
                    &mut out,
                    &mut styles,
                    original,
                    &chunk_path,
                    chunk,
                    json!(label),
                    json!({"slackslug":slug}),
                )?;
            }
        }
        // Source has no conversion branch for the remaining known elements or
        // unknown non-text/non-URL elements; raw original remains in the witness.
    }
    for style in styles.iter().rev() {
        out.text(marker(style).unwrap());
    }
    Ok(out)
}

fn styled(
    out: &mut Delta,
    stack: &mut Vec<String>,
    original: &MessageWitness,
    path: &[usize],
    chunk: &Value,
    insert: Value,
    attributes: Value,
) -> Result<(), SeedUnavailable> {
    let keys = match chunk.get("style") {
        None | Some(Value::Null) => vec![],
        Some(Value::Object(_)) => original
            .style_keys(path)
            .ok_or(SeedUnavailable::InvalidObservation)?,
        _ => return Err(SeedUnavailable::MalformedMessage),
    };
    for old in stack.clone().iter().rev() {
        if !keys.contains(&old.as_str()) {
            out.text(marker(old).unwrap());
            stack.retain(|key| key != old);
        }
    }
    for key in keys {
        if let Some(mark) = marker(key) {
            if !stack.iter().any(|old| old == key) {
                out.text(mark);
                stack.push(key.to_owned());
            }
        }
    }
    out.insert(insert, attributes);
    Ok(())
}
fn marker(style: &str) -> Option<&'static str> {
    match style {
        "bold" => Some("*"),
        "italic" => Some("_"),
        "strike" => Some("~"),
        "code" => Some("`"),
        _ => None,
    }
}
fn slug_shape(chunk: &Value, ty: &str) -> bool {
    let key = match ty {
        "canvas" | "file" | "list_record" => "file_id",
        "skill" => "skill_ref",
        "message_mention" => "message_ts",
        "workflow_mention" => "workflow_id",
        _ => return false,
    };
    chunk.get(key).is_some()
}
// Literal values of retained N8wa.Lv, used by BeP8/tlJF's unknown-type tests.
const KNOWN_TYPES: &[&str] = &[
    "rich_text",
    "expanded_rich_text",
    "rich_text_section",
    "rich_text_header",
    "rich_text_divider",
    "rich_text_quote",
    "rich_text_preformatted",
    "rich_text_list",
    "text",
    "user",
    "usergroup",
    "channel",
    "broadcast",
    "emoji",
    "link",
    "date",
    "color",
    "workflowtoken",
    "truncation",
    "unknown",
    "file",
    "canvas",
    "message_mention",
    "workflow_mention",
    "canvas_message_unfurl",
    "list_record",
    "skill",
    "citation",
    "tag",
    "salesforce_data_field",
    "canvas_user_mention",
    "attachment_mention",
    "work_object_mention",
    "_edited_label",
    "_stopped_label",
    "_emoji_group",
    "_quote_group",
    "_theme_install_button",
    "_truncation_expand_button",
    "_truncation_collapse_button",
    "_file_mention",
    "_canvas_mention",
    "_unfurl_mention",
    "_canvas_user_mention",
    "_variable",
    "slackhorizontalrule",
];
