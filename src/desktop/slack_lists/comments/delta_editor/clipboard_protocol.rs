//! Typed native-envelope boundary. Host ownership/store checks remain mandatory
//! before and after observations; decoding itself grants no acquisition or edit.
use super::EmojiAppearance;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_SAFE: u64 = 9_007_199_254_740_991;
const MAX_ENVELOPE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    pub index: u64,
    pub length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Paste {
    pub request: String,
    pub scope: String,
    pub generation: String,
    pub revision: u64,
    pub selection_revision: u64,
    pub selection: Selection,
    pub ops: Vec<Value>,
    pub mention_ids: Vec<String>,
    pub emoji_names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    kind: String,
    scope: String,
    mount: String,
    sequence: u64,
    generation: u64,
    pub clipboard: Paste,
}

impl Request {
    pub fn decode(value: Value) -> Option<Self> {
        if serde_json::to_vec(&value).ok()?.len() > MAX_ENVELOPE_BYTES {
            return None;
        }
        let request: Self = serde_json::from_value(value).ok()?;
        let paste = &request.clipboard;
        let id = uuid::Uuid::parse_str(&paste.request).ok()?;
        if request.kind != "clipboard-request"
            || request.scope.is_empty()
            || request.mount.is_empty()
            || request.scope != paste.scope
            || request.generation > MAX_SAFE
            || request.generation.to_string() != paste.generation
            || request.sequence == 0
            || request.sequence > MAX_SAFE
            || paste.revision > MAX_SAFE
            || paste.selection_revision > MAX_SAFE
            || paste.selection.index > MAX_SAFE
            || paste.selection.length > MAX_SAFE
            || paste.selection.index.checked_add(paste.selection.length)? > 262144
            || id.get_version_num() != 4
            || id.to_string() != paste.request
            || paste.ops.len() > 32768
        {
            return None;
        }
        let mut units = 0usize;
        let mut mentions = Vec::new();
        let mut emoji = Vec::new();
        let mut seen_mentions = HashSet::new();
        let mut seen_emoji = HashSet::new();
        let mut budget = 1_000_000usize;
        for op in &paste.ops {
            if !bounded(op, 0, &mut budget) {
                return None;
            }
            let object = op.as_object()?;
            if object
                .keys()
                .any(|key| key != "insert" && key != "attributes")
            {
                return None;
            }
            let insert = object.get("insert")?;
            units = units.checked_add(if let Some(text) = insert.as_str() {
                text.encode_utf16().count()
            } else {
                let embed = insert.as_object()?;
                if embed.len() != 1 {
                    return None;
                }
                if let Some(value) = embed.get("slackemoji") {
                    push_unique(&mut emoji, &mut seen_emoji, value.get("text")?.as_str()?);
                } else {
                    embed.get("workflowtoken")?.get("id")?.as_str()?;
                }
                1
            })?;
            if units > 262144 {
                return None;
            }
            if let Some(attributes) = object.get("attributes") {
                let attributes = attributes.as_object()?;
                if let Some(id) = attributes
                    .get("slackmention")
                    .and_then(|mention| mention.get("id"))
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                {
                    push_unique(&mut mentions, &mut seen_mentions, id);
                }
            }
        }
        (mentions == paste.mention_ids && emoji == paste.emoji_names).then_some(request)
    }

    /// Compare to independently retained original native capture, never to
    /// fields taken from this request itself. The host also checks Origin,
    /// edit intent and current store/native-map generations around every await.
    pub fn matches(
        &self,
        scope: &str,
        mount: &str,
        generation: u64,
        sequence: u64,
        revision: u64,
    ) -> bool {
        self.scope == scope
            && self.mount == mount
            && self.generation == generation
            && self.sequence == sequence
            && self.clipboard.revision == revision
    }

    pub fn request_id(&self) -> &str {
        &self.clipboard.request
    }

    /// A refusal is not a negative semantic observation or a successful paste.
    /// It can retire only the exact original request in the renderer broker.
    pub fn rejection(&self) -> Value {
        json!({"kind":"clipboard-rejected","scope":self.scope,"mount":self.mount,
            "sequence":self.sequence,"request":self.clipboard.request})
    }

    /// Call only after fresh `matches` and the host's owner/store checks.
    /// Missing validity or appearance is unavailable, never a negative row.
    pub fn receipt(
        &self,
        validity: Vec<(String, bool)>,
        emoji: Vec<EmojiAppearance>,
    ) -> Option<Value> {
        if validity
            .iter()
            .map(|row| &row.0)
            .ne(self.clipboard.mention_ids.iter())
            || emoji
                .iter()
                .map(|row| &row.name)
                .ne(self.clipboard.emoji_names.iter())
        {
            return None;
        }
        let mut receipt = serde_json::to_value(&self.clipboard).ok()?;
        receipt["validity"] = serde_json::to_value(validity).ok()?;
        receipt["emoji"] = serde_json::to_value(emoji).ok()?;
        Some(json!({"kind":"clipboard-receipt","scope":self.scope,
            "mount":self.mount,"sequence":self.sequence,"receipt":receipt}))
    }
}

