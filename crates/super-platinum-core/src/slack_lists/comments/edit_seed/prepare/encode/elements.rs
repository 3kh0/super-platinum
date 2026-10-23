use super::*;
const SECTION: &str = "rich_text_section";
const LIST: &str = "rich_text_list";
const CODE: &str = "rich_text_preformatted";
const QUOTE: &str = "rich_text_quote";
const HEADER: &str = "rich_text_header";
const DIVIDER: &str = "rich_text_divider";
fn list(attributes: &Map<String, Value>) -> bool {
    attributes
        .get("list")
        .and_then(Value::as_str)
        .is_some_and(|s| ["bullet", "ordered", "checked", "unchecked"].contains(&s))
}
fn kind(op: &Value, line: &Map<String, Value>) -> &'static str {
    if truthy(op["insert"].get("slackhorizontalrule")) {
        DIVIDER
    } else if truthy(line.get("code-block")) {
        CODE
    } else if truthy(line.get("blockquote")) {
        QUOTE
    } else if list(line) {
        LIST
    } else if truthy(line.get("header")) {
        HEADER
    } else {
        SECTION
    }
}
pub(super) fn encode(document: &Document, context: &mut Context<'_>) -> Result<Vec<Value>> {
    let mut output = vec![];
    let mut group = vec![];
    let mut current = SECTION;
    for (_, mut line, attributes) in document.lines() {
        line.0.push(super::super::document::Unit::new(
            Insert::Unit(10),
            attributes.clone(),
        ));
        for op in line.ops()? {
            let next = kind(&op, &attributes);
            if next != current {
                output.extend(segment(&group, current, false, context)?);
                group.clear();
                current = next;
            }
            group.push(op);
        }
    }
    output.extend(segment(&group, current, true, context)?);
    Ok(output)
}
fn segment(ops: &[Value], kind: &str, last: bool, context: &mut Context<'_>) -> Result<Vec<Value>> {
    if ops.is_empty() {
        return Ok(vec![]);
    }
    let mut document = Document::capture(ops)?;
    document.normalize_insert_attributes();
    let ops = document.ops()?;
    match kind {
        LIST => lists(&document, context),
        CODE => {
            let mut border = false;
            for op in &ops {
                if truthy(op["attributes"].get("code-block"))
                    && number(op["attributes"].get("border"))? >= 1.0
                {
                    border = true;
                }
            }
            Ok(vec![
                json!({"type":CODE,"border":u8::from(border),"elements":chunks::encode(&ops,true,true,context)?}),
            ])
        }
        QUOTE => {
            let mut bordered = false;
            for op in &ops {
                if truthy(op["attributes"].get("blockquote"))
                    && number(op["attributes"].get("border"))? >= 1.0
                {
                    bordered = true;
                }
            }
            if bordered {
                let mut output = vec![];
                for (_, line, attributes) in document.lines() {
                    let mut value = json!({"type":QUOTE,"elements":chunks::encode(&line.ops()?,true,false,context)?});
                    if truthy(attributes.get("border")) {
                        value["border"] = attributes["border"].clone();
                    }
                    output.push(value);
                }
                Ok(output)
            } else {
                Ok(vec![
                    json!({"type":QUOTE,"elements":chunks::encode(&ops,true,false,context)?}),
                ])
            }
        }
        HEADER => headers(&document, context),
        DIVIDER => Ok(ops
            .iter()
            .filter(|op| truthy(op["insert"].get("slackhorizontalrule")))
            .map(|_| json!({"type":DIVIDER}))
            .collect()),
        _ => Ok(vec![
            json!({"type":kind,"elements":chunks::encode(&ops,last,false,context)?}),
        ]),
    }
}
// Explicit numeric/bool source-coercion domain. Other JSON values are refused,
// rather than silently changing the retained >=1 / Math.max comparisons.
fn number(value: Option<&Value>) -> Result<f64> {
    match value {
        None | Some(Value::Null) => Ok(0.0),
        Some(Value::Bool(v)) => Ok(if *v { 1.0 } else { 0.0 }),
        Some(Value::Number(v)) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or(SeedUnavailable::MalformedMessage),
        _ => Err(SeedUnavailable::MalformedMessage),
    }
}
fn level(value: Option<&Value>) -> Result<Value> {
    let value = if truthy(value) { number(value)? } else { 2.0 };
    let value = value.clamp(1.0, 3.0);
    Ok(if value.fract() == 0.0 {
        json!(value as u8)
    } else {
        json!(value)
    })
}
fn headers(document: &Document, context: &mut Context<'_>) -> Result<Vec<Value>> {
    let mut output = vec![];
    let mut previous: Option<Value> = None;
    let mut content = vec![];
    for (index, (_, line, attributes)) in document.lines().into_iter().enumerate() {
        let header = attributes.get("header").cloned();
        if previous.is_none() {
            previous = header.clone();
        }
        if previous != header || index > 0 && truthy(header.as_ref()) {
            output
                .push(json!({"type":HEADER,"level":level(previous.as_ref())?,"elements":content}));
            previous = header;
        }
        content = chunks::encode(&line.ops()?, false, false, context)?;
    }
    output.push(json!({"type":HEADER,"level":level(previous.as_ref())?,"elements":content}));
    Ok(output)
}
fn lists(document: &Document, context: &mut Context<'_>) -> Result<Vec<Value>> {
    let mut output = vec![];
    let mut style: Option<Value> = None;
    let mut indent: Option<Value> = None;
    let mut border: Option<Value> = None;
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries: Vec<Value> = vec![];
    let zero = |value: Option<&Value>| {
        value
            .filter(|v| truthy(Some(v)))
            .cloned()
            .unwrap_or(json!(0))
    };
    for (_, line, attributes) in document.lines() {
        let next_style = attributes.get("list").cloned();
        let next_indent = zero(attributes.get("indent"));
        let next_border = zero(attributes.get("border"));
        if !next_indent.is_number() {
            return Err(SeedUnavailable::MalformedMessage);
        }
        number(Some(&next_indent))?;
        number(Some(&next_border))?;
        if style.is_none() {
            style = next_style.clone();
        }
        if indent.is_none() {
            indent = Some(next_indent.clone());
        }
        if border.is_none() {
            border = Some(next_border.clone());
        }
        let current_indent = indent.as_ref().unwrap();
        let key = number(Some(current_indent))?.to_string();
        counters.entry(key.clone()).or_default();
        let changed =
            style != next_style || number(Some(current_indent))? != number(Some(&next_indent))?;
        if changed {
            output.push(list_block(
                style.as_ref(),
                current_indent,
                border.as_ref().unwrap(),
                &entries,
                *counters.get(&key).unwrap_or(&0),
            ));
            if !list(&attributes) {
                counters.clear();
            } else {
                if number(Some(current_indent))? < number(Some(&next_indent))?
                    && next_style.as_ref() == Some(&json!("ordered"))
                {
                    counters.insert(number(Some(&next_indent))?.to_string(), 0);
                }
                if style.as_ref() == Some(&json!("ordered")) {
                    *counters.entry(key).or_default() += entries.len();
                }
            }
            style = next_style;
            indent = Some(next_indent);
            border = Some(next_border);
            entries.clear();
        }
        entries.push(
            json!({"type":SECTION,"elements":chunks::encode(&line.ops()?,false,false,context)?}),
        );
    }
    let indent = indent.unwrap_or(json!(0));
    let border = border.unwrap_or(json!(0));
    let count = *counters
        .get(&number(Some(&indent))?.to_string())
        .unwrap_or(&0);
    output.push(list_block(
        style.as_ref(),
        &indent,
        &border,
        &entries,
        count,
    ));
    Ok(output)
}
fn list_block(
    style: Option<&Value>,
    indent: &Value,
    border: &Value,
    entries: &[Value],
    offset: usize,
) -> Value {
    let mut value = json!({"type":LIST,"style":style.filter(|v|truthy(Some(v))).cloned().unwrap_or(json!("bullet")),"indent":indent,"border":border,"elements":entries});
    if style == Some(&json!("ordered")) && offset > 0 {
        value["offset"] = json!(offset);
    }
    value
}
pub(super) fn header_block(
    ops: &[Value],
    header: Option<&Value>,
    context: &mut Context<'_>,
) -> Result<Option<Value>> {
    let chunks = chunks::encode(&Document::capture(ops)?.ops()?, false, false, context)?;
    let mut text = String::new();
    let mut all_text = true;
    for chunk in &chunks {
        if chunk["type"] == "text" {
            context.append(
                &mut text,
                chunk["text"]
                    .as_str()
                    .ok_or(SeedUnavailable::MalformedMessage)?,
            )?;
        } else if chunk["type"] == "emoji" && truthy(chunk.get("name")) {
            let name = chunk["name"]
                .as_str()
                .ok_or(SeedUnavailable::MalformedMessage)?;
            let unicode = chunk.get("unicode").and_then(Value::as_str).unwrap_or("");
            let valid = !unicode.is_empty()
                && unicode.split('-').all(|s| {
                    (4..=5).contains(&s.len())
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                });
            let resolved = if valid {
                context.unicode_name(unicode)?
            } else {
                None
            };
            if let Some(resolved) = resolved.filter(|s| !s.is_empty()) {
                let parsed = chunks::semantic::parse_emoji(&resolved)?;
                if let Some((name, Some(tone))) =
                    parsed.filter(|(_, t)| t.as_ref().is_some_and(|v| v.contains('-')))
                {
                    for piece in [
                        ":",
                        name.as_str(),
                        "::skin-tone-",
                        tone.split('-').next().unwrap(),
                        ":",
                    ] {
                        context.append(&mut text, piece)?;
                    }
                } else {
                    for piece in [":", resolved.as_str(), ":"] {
                        context.append(&mut text, piece)?;
                    }
                }
            } else {
                for piece in [":", name, ":"] {
                    context.append(&mut text, piece)?;
                }
                if truthy(chunk.get("skin_tone")) {
                    context.append(&mut text, &format!(":skin-tone-{}:", chunk["skin_tone"]))?;
                }
            }
        } else {
            all_text = false;
        }
    }
    if all_text && text.is_empty() {
        return Ok(None);
    }
    if all_text && text.encode_utf16().count() <= 150 {
        return Ok(Some(
            json!({"type":"header","text":{"type":"plain_text","text":text,"emoji":true},"level":level(header)?}),
        ));
    }
    let elements: Vec<_> = chunks
        .into_iter()
        .map(|mut chunk| {
            if chunk["type"] == "text" {
                let mut style = chunk
                    .get("style")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                style.insert("bold".into(), json!(true));
                chunk["style"] = json!(style);
            }
            chunk
        })
        .collect();
    Ok(Some(
        json!({"type":"rich_text","elements":[{"type":SECTION,"elements":elements}]}),
    ))
}
