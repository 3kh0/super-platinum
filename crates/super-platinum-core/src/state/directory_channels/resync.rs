//! 3iGP/WRAU response normalization. Output is raw ingress, never canonical rows.
//! Parent owns the original request, source collection witness and dispatch order.
use super::{Error, Result};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Follow source action order; source payload presence is preserved.
pub enum Action {
    Channels(Vec<Value>),
    Unchanged(Vec<String>),
    Priority(Value),
    HasMoreMpdms(Value),
    Visitors(Value),
    TemporaryExpirations(Value),
    Sections(Value),
    Stars(Vec<Value>),
}
pub struct ChannelsData {
    actions: Vec<Action>,
}
impl ChannelsData {
    /// `canonical` is the current source collection in encounter order. A typed
    /// HashMap iteration or queued raw wire rows cannot supply that observation.
    pub fn decode(raw: &Value, canonical: &[Value], version_all_channels: bool) -> Result<Self> {
        let channels = raw
            .get("channels")
            .and_then(Value::as_object)
            .ok_or(Error::Shape)?;
        let mut rows = Vec::new();
        for key in ["channels", "ims", "mpims"] {
            rows.extend(array(channels.get(key))?.iter().cloned());
        }
        let open = string_ids(channels.get("is_open"))?;
        let unchanged = channels
            .get("unchanged_channel_ids")
            .filter(|v| truth(v))
            .map(|v| string_ids(Some(v)))
            .transpose()?;
        for row in &mut rows {
            let id = channel_id(row)?.to_owned();
            let im = truth(&row["is_im"]) || truth(&row["is_mpim"]);
            let object = row.as_object_mut().ok_or(Error::Shape)?;
            object.insert("is_member".into(), true.into());
            object.remove("latest");
            object.remove("last_read");
            if im {
                object.insert("is_open".into(), open.contains(&id).into());
            }
        }
        if version_all_channels {
            let mut retained = rows
                .iter()
                .map(channel_id)
                .collect::<Result<BTreeSet<_>>>()?;
            if let Some(ids) = &unchanged {
                retained.extend(ids.iter().map(String::as_str));
            }
            let mut closing = Vec::new();
            for row in canonical {
                let id = channel_id(row)?;
                if truth(&row["is_open"])
                    && (truth(&row["is_im"]) || truth(&row["is_mpim"]))
                    && !retained.contains(id)
                {
                    let mut closed = row.clone();
                    closed["is_open"] = false.into();
                    closing.push(closed);
                }
            }
            rows.extend(closing);
        }
        let mut actions = Vec::new();
        if !rows.is_empty() {
            actions.push(Action::Channels(rows));
        }
        if let Some(ids) = unchanged {
            actions.push(Action::Unchanged(ids));
        }
        if let Some(v) = channels.get("channels_priority").filter(|v| truth(v)) {
            actions.push(Action::Priority(v.clone()));
        }
        if let Some(v) = channels.get("has_more_mpdms") {
            actions.push(Action::HasMoreMpdms(v.clone()));
        }
        if let Some(v) = channels.get("visitor_channels") {
            actions.push(Action::Visitors(v.clone()));
        }
        if let Some(sections) = raw
            .get("sections")
            .filter(|s| !s["channel_sections"].is_null())
        {
            let source = array(sections.get("channel_sections"))?;
            let mut projected = Vec::new();
            for (at, section) in source.iter().enumerate() {
                let object = section.as_object().ok_or(Error::Shape)?;
                let ids = array(object.get("channel_ids"))?;
                let mut row = serde_json::Map::new();
                for key in [
                    "channel_section_id",
                    "name",
                    "type",
                    "emoji",
                    "is_redacted",
                    "is_hidden",
                    "usergroup_id",
                ] {
                    if let Some(v) = object.get(key) {
                        row.insert(key.into(), v.clone());
                    }
                }
                row.insert(
                    "next_channel_section_id".into(),
                    source
                        .get(at + 1)
                        .and_then(|s| s.get("channel_section_id"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                row.insert("last_updated".into(), 0.into());
                let mut page = json!({"channel_ids":ids,"count":ids.len()});
                if let Some(v) = object.get("placement_sources") {
                    page["placement_sources"] = v.clone();
                }
                row.insert("channel_ids_page".into(), page);
                projected.push(Value::Object(row));
            }
            let mut stars = Vec::new();
            if let Some(section) = projected.iter_mut().find(|s| s["type"] == "stars") {
                stars = section["channel_ids_page"]["channel_ids"]
                    .as_array()
                    .unwrap()
                    .clone();
                section["channel_ids_page"] = json!({"channel_ids":[],"count":0});
            }
            if let Some(section) = projected
                .iter_mut()
                .find(|s| s["type"] == "temporary_channels")
            {
                section["channel_ids_page"] = json!({"channel_ids":[],"count":0});
                actions.push(Action::TemporaryExpirations(
                    channels
                        .get("temporary_channel_expirations")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                ));
            }
            let count = projected.len();
            actions.push(Action::Sections(json!({"channel_sections":projected,"last_updated":0,"count":count,"ok":true,
                "entities":sections.get("entities").filter(|v|!v.is_null()).cloned().unwrap_or_else(||json!([]))})));
            // qobR/5ZBK unique retains first encounter for string channel IDs.
            let mut seen = BTreeSet::new();
            stars.retain(|id| seen.insert(id.as_str().map(str::to_owned)));
            if stars.iter().any(|id| id.as_str().is_none()) {
                return Err(Error::Shape);
            }
            actions.push(Action::Stars(stars));
        }
        Ok(Self { actions })
    }
    pub fn into_actions(self) -> Vec<Action> {
        self.actions
    }
}
fn array(value: Option<&Value>) -> Result<&[Value]> {
    match value {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(Error::Shape),
    }
}
fn string_ids(value: Option<&Value>) -> Result<Vec<String>> {
    array(value)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or(Error::Shape))
        .collect()
}
fn channel_id(row: &Value) -> Result<&str> {
    row.get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(Error::Shape)
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