fn push_unique(values: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
    if seen.insert(value.to_owned()) {
        values.push(value.into());
    }
}

fn bounded(value: &Value, depth: usize, budget: &mut usize) -> bool {
    if depth > 48 || *budget == 0 {
        return false;
    }
    *budget -= 1;
    match value {
        Value::Array(values) => values.iter().all(|value| bounded(value, depth + 1, budget)),
        Value::Object(values) => values.iter().all(|(key, value)| {
            !matches!(key.as_str(), "__proto__" | "constructor" | "prototype")
                && bounded(value, depth + 1, budget)
        }),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> Value {
        json!({"kind":"clipboard-request","scope":"owner","mount":"mount1",
            "generation":4,"sequence":8,"clipboard":{
                "request":"ed0f63eb-63d5-4b3c-92c6-8985471fcae8","scope":"owner","generation":"4",
                "revision":7,"selectionRevision":3,"selection":{"index":1,"length":2},
                "ops":[{"insert":"@Ada","attributes":{"slackmention":{"id":"U1","label":"@Ada"}}}],
                "mentionIds":["U1"],"emojiNames":[]}})
    }

    #[test]
    fn clipboard_capture_requires_exact_owner_mount_and_retained_capture() {
        let request = Request::decode(envelope()).unwrap();
        assert!(request.matches("owner", "mount1", 4, 8, 7));
        assert!(!request.matches("other", "mount1", 4, 8, 7));
        assert!(!request.matches("owner", "mount2", 4, 8, 7));
        assert!(!request.matches("owner", "mount1", 5, 8, 7));
        assert!(!request.matches("owner", "mount1", 4, 9, 7));
        assert!(!request.matches("owner", "mount1", 4, 8, 8));
    }

    #[test]
    fn clipboard_requested_ids_are_derived_from_exact_pasted_operations() {
        for ids in [json!([]), json!(["U2"]), json!(["U1", "U1"])] {
            let mut value = envelope();
            value["clipboard"]["mentionIds"] = ids;
            assert!(Request::decode(value).is_none());
        }
        let mut value = envelope();
        value["clipboard"]["generation"] = json!("04");
        assert!(Request::decode(value).is_none());
        let mut value = envelope();
        value["clipboard"]["ops"][0]["attributes"]["constructor"] = json!({});
        assert!(Request::decode(value).is_none());
    }

    #[test]
    fn clipboard_unavailable_observations_never_become_negative_validity() {
        let request = Request::decode(envelope()).unwrap();
        assert!(request.receipt(vec![], vec![]).is_none());
        assert!(
            request
                .receipt(vec![("U2".into(), false)], vec![])
                .is_none()
        );
        let receipt = request.receipt(vec![("U1".into(), false)], vec![]).unwrap();
        assert_eq!(receipt["receipt"]["ops"], envelope()["clipboard"]["ops"]);
        assert_eq!(receipt["receipt"]["validity"], json!([["U1", false]]));
        assert_eq!(receipt["sequence"], 8);
        assert_eq!(receipt["mount"], "mount1");
        assert_eq!(request.request_id(), "ed0f63eb-63d5-4b3c-92c6-8985471fcae8");
        assert_eq!(
            request.rejection(),
            json!({"kind":"clipboard-rejected","scope":"owner",
            "mount":"mount1","sequence":8,"request":"ed0f63eb-63d5-4b3c-92c6-8985471fcae8"})
        );
    }

    #[test]
    fn clipboard_envelope_bounds_include_opaque_payload_and_selection_sum() {
        let mut value = envelope();
        value["clipboard"]["selection"] = json!({"index":262144,"length":1});
        assert!(Request::decode(value).is_none());
        let mut value = envelope();
        value["clipboard"]["ops"][0]["attributes"]["slackmention"]["opaque"] =
            json!("x".repeat(MAX_ENVELOPE_BYTES));
        assert!(Request::decode(value).is_none());
    }

    #[test]
    fn clipboard_distinct_and_interleaved_ids_keep_first_occurrence_order() {
        let mut value = envelope();
        let mut ops = Vec::new();
        let mut ids = Vec::new();
        let mut emoji = Vec::new();
        for index in (0..8192).rev() {
            let id = format!("U{index}");
            let name = format!(":emoji{index}:");
            let mention = json!({"insert":"x","attributes":{"slackmention":{"id":id,"label":"x"}}});
            let embed = json!({"insert":{"slackemoji":{"text":name}}});
            ops.extend([mention.clone(), embed.clone(), mention, embed]);
            ids.push(id);
            emoji.push(name);
        }
        value["clipboard"]["ops"] = json!(ops);
        value["clipboard"]["mentionIds"] = json!(ids);
        value["clipboard"]["emojiNames"] = json!(emoji);
        let request = Request::decode(value.clone()).unwrap();
        assert_eq!(request.clipboard.mention_ids, ids);
        assert_eq!(request.clipboard.emoji_names, emoji);
        value["clipboard"]["mentionIds"][0] = json!("U0");
        assert!(Request::decode(value).is_none());
    }
}
