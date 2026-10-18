//! Closed fixture server. Only actual prepared drafts requests enter this reducer.
use super::*;
use serde::{Deserialize, Serialize};
use super_platinum_core::slack::client::RequestBody;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Server {
    pub rows: BTreeMap<String, Value>,
    pub ledger: Vec<Value>,
    pub writes: usize,
    pub external: Vec<Value>,
}
impl Server {
    pub fn inject_conflict(&mut self) -> Result<(), String> {
        if self.rows.len() != 1 || self.writes != 1 || !self.external.is_empty() {
            return Err("Unexpected conflict injection state".into());
        }
        let row = self
            .rows
            .get_mut("DrScheduledForward")
            .ok_or("Owned draft missing")?;
        let before = row.clone();
        row["blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Other editor's retained version 日本語"}]}]}]);
        row["last_updated_ts"] = "300.000001".into();
        self.external
            .push(json!({"kind":"synthetic-other-editor","before":before,"after":row}));
        Ok(())
    }
    pub fn execute(
        &mut self,
        request: PreparedRequest,
        scenario: &str,
        phase: &str,
    ) -> Result<Value, Error> {
        if self.ledger.len() >= 64 || self.writes >= 16 {
            return Err(refused("live_bounds"));
        }
        let RequestBody::Form(form) = &request.body else {
            return Err(refused("form"));
        };
        let fields: BTreeMap<_, _> = form.iter().cloned().collect();
        let method = request
            .url
            .split('?')
            .next()
            .and_then(|s| s.strip_prefix("https://fixture.slack.com/api/"))
            .ok_or_else(|| refused("url"))?;
        if request.method != "POST"
            || fields.len() != form.len()
            || fields.get("token").map(String::as_str) != Some(TOKEN)
            || request.headers.iter().any(|(name, _)| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "cookie" | "authorization"
                )
            })
        {
            return Err(refused("identity"));
        }
        if !matches!(
            method,
            "drafts.create" | "drafts.update" | "drafts.delete" | "drafts.info" | "drafts.list"
        ) {
            return Err(refused("method"));
        }
        let mutation = matches!(method, "drafts.create" | "drafts.update" | "drafts.delete");
        if phase == "restart" && mutation {
            return Err(refused("restart_write"));
        }
        let names = form.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
        let raw = match method {
            "drafts.info" => {
                if names != ["token", "draft_id"] {
                    return Err(refused("info_fields"));
                }
                let row = self
                    .rows
                    .get(&fields["draft_id"])
                    .ok_or_else(|| refused("missing_row"))?;
                json!({"ok":true,"draft":row})
            }
            "drafts.list" => {
                if names != ["token", "is_active", "limit"]
                    && names != ["token", "is_active", "limit", "next_ts"]
                {
                    return Err(refused("list_fields"));
                }
                if fields["is_active"] != "true" || fields["limit"] != "25" {
                    return Err(refused("list_scope"));
                }
                if fields.get("next_ts").is_none() && scenario == "lost-readback" {
                    // A full nonmatching first page proves explicit continuation, not absence.
                    let rows=(0..25).map(|n|json!({"id":format!("DrOther{n}"),"client_msg_id":format!("other-{n}"),"last_updated_ts":format!("{}.000001",100-n)})).collect::<Vec<_>>();
                    json!({"ok":true,"drafts":rows,"has_more":true})
                } else {
                    if fields.get("next_ts").is_some_and(|n| n != "76.000001") {
                        return Err(refused("cursor"));
                    }
                    json!({"ok":true,"drafts":self.rows.values().filter(|r|r["is_deleted"]==false).collect::<Vec<_>>(),"has_more":false})
                }
            }
            "drafts.delete" => {
                if names
                    != [
                        "token",
                        "draft_id",
                        "client_last_updated_ts",
                        "skip_file_deletion",
                        "_x_reason",
                    ]
                    || fields["skip_file_deletion"] != "false"
                    || fields["_x_reason"] != "DeleteDraftModal"
                {
                    return Err(refused("delete_fields"));
                }
                decimal(&fields["client_last_updated_ts"])?;
                let row = self
                    .rows
                    .get_mut(&fields["draft_id"])
                    .ok_or_else(|| refused("missing_row"))?;
                row["is_deleted"] = true.into();
                row["last_updated_ts"] = format!("{}.000001", 200 + self.writes).into();
                json!({"ok":true,"draft":row})
            }
            _ => {
                let mut expected = vec![
                    "token",
                    "blocks",
                    "destinations",
                    "file_ids",
                    "attachments",
                    "date_scheduled",
                    "is_from_composer",
                    "_x_reason",
                ];
                if fields.contains_key("unfurl") {
                    expected.push("unfurl");
                }
                if method == "drafts.create" {
                    expected.push("client_msg_id");
                } else {
                    expected.extend(["draft_id", "client_last_updated_ts"]);
                }
                if names != expected
                    || fields["file_ids"] != "[]"
                    || fields["attachments"] != ""
                    || fields["is_from_composer"] != "true"
                    || fields["_x_reason"] != "schedule-draft"
                {
                    return Err(refused("write_fields"));
                }
                if method == "drafts.update" {
                    decimal(&fields["client_last_updated_ts"])?;
                }
                let blocks: Value =
                    serde_json::from_str(&fields["blocks"]).map_err(|_| refused("blocks"))?;
                let destinations: Value = serde_json::from_str(&fields["destinations"])
                    .map_err(|_| refused("destinations"))?;
                if !blocks.is_array() || destinations.as_array().is_none_or(|v| v.len() != 1) {
                    return Err(refused("payload"));
                }
                let time = fields["date_scheduled"]
                    .parse::<i64>()
                    .map_err(|_| refused("time"))?;
                let id = if method == "drafts.create" {
                    "DrScheduledForward".to_owned()
                } else {
                    fields["draft_id"].clone()
                };
                let client = if method == "drafts.create" {
                    if !self.rows.is_empty() {
                        return Err(refused("duplicate_create"));
                    }
                    fields["client_msg_id"].clone()
                } else {
                    self.rows
                        .get(&id)
                        .and_then(|r| r["client_msg_id"].as_str())
                        .ok_or_else(|| refused("missing_row"))?
                        .to_owned()
                };
                let mut row = json!({"id":id,"client_msg_id":client,"blocks":blocks,"destinations":destinations,"file_ids":[],"attachments":[],"date_scheduled":time,"last_updated_ts":format!("{}.000001",200+self.writes),"is_deleted":false,"is_sent":false});
                if let Some(unfurl) = fields.get("unfurl") {
                    row["unfurl"] = serde_json::from_str(unfurl).map_err(|_| refused("unfurl"))?;
                }
                self.rows.insert(id, row.clone());
                json!({"ok":true,"draft":row})
            }
        };
        if mutation {
            self.writes += 1;
        }
        let lost = mutation
            && ((scenario == "lost-readback" && self.writes == 1)
                || (scenario == "lost-edit" && self.writes == 2));
        self.ledger.push(json!({"method":request.method,"url":request.url,"headers":request.headers,"form":form,"retrySafe":request.retry_safe(),"phase":phase,"result":if lost {json!({"error":"synthetic-response-lost","serverApplied":true})}else{json!({"raw":raw})}}));
        if lost {
            Err(Error::Transport("Synthetic committed response lost".into()))
        } else {
            Ok(raw)
        }
    }
}
fn decimal(value: &str) -> Result<(), Error> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        Err(refused("timestamp"))
    } else {
        Ok(())
    }
}
pub(super) fn refused(why: &str) -> Error {
    Error::Api(format!("scheduled_forward_fixture_{why}"))
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
