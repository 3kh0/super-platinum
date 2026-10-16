//! Loaded saved-view Find. Port of retained B +E6Z and S getTypedFieldValue.
//! Deliberately distinct from display text and saved-filter atoms. Never mutates
//! records or reads transport. Malformed/oversize searchable cells fail visibly.
use super::{Column, Document, Record, RecordsPage};
use icu_normalizer::DecomposingNormalizerBorrowed;
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

const LIMIT: usize = 65_536;
type Result<T> = std::result::Result<T, &'static str>;
const INVALID: &str = "Find cannot safely read a malformed or oversized loaded field.";

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub text: String,
    /// Some(empty) still suppresses raw identity matching, as in Slack.
    pub user_names: Option<BTreeMap<String, String>>,
    pub channel_names: Option<BTreeMap<String, String>>,
}
/// Keys requested by B mMPz/k+RH. Users retain the whole decoded key;
/// channels split it. Rich text contributes the first user and primary-column
/// channel references. Missing directory entries still map to an empty name.
pub fn name_keys(
    document: &Document,
    page: &RecordsPage,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut users = BTreeSet::new();
    let mut channels = BTreeSet::new();
    let primary_text = document
        .columns
        .iter()
        .find(|c| c.kind == "text" && c.raw["is_primary_column"] == true)
        .map(|c| c.id.as_str());
    if document.columns.len() > 1024 {
        return Err(INVALID);
    }
    for record in &page.records {
        // VBDB builds name maps after base admission, before saved filters.
        if record.archived()
            || record.raw["is_deleted"] == true
            || record.raw["suggested"] == true
            || record.parent_id().is_some()
        {
            continue;
        }
        for column in &document.columns {
            if !matches!(
                column.kind.as_str(),
                "user" | "todo_assignee" | "text" | "channel"
            ) {
                continue;
            }
            let field = record.fields.get(&column.key);
            if let Some(field) = field {
                validate(field, 0, &mut 2048, &mut 65_536)?;
            }
            let value = decode(field)?;
            let Some(value) = value.as_deref().filter(|v| truthy(v)) else {
                continue;
            };
            match column.kind.as_str() {
                "user" | "todo_assignee" => {
                    users.insert(match value {
                        Value::String(s) => s.clone(),
                        _ => return Err(INVALID),
                    });
                }
                "channel" => {
                    for id in value.as_str().ok_or(INVALID)?.split('|') {
                        channels.insert(id.into());
                    }
                }
                "text" => {
                    let mut first = None;
                    if let Some(nodes) = value.as_array() {
                        for node in nodes {
                            if let Some(children) = node["elements"].as_array() {
                                rich_keys(
                                    children,
                                    &mut first,
                                    &mut channels,
                                    primary_text == Some(column.id.as_str()),
                                );
                            }
                        }
                    }
                    // The source adds an empty key if no rich mention exists.
                    users.insert(first.unwrap_or_default());
                }
                _ => {}
            }
        }
    }
    Ok((users, channels))
}
fn rich_keys(
    nodes: &[Value],
    first_user: &mut Option<String>,
    channels: &mut BTreeSet<String>,
    primary: bool,
) {
    for node in nodes {
        if first_user.is_none() && node["type"] == "user" {
            if let Some(id) = node["user_id"].as_str().filter(|id| !id.is_empty()) {
                *first_user = Some(id.into());
            }
        }
        if primary && node["type"] == "channel" {
            if let Some(id) = node["channel_id"].as_str().filter(|id| !id.is_empty()) {
                channels.insert(id.into());
            }
        }
        if let Some(children) = node["elements"].as_array() {
            rich_keys(children, first_user, channels, primary);
        }
    }
}
pub struct Search<'a> {
    columns: Vec<(&'a Column, BTreeMap<String, String>)>,
    query: String,
    bypass: bool,
    users: Option<BTreeMap<String, String>>,
    channels: Option<BTreeMap<String, String>>,
}
pub fn normalize(text: &str) -> String {
    DecomposingNormalizerBorrowed::new_nfd()
        .normalize(text)
        .chars()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect::<String>()
        .to_lowercase()
}
fn normalized(text: &str) -> Result<String> {
    if text.len() > LIMIT {
        return Err(INVALID);
    }
    Ok(normalize(text))
}
fn names(map: &Option<BTreeMap<String, String>>) -> Result<Option<BTreeMap<String, String>>> {
    map.as_ref()
        .map(|map| {
            map.iter()
                .map(|(id, name)| Ok((id.clone(), normalized(name)?)))
                .collect()
        })
        .transpose()
}
impl<'a> Search<'a> {
    pub fn new(document: &'a Document, query: &Query) -> Result<Self> {
        if query.text.is_empty() {
            return Ok(Self {
                columns: Vec::new(),
                query: String::new(),
                bypass: true,
                users: None,
                channels: None,
            });
        }
        let mut columns = Vec::new();
        if !query.text.is_empty() {
            if document.columns.len() > 1024 {
                return Err(INVALID);
            }
            for column in &document.columns {
                if !matches!(
                    column.kind.as_str(),
                    "text"
                        | "email"
                        | "phone"
                        | "number"
                        | "rating"
                        | "select"
                        | "user"
                        | "todo_assignee"
                        | "channel"
                        | "vote"
                ) {
                    continue;
                }
                let mut choices = BTreeMap::new();
                if column.kind == "select" {
                    if let Some(raw) = column
                        .raw
                        .pointer("/options/choices")
                        .filter(|v| !v.is_null())
                    {
                        let values = raw.as_array().ok_or(INVALID)?;
                        if values.len() > 2048 {
                            return Err(INVALID);
                        }
                        for choice in values {
                            let id = choice["value"].as_str().ok_or(INVALID)?;
                            choices.insert(
                                id.to_owned(),
                                normalized(choice["label"].as_str().ok_or(INVALID)?)?,
                            );
                        }
                    }
                }
                columns.push((column, choices));
            }
        }
        Ok(Self {
            columns,
            query: normalized(&query.text)?,
            bypass: query.text.is_empty(),
            users: names(&query.user_names)?,
            channels: names(&query.channel_names)?,
        })
    }
    pub fn matches(&self, record: &Record) -> Result<bool> {
        if self.bypass {
            return Ok(true);
        }
        let mut found = false;
        for (column, choices) in &self.columns {
            let field = record.fields.get(&column.key);
            if let Some(field) = field {
                if field
                    .get("column_id")
                    .is_some_and(|id| id.as_str() != Some(&column.id))
                    || field
                        .get("row_id")
                        .is_some_and(|id| id.as_str() != Some(&record.id))
                {
                    return Err(INVALID);
                }
                validate(field, 0, &mut 2048, &mut 65_536)?;
            }
            let decoded = decode(field)?;
            // S turns field.text into the decoded record's fallback map. The
            // original wire record is not itself that decoded projection.
            let value = match field.and_then(|f| f.get("text")).filter(|v| truthy(v)) {
                Some(value) => Cow::Borrowed(value.as_str().ok_or(INVALID)?),
                None => string_value(&decoded)?,
            };
            let hit = match column.kind.as_str() {
                "vote" => {
                    let count = decoded
                        .as_deref()
                        .and_then(Value::as_str)
                        .map(|s| s.split('|').count())
                        .unwrap_or(0);
                    count.to_string().contains(&self.query)
                }
                "user" | "todo_assignee" if self.users.is_some() => self
                    .users
                    .as_ref()
                    .unwrap()
                    .get(value.as_ref())
                    .is_some_and(|name| name.contains(&self.query)),
                "channel" if self.channels.is_some() => self
                    .channels
                    .as_ref()
                    .unwrap()
                    .get(value.as_ref())
                    .is_some_and(|name| name.contains(&self.query)),
                "select" => {
                    let raw = normalized(&value)?;
                    let multi = column
                        .raw
                        .pointer("/options/format")
                        .and_then(Value::as_str)
                        == Some("multi_select");
                    let labels = value
                        .split('|')
                        .filter(|s| !s.is_empty())
                        .take(if multi { usize::MAX } else { 1 })
                        .any(|id| choices.get(id).unwrap_or(&raw).contains(&self.query));
                    labels || raw.contains(&self.query)
                }
                _ => normalized(&value)?.contains(&self.query),
            };
            found |= hit;
        }
        Ok(found)
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn validate(value: &Value, depth: usize, nodes: &mut usize, bytes: &mut usize) -> Result<()> {
    if depth > 32 || *nodes == 0 {
        return Err(INVALID);
    }
    *nodes -= 1;
    match value {
        Value::String(text) => {
            *bytes = bytes.checked_sub(text.len()).ok_or(INVALID)?;
        }
        Value::Array(values) => {
            for value in values {
                validate(value, depth + 1, nodes, bytes)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate(value, depth + 1, nodes, bytes)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn array<'a>(field: &'a Value, key: &str) -> Result<Option<&'a [Value]>> {
    match field.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(v)) => Ok((!v.is_empty()).then_some(v.as_slice())),
        _ => Err(INVALID),
    }
}
fn join(values: &[Value], separator: &str) -> Result<Value> {
    let mut out = Vec::new();
    for value in values {
        // Native bounds reject malformed object/array entity IDs rather than
        // inventing identifiers using JavaScript object coercion.
        out.push(match value {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            Value::Bool(v) => v.to_string(),
            Value::Number(n) => number(n)?,
            _ => return Err(INVALID),
        });
    }
    Ok(Value::String(out.join(separator)))
}
fn number(number: &serde_json::Number) -> Result<String> {
    let n = number.as_f64().filter(|n| n.is_finite()).ok_or(INVALID)?;
    if n == 0.0 {
        return Ok("0".into());
    }
    if n.abs() >= 1e21 || n.abs() < 1e-6 {
        let scientific = format!("{n:e}");
        let (mantissa, exponent) = scientific.split_once('e').ok_or(INVALID)?;
        let exponent: i32 = exponent.parse().map_err(|_| INVALID)?;
        return Ok(format!("{mantissa}e{exponent:+}"));
    }
    Ok(n.to_string())
}
/// S decoder precedence is independent of the schema's nominal kind.
fn decode(field: Option<&Value>) -> Result<Option<Cow<'_, Value>>> {
    let Some(field) = field else { return Ok(None) };
    for (key, separator) in [("attachment", ","), ("channel", "|")] {
        if let Some(values) = array(field, key)? {
            return Ok(Some(Cow::Owned(join(values, separator)?)));
        }
    }
    if let Some(value) = field.get("checkbox").filter(|v| v.is_boolean()) {
        return Ok(Some(Cow::Borrowed(value)));
    }
    if let Some(values) = array(field, "date")? {
        return Ok(Some(Cow::Owned(join(values, ":")?)));
    }
    if let Some(values) = array(field, "email")? {
        return Ok(Some(Cow::Borrowed(&values[0])));
    }
    if let Some(values) = array(field, "message")? {
        if let Some(value) = values[0].get("value").filter(|v| truthy(v)) {
            return Ok(Some(Cow::Borrowed(value)));
        }
    }
    for key in ["link", "number", "phone", "rating"] {
        if let Some(values) = array(field, key)? {
            return Ok(Some(Cow::Borrowed(&values[0])));
        }
    }
    for key in ["rich_text", "richText"] {
        if let Some(value) = field.get(key).filter(|v| truthy(v)) {
            if !value.is_array() {
                return Err(INVALID);
            }
            return Ok(Some(Cow::Borrowed(value)));
        }
    }
    for key in ["select", "user"] {
        if let Some(values) = array(field, key)? {
            return Ok(Some(Cow::Owned(join(values, "|")?)));
        }
    }
    for key in ["timestamp", "reference"] {
        if let Some(values) = array(field, key)? {
            return Ok(Some(Cow::Borrowed(&values[0])));
        }
    }
    Ok(None)
}
fn first_text(nodes: &[Value]) -> &str {
    for node in nodes {
        if node["type"] == "text" {
            if let Some(text) = node["text"].as_str().filter(|s| !s.is_empty()) {
                return text;
            }
        }
        if let Some(children) = node["elements"].as_array() {
            let text = first_text(children);
            if !text.is_empty() {
                return text;
            }
        }
    }
    ""
}
fn string_value<'a>(value: &'a Option<Cow<'_, Value>>) -> Result<Cow<'a, str>> {
    let Some(value) = value.as_deref().filter(|v| truthy(v)) else {
        return Ok(Cow::Borrowed(""));
    };
    Ok(match value {
        Value::String(text) => Cow::Borrowed(text),
        Value::Bool(v) => Cow::Owned(v.to_string()),
        Value::Number(n) => Cow::Owned(number(n)?),
        Value::Array(nodes) => {
            // Captured helper deliberately starts inside each top-level block.
            let text = nodes
                .iter()
                .filter_map(|n| n["elements"].as_array())
                .map(|children| first_text(children))
                .find(|text| !text.is_empty())
                .unwrap_or("");
            Cow::Borrowed(text)
        }
        Value::Object(_) => Cow::Borrowed("[object Object]"),
        Value::Null => Cow::Borrowed(""),
    })
}
#[cfg(test)]
mod tests;
