use super::*;

pub(super) fn blocks(
    original: &MessageWitness,
    blocks: &[Value],
    observations: &PlainObservations,
) -> Result<Delta, SeedUnavailable> {
    let mut out = Delta::default();
    let mut accepted = 0;
    for (bi, block) in blocks.iter().enumerate().filter(|(_, b)| editable(b)) {
        if accepted > 0 && !out.ends_newline() {
            out.text("\n");
        }
        accepted += 1;
        match required(block, "type")? {
            "rich_text" => {
                for (ei, element) in elements(block)?.iter().enumerate() {
                    let kind = required(element, "type")?;
                    let path = [bi, ei];
                    match kind {
                        "rich_text_section" => out.append(chunks::convert(
                            original,
                            element,
                            &path,
                            kind,
                            observations,
                        )?),
                        "rich_text_quote" => {
                            out.text("> ");
                            if number(element, "border", 0)? >= 1 {
                                out.text("> ");
                            }
                            out.append(chunks::convert(
                                original,
                                element,
                                &path,
                                kind,
                                observations,
                            )?);
                            out.text("\n");
                        }
                        "rich_text_preformatted" => {
                            if number(element, "border", 0)? >= 1 {
                                out.text("> ");
                            }
                            out.text("```");
                            out.append(chunks::convert(
                                original,
                                element,
                                &path,
                                kind,
                                observations,
                            )?);
                            out.text("```\n");
                        }
                        "rich_text_header" => {
                            out.append(chunks::convert(
                                original,
                                element,
                                &path,
                                kind,
                                observations,
                            )?);
                            out.text("\n");
                        }
                        "rich_text_divider" => {
                            if !out.ends_newline() {
                                out.text("\n");
                            }
                            out.text("--\n");
                        }
                        "rich_text_list" => {
                            let indent = number(element, "indent", 0)?;
                            let border = number(element, "border", 0)?;
                            let offset = number(element, "offset", 0)?;
                            if !(0..=32).contains(&indent)
                                || !(0..=32).contains(&border)
                                || offset.unsigned_abs() > 100_000
                            {
                                return Err(SeedUnavailable::Limit);
                            }
                            let spaces = "    ".repeat(indent as usize);
                            for (si, section) in elements(element)?.iter().enumerate() {
                                if border > 0 {
                                    for _ in 0..border {
                                        out.text("> ");
                                        out.text(&spaces);
                                    }
                                } else {
                                    out.text(&spaces);
                                }
                                match element.get("style").and_then(Value::as_str) {
                                    Some("bullet") => out.text("* "),
                                    Some("ordered") => {
                                        out.text(&format!("{}. ", si as i64 + 1 + offset))
                                    }
                                    _ => {}
                                }
                                out.append(chunks::convert(
                                    original,
                                    section,
                                    &[bi, ei, si],
                                    "rich_text_section",
                                    observations,
                                )?);
                                out.text("\n");
                            }
                        }
                        // Xc/Q has no default top-level conversion branch.
                        _ => {}
                    }
                }
            }
            "header" | "section" => {
                let text = match block.get("text").and_then(|t| t.get("text")) {
                    None => "",
                    Some(Value::String(text)) => text,
                    _ => return Err(SeedUnavailable::MalformedMessage),
                };
                out.append(text_delta(text, observations)?);
            }
            "divider" => out.text("--\n"),
            _ => unreachable!(),
        }
    }
    if accepted > 0 && !out.ends_newline() {
        out.text("\n");
    }
    Ok(out)
}

pub(super) fn elements(value: &Value) -> Result<&[Value], SeedUnavailable> {
    value
        .get("elements")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(SeedUnavailable::MalformedMessage)
}
fn number(value: &Value, key: &str, default: i64) -> Result<i64, SeedUnavailable> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_i64().ok_or(SeedUnavailable::MalformedMessage),
    }
}
fn editable(block: &Value) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("rich_text" | "header" | "divider") => true,
        Some("section") => {
            truthy(block.get("text"))
                && !truthy(block.get("accessory"))
                && !truthy(block.get("fields"))
        }
        _ => false,
    }
}
pub(super) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Object(_) | Value::Array(_)) => true,
    }
}
