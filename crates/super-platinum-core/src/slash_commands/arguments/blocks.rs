//! Port of 8J14 line grouping, list offsets, quote/preformat and header assembly.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq)]
enum Kind {
    Section,
    List,
    Quote,
    Pre,
    Header,
}
struct Line {
    ops: Vec<Op>,
    attrs: Map<String, Value>,
}
fn kind(attrs: &Map<String, Value>) -> Kind {
    if attrs.contains_key("code-block") {
        Kind::Pre
    } else if attrs.contains_key("blockquote") {
        Kind::Quote
    } else if attrs.contains_key("list") {
        Kind::List
    } else if attrs.contains_key("header") {
        Kind::Header
    } else {
        Kind::Section
    }
}
fn number(attrs: &Map<String, Value>, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}
fn lines(ops: Vec<Op>) -> Result<Vec<Line>, Unsupported> {
    let mut result = Vec::new();
    let mut pending = Vec::new();
    for op in ops {
        if let Some(text) = op.insert.as_str() {
            let mut offset = 0;
            for (index, _) in text.match_indices('\n') {
                if index > offset {
                    push(
                        &mut pending,
                        Op {
                            insert: text[offset..index].into(),
                            attrs: op.attrs.clone(),
                        },
                    );
                }
                result.push(Line {
                    ops: std::mem::take(&mut pending),
                    attrs: op.attrs.clone(),
                });
                if result.len() > 4096 {
                    return Err(Unsupported::Shape);
                }
                offset = index + 1;
            }
            if offset < text.len() {
                push(
                    &mut pending,
                    Op {
                        insert: text[offset..].into(),
                        attrs: op.attrs,
                    },
                );
            }
        } else {
            pending.push(op);
        }
    }
    if !pending.is_empty() {
        result.push(Line {
            ops: pending,
            attrs: Map::new(),
        });
    }
    Ok(result)
}
fn combined(lines: &[Line]) -> Vec<Op> {
    let mut result = Vec::new();
    for line in lines {
        for op in &line.ops {
            push(&mut result, op.clone());
        }
        push(
            &mut result,
            Op {
                insert: "\n".into(),
                attrs: line.attrs.clone(),
            },
        );
    }
    result
}
fn chunks(
    ops: &[Op],
    emoji: &Value,
    strip: bool,
    pre: bool,
    expand: bool,
) -> Result<Vec<Value>, Unsupported> {
    let standalone = strip
        && ops
            .last()
            .is_some_and(|op| op.insert == Value::String("\n".into()));
    let end = ops.len() - usize::from(standalone);
    let mut result: Vec<Value> = Vec::new();
    for (index, op) in ops[..end].iter().enumerate() {
        for value in chunks::context(
            op,
            emoji,
            strip && !standalone && index + 1 == end,
            pre,
            expand,
        )? {
            if value["type"] == "text" && value.get("style").is_none() {
                if let Some(prior) = result
                    .last_mut()
                    .filter(|v| v["type"] == "text" && v.get("style").is_none())
                {
                    prior["text"] = format!(
                        "{}{}",
                        prior["text"].as_str().unwrap(),
                        value["text"].as_str().unwrap()
                    )
                    .into();
                    continue;
                }
            }
            result.push(value);
            if result.len() > 4096 {
                return Err(Unsupported::Shape);
            }
        }
    }
    Ok(result)
}
fn list(lines: &[Line], emoji: &Value) -> Result<Vec<Value>, Unsupported> {
    let mut result = Vec::new();
    let mut offsets = BTreeMap::<u64, usize>::new();
    let mut start = 0;
    while start < lines.len() {
        let style = lines[start].attrs["list"]
            .as_str()
            .ok_or(Unsupported::Content)?;
        let indent = number(&lines[start].attrs, "indent");
        let border = number(&lines[start].attrs, "border");
        let offset = *offsets.entry(indent).or_default();
        let mut end = start + 1;
        while end < lines.len()
            && lines[end].attrs["list"] == style
            && number(&lines[end].attrs, "indent") == indent
        {
            end += 1;
        }
        let mut items = Vec::new();
        for line in &lines[start..end] {
            items.push(json!({"type":"rich_text_section","elements":chunks(&line.ops,emoji,false,false,true)?}));
        }
        let mut item = json!({"type":"rich_text_list","style":style,"indent":indent,"border":border,"elements":items});
        if style == "ordered" && offset > 0 {
            item["offset"] = offset.into();
        }
        result.push(item);
        if end < lines.len() {
            let next_indent = number(&lines[end].attrs, "indent");
            if indent < next_indent && lines[end].attrs["list"] == "ordered" {
                offsets.insert(next_indent, 0);
            }
            if style == "ordered" {
                offsets.insert(indent, offset + end - start);
            }
        }
        start = end;
    }
    Ok(result)
}
fn group(
    lines: &[Line],
    kind: &Kind,
    emoji: &Value,
    final_group: bool,
) -> Result<Vec<Value>, Unsupported> {
    match kind {
        Kind::List => list(lines, emoji),
        Kind::Header => lines.iter().map(|line| Ok(json!({"type":"rich_text_header","level":number(&line.attrs,"header"),"elements":chunks(&line.ops,emoji,false,false,false)?}))).collect(),
        Kind::Quote if lines.iter().any(|line| number(&line.attrs,"border") >= 1) => {
            lines.iter().map(|line| {
                let mut value = json!({"type":"rich_text_quote","elements":chunks(&line.ops,emoji,true,false,true)?});
                let border=number(&line.attrs,"border"); if border > 0 { value["border"] = border.into(); }
                Ok(value)
            }).collect()
        }
        _ => {
            let ops = combined(lines);
            let (name, strip, pre) = match kind {
                Kind::Section => ("rich_text_section",final_group,false),
                Kind::Quote => ("rich_text_quote",true,false),
                Kind::Pre => ("rich_text_preformatted",true,true),
                _ => unreachable!(),
            };
            let mut value=json!({"type":name,"elements":chunks(&ops,emoji,strip,pre,true)?});
            if *kind == Kind::Pre { value["border"] = u64::from(lines.iter().all(|line| number(&line.attrs,"border") >= 1)).into(); }
            Ok(vec![value])
        }
    }
}
pub(super) fn render(mut ops: Vec<Op>, emoji: &Value) -> Result<Option<Value>, Unsupported> {
    if ops.is_empty() {
        return Ok(None);
    }
    if !ops
        .last()
        .and_then(|op| op.insert.as_str())
        .is_some_and(|s| s.ends_with('\n'))
    {
        push(
            &mut ops,
            Op {
                insert: "\n".into(),
                attrs: Map::new(),
            },
        );
    }
    let rows = lines(ops)?;
    let mut elements = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let current = kind(&rows[start].attrs);
        let mut end = start + 1;
        while end < rows.len() && kind(&rows[end].attrs) == current {
            end += 1;
        }
        elements.extend(group(
            &rows[start..end],
            &current,
            emoji,
            end == rows.len(),
        )?);
        start = end;
    }
    let value = json!([{"type":"rich_text","elements":elements}]);
    if value.to_string().len() > 128 * 1024 {
        return Err(Unsupported::Shape);
    }
    Ok(Some(value))
}
