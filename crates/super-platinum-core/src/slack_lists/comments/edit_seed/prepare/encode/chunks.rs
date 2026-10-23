use super::*;
pub(super) mod semantic;
mod slug;
pub(super) mod text;

pub(super) fn encode(
    ops: &[Value],
    strip: bool,
    preformatted: bool,
    context: &mut Context<'_>,
) -> Result<Vec<Value>> {
    let last = ops.len().checked_sub(1);
    let end = if strip && ops.last().is_some_and(|v| v["insert"] == "\n") {
        ops.len() - 1
    } else {
        ops.len()
    };
    let mut output: Vec<Value> = vec![];
    for (index, op) in ops[..end].iter().enumerate() {
        let attributes = op.get("attributes");
        let mention =
            op["insert"].is_string() && truthy(attributes.and_then(|a| a.get("slackmention")));
        let mut chunks = if truthy(op["insert"].get("slackemoji")) {
            semantic::emoji(op, preformatted, context)?
        } else if mention && semantic::real_mention(op)? {
            semantic::mention(op, preformatted, context)?
        } else if op["insert"].is_string() && truthy(attributes.and_then(|a| a.get("slackslug"))) {
            slug::encode(op, preformatted)?
        } else if op["insert"].is_string() && truthy(attributes.and_then(|a| a.get("link"))) {
            let url = attributes.unwrap().get("link").unwrap();
            let mut chunk = json!({"type":"link","url":url});
            if op["insert"] != *url {
                chunk["text"] = op["insert"].clone();
            }
            vec![if preformatted {
                chunk
            } else {
                text::annotate(chunk, op)
            }]
        } else if truthy(op["insert"].get("workflowtoken")) {
            let token = &op["insert"]["workflowtoken"];
            let token = token.as_object().ok_or(SeedUnavailable::MalformedMessage)?;
            let mut chunk = json!({"type":"workflowtoken"});
            if token.is_empty() {
                chunk["id"] = json!("");
                chunk["data_type"] = json!("");
            } else {
                if let Some(v) = token.get("id") {
                    chunk["id"] = v.clone();
                }
                if let Some(v) = token.get("type") {
                    chunk["data_type"] = v.clone();
                }
            }
            if truthy(token.get("isPlaceholder")) {
                chunk["is_placeholder"] = token["isPlaceholder"].clone();
                for (from, to) in [("label", "label"), ("sourceLabel", "source_label")] {
                    if truthy(token.get(from)) {
                        chunk[to] = token[from].clone();
                    }
                }
            }
            // The dispatch does not pass preformattedStyle to this source helper.
            vec![text::annotate(chunk, op)]
        } else if truthy(attributes.and_then(|a| a.get("slacktag"))) {
            let tag = attributes.unwrap().get("slacktag").unwrap();
            let text = if op["insert"].is_string() {
                op["insert"].clone()
            } else {
                tag.get("text")
                    .filter(|v| truthy(Some(v)))
                    .cloned()
                    .unwrap_or(json!(""))
            };
            let mut chunk = json!({"type":"tag","text":text});
            if truthy(tag.get("color")) {
                chunk["color"] = tag["color"].clone();
            }
            vec![if preformatted {
                chunk
            } else {
                self::text::annotate(chunk, op)
            }]
        } else {
            text::chunks(op, strip && Some(index) == last, preformatted, false)?
        };
        context.reserve(&chunks, 1)?;
        if let Some(first) = chunks.first() {
            if first["type"] == "text"
                && first.get("style").is_none()
                && output
                    .last()
                    .is_some_and(|v| v["type"] == "text" && v.get("style").is_none())
            {
                let previous = output.last_mut().unwrap();
                let prior = previous["text"]
                    .as_str()
                    .ok_or(SeedUnavailable::MalformedMessage)?;
                context.reserve(prior, 1)?;
                let mut joined = prior.to_owned();
                context.append(
                    &mut joined,
                    first["text"]
                        .as_str()
                        .ok_or(SeedUnavailable::MalformedMessage)?,
                )?;
                previous["text"] = json!(joined);
                chunks.remove(0);
            }
        }
        output.extend(chunks);
    }
    Ok(output)
}
