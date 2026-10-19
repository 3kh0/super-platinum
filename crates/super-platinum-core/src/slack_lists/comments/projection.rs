//! Exact List flexpane predicates from f2Yt -> Blei -> iub5, with positively
//! observed c4db user/team release assignments. Raw messages remain untouched.
use super::ListError;
use crate::slack::models::Message;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Projection {
    assignments: Value,
    lists_enabled: bool,
}
impl Projection {
    pub fn observed(raw: Value) -> Result<Self, ListError> {
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(ListError(
                "Comment projection assignments were not confirmed.",
            ));
        }
        let assignments = raw["assignments"]
            .as_object()
            .ok_or(ListError("Comment projection assignments are unavailable."))?;
        let enabled = |team: bool| -> Result<bool, ListError> {
            let mut on = false;
            for tier in ["playground", "friends_of", "dogfood", "pilot"] {
                let key = format!("lists_{tier}{}", if team { "_teams" } else { "" });
                if let Some(value) = assignments.get(&key) {
                    on |= value["group"]
                        .as_str()
                        .ok_or(ListError("Comment release assignment is malformed."))?
                        == "on";
                }
            }
            Ok(on)
        };
        let user = enabled(false)?;
        let team = enabled(true)?;
        let lists_enabled = user && team;
        Ok(Self {
            assignments: raw,
            lists_enabled,
        })
    }
    pub fn raw(&self) -> &Value {
        &self.assignments
    }
    pub fn visible<'a>(&self, messages: &'a [Message], root: &str) -> Vec<&'a Message> {
        // Blei builds this set before filtering hidden replies.
        let has_replies = messages
            .iter()
            .any(|m| m.ts.as_deref().is_some_and(|ts| ts != root));
        messages
            .iter()
            .filter(|m| {
                if m.ts.as_deref() == Some(root) {
                    return !(has_replies
                        && m.subtype.as_deref() == Some("list_record_comment")
                        && truthy(m.extra.get("slack_list")));
                }
                !truthy(m.extra.get("isNonExistent"))
                    && !truthy(m.extra.get("no_display"))
                    && !truthy(m.extra.get("_hidden_stub"))
                    && !(self.lists_enabled
                        && m.subtype.as_deref() == Some("list_edit")
                        && truthy(m.extra.get("list_edit")))
            })
            .collect()
    }
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn rows() -> Vec<Message> {
        serde_json::from_value(json!([
            {"ts":"1700000000.000001","subtype":"list_record_comment","slack_list":{}},
            {"ts":"1700000001.000001","subtype":"list_edit","list_edit":{}},
            {"ts":"1700000002.000001","_hidden_stub":true},
            {"ts":"1700000003.000001","_hidden_reply":true},
            {"ts":"1700000004.000001","no_display":"yes"},
            {"ts":"1700000005.000001","isNonExistent":true}
        ]))
        .unwrap()
    }
    #[test]
    fn root_and_hidden_reply_rules_preserve_raw_rows_and_gate_edit_history() {
        let rows = rows();
        let off = Projection::observed(json!({"ok":true,"assignments":{}})).unwrap();
        assert_eq!(
            off.visible(&rows, "1700000000.000001")
                .iter()
                .map(|m| m.ts.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["1700000001.000001", "1700000003.000001"]
        );
        let on = Projection::observed(json!({"ok":true,"assignments":{"lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}})).unwrap();
        assert_eq!(on.visible(&rows, "1700000000.000001").len(), 1);
        assert_eq!(on.visible(&rows[..1], "1700000000.000001").len(), 1);
        assert!(
            on.visible(&[rows[0].clone(), rows[2].clone()], "1700000000.000001")
                .is_empty()
        );
        assert_eq!(rows.len(), 6);
    }
    #[test]
    fn missing_producer_is_not_an_off_assignment() {
        assert!(Projection::observed(json!({"ok":true})).is_err());
        assert!(
            Projection::observed(json!({"ok":true,"assignments":{"lists_pilot":null}})).is_err()
        );
        let one_side =
            Projection::observed(json!({"ok":true,"assignments":{"lists_pilot":{"group":"on"}}}))
                .unwrap();
        assert_eq!(one_side.visible(&rows(), "1700000000.000001").len(), 2);
    }
}
