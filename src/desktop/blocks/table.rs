//! Strict message-only static-table reading, never a form/action/media parser.
//! Named passive adaptation: active rich renderers and resolved labels unclaimed.
use crate::model::table::{Align, Cell, Column, Part, ReadTable, Run, Style};
use serde_json::Value;
use std::borrow::Cow;

pub(crate) mod fixture;

const MAX_BYTES: usize = 64 * 1024;
const MAX_RUNS: usize = 8192;
const MAX_PARTS: usize = 4096;
#[derive(Default)]
struct Budget {
    bytes: usize,
    runs: usize,
    parts: usize,
    nodes: usize,
}
impl Budget {
    fn node(&mut self) -> Option<()> {
        self.nodes = self.nodes.checked_add(1)?;
        (self.nodes <= 16384).then_some(())
    }
    fn run(&mut self, text: &str, style: Style) -> Option<Run> {
        self.bytes = self.bytes.checked_add(text.len())?;
        self.runs = self.runs.checked_add(1)?;
        (self.bytes <= MAX_BYTES && self.runs <= MAX_RUNS).then(|| Run {
            text: text.into(),
            style,
        })
    }
    fn part(&mut self) -> Option<()> {
        self.parts = self.parts.checked_add(1)?;
        (self.parts <= MAX_PARTS).then_some(())
    }
}
fn style(v: &Value) -> Style {
    let flag = |key: &str| {
        v.get("style")
            .and_then(|s| s.get(key))
            .and_then(Value::as_bool)
            == Some(true)
    };
    Style {
        bold: flag("bold"),
        italic: flag("italic"),
        underline: flag("underline"),
        strike: flag("strike"),
        code: flag("code"),
    }
}
fn space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn identifier(v: &Value, key: &str, prefix: &str) -> Option<String> {
    let id = v.get(key)?.as_str()?;
    let starts = match key {
        "user_id" => "UW",
        "channel_id" => "CGD",
        "usergroup_id" => "S",
        _ => return None,
    };
    (id.len() >= 2
        && id.len() <= 64
        && id.starts_with(|c: char| starts.contains(c))
        && id
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
    .then(|| format!("{prefix}{id}"))
}
fn inline(values: &[Value], b: &mut Budget, depth: usize) -> Option<Vec<Run>> {
    if depth > 8 {
        return None;
    }
    let mut result = Vec::new();
    for value in values {
        b.node()?;
        if ["userId", "channelId", "usergroupId", "skinTone"]
            .iter()
            .any(|key| value.get(*key).is_some())
        {
            return None;
        }
        let flags = style(value);
        let text: Cow<'_, str> = match value.get("type")?.as_str()? {
            "text" => value.get("text")?.as_str()?.into(),
            "link" => {
                // Source checks the original URL before either label branch.
                let url = value.get("url")?.as_str()?.trim_matches(space);
                if url.is_empty() {
                    return None;
                }
                if let Some(elements) = value.get("elements") {
                    let nested = inline(elements.as_array()?, b, depth + 1)?;
                    result.extend(nested.into_iter().map(|mut r| {
                        r.style.bold |= flags.bold;
                        r.style.italic |= flags.italic;
                        r.style.underline |= flags.underline;
                        r.style.strike |= flags.strike;
                        r.style.code |= flags.code;
                        r
                    }));
                    continue;
                }
                match value.get("text") {
                    Some(Value::String(s)) if !s.is_empty() => s.as_str().into(),
                    None | Some(Value::Null) | Some(Value::String(_)) => url.into(),
                    _ => return None,
                }
            }
            // Literal reference fallbacks only: no human-name/member/provider claim.
            "user" => identifier(value, "user_id", "@")?.into(),
            "channel" => identifier(value, "channel_id", "#")?.into(),
            "usergroup" => identifier(value, "usergroup_id", "@")?.into(),
            "broadcast" => {
                let range = value.get("range")?.as_str()?;
                if !matches!(range, "here" | "channel" | "everyone") {
                    return None;
                }
                format!("@{range}").into()
            }
            "date" => value.get("fallback")?.as_str()?.into(),
            "emoji" => {
                if let Some(unicode) = value
                    .get("unicode")
                    .filter(|v| !v.is_null() && v.as_str() != Some(""))
                {
                    let unicode = unicode.as_str()?;
                    if unicode.len() > MAX_BYTES * 2 {
                        return None;
                    }
                    super::emoji::unicode_glyph(unicode)?.into()
                } else {
                    let name = value.get("name")?.as_str()?;
                    if name.is_empty() || name.len() > 128 {
                        return None;
                    }
                    let tone = match value.get("skin_tone") {
                        None | Some(Value::Null) => String::new(),
                        Some(Value::String(t)) if t.len() <= 32 => format!("::skin-tone-{t}"),
                        Some(Value::Number(t)) if t.is_u64() => format!("::skin-tone-{t}"),
                        _ => return None,
                    };
                    format!(":{name}{tone}:").into()
                }
            }
            // No controls, file/Canvas references, arbitrary resources or HTML.
            _ => return None,
        };
        result.push(b.run(&text, flags)?);
    }
    Some(result)
}
fn cell(value: &Value, b: &mut Budget) -> Option<Cell> {
    b.node()?;
    if value.is_null()
        || value.as_bool() == Some(false)
        || value.as_f64() == Some(0.0)
        || value.as_str() == Some("")
    {
        return Some(Cell(Vec::new()));
    }
    let kind = value.get("type")?.as_str()?;
    if matches!(kind, "raw_text" | "raw_number") {
        if kind == "raw_number" && !value.get("value")?.is_number() {
            return None;
        }
        b.part()?;
        return Some(Cell(vec![Part::Paragraph(vec![
            b.run(value.get("text")?.as_str()?, Style::default())?,
        ])]));
    }
    if kind != "rich_text" {
        return None;
    }
    let mut parts = Vec::new();
    for element in value.get("elements")?.as_array()? {
        b.part()?;
        let kind = element.get("type")?.as_str()?;
        let elements = element.get("elements")?.as_array()?;
        parts.push(match kind {
            "rich_text_section" => Part::Paragraph(inline(elements, b, 0)?),
            "rich_text_quote" => Part::Quote(inline(elements, b, 0)?),
            "rich_text_preformatted" => Part::Code(
                inline(elements, b, 0)?
                    .into_iter()
                    .map(|r| r.text)
                    .collect(),
            ),
            "rich_text_list" => {
                let mut items = Vec::new();
                for section in elements {
                    b.part()?;
                    if section.get("type")?.as_str()? != "rich_text_section" {
                        return None;
                    }
                    items.push(inline(section.get("elements")?.as_array()?, b, 0)?);
                }
                Part::List {
                    ordered: element.get("style").and_then(Value::as_str) == Some("ordered"),
                    indent: element
                        .get("indent")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .min(8) as u8,
                    offset: element
                        .get("offset")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .try_into()
                        .ok()?,
                    items,
                }
            }
            _ => return None,
        });
    }
    Some(Cell(parts))
}
/// No BlockCtx, workspace, media, Signal, callback or factory is accepted here.
pub(crate) fn parse(value: &Value) -> Option<ReadTable> {
    if value.get("type")?.as_str()? != "table" || value.get("columnSettings").is_some() {
        return None;
    }
    let raw = value.get("rows")?.as_array()?;
    if raw.is_empty() || raw.len() > 100 {
        return None;
    }
    let mut budget = Budget::default();
    let mut rows = Vec::new();
    let mut width = 0;
    for row in raw {
        let row = row.as_array()?;
        if row.len() > 20 {
            return None;
        }
        width = width.max(row.len());
        rows.push(
            row.iter()
                .map(|v| cell(v, &mut budget))
                .collect::<Option<Vec<_>>>()?,
        );
    }
    if width == 0 {
        return None;
    }
    let mut columns = Vec::new();
    match value.get("column_settings") {
        None | Some(Value::Null) => {}
        Some(Value::Array(settings)) if settings.len() <= 20 => {
            for c in settings {
                if c.is_null() {
                    columns.push(Column::default());
                    continue;
                }
                if !c.is_object() || c.get("isWrapped").is_some() {
                    return None;
                }
                let wrapped = match c.get("is_wrapped") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(v)) => *v,
                    _ => return None,
                };
                let align = match c.get("align").and_then(Value::as_str) {
                    Some("right") => Align::Right,
                    Some("center") => Align::Center,
                    _ => Align::Left,
                };
                columns.push(Column { align, wrapped });
            }
        }
        _ => return None,
    }
    Some(ReadTable::new(columns, rows))
}

#[cfg(test)]
mod tests;
