use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{PreparedRequest, SlackClient},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    AddCanvas {
        file: String,
        label: String,
        access: Access,
        after: Option<String>,
    },
    Rename {
        id: String,
        label: String,
    },
    Move {
        id: String,
        after: Option<String>,
    },
    Remove {
        id: String,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    base: Inventory,
    action: Action,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    Satisfied,
    Unchanged,
    Changed,
}
impl Intent {
    pub fn capture(base: Inventory, action: Action) -> Result<Self, Fault> {
        match &action {
            Action::AddCanvas {
                file, label, after, ..
            } => {
                if !resource_id(file) || !valid_label(label) {
                    return Err(Fault::Shape);
                }
                if base.rows.iter().filter(|r| !system(r)).count() >= 15 {
                    return Err(Fault::Limit);
                }
                if base.rows.iter().any(|r| file_id(r) == Some(file.as_str())) {
                    return Err(Fault::Changed);
                }
                anchor(&base, after, None)?;
            }
            Action::Rename { id, label } => {
                let row = base.canvas(id)?;
                if super::label(row) == label {
                    return Err(Fault::Changed);
                }
                if !valid_label(label) {
                    return Err(Fault::Shape);
                }
            }
            Action::Move { id, after } => {
                base.canvas(id)?;
                anchor(&base, after, Some(id))?;
                let index = base
                    .rows
                    .iter()
                    .position(|r| super::id(r) == Some(id))
                    .unwrap();
                let predecessor = index.checked_sub(1).and_then(|i| super::id(&base.rows[i]));
                if predecessor == after.as_deref() {
                    return Err(Fault::Changed);
                }
            }
            Action::Remove { id } => {
                base.canvas(id)?;
            }
        }
        Ok(Self { base, action })
    }
    pub fn base(&self) -> &Inventory {
        &self.base
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
    pub fn revalidate(&self, current: &Inventory) -> Result<(), Fault> {
        if self.base.same_basis(current) {
            Ok(())
        } else {
            Err(Fault::Changed)
        }
    }
    pub fn request(&self, client: &SlackClient, session: &WorkspaceSession) -> PreparedRequest {
        let mut fields = vec![("channel_id", self.base.channel.clone())];
        let endpoint = match &self.action {
            Action::AddCanvas {
                file,
                label,
                access,
                after,
            } => {
                fields.extend([("type","canvas".into()),("label",label.clone()),("data",serde_json::json!({"file_id":file,"access_level":match access{Access::Read=>"read",Access::Write=>"write"}}).to_string())]);
                if let Some(after) = after {
                    fields.push(("after", after.clone()));
                }
                "conversations.addTab"
            }
            Action::Rename { id, label } => {
                fields.extend([("id", id.clone()), ("label", label.clone())]);
                "conversations.updateTab"
            }
            Action::Move { id, after } => {
                fields.push(("id", id.clone()));
                if let Some(after) = after {
                    fields.push(("after", after.clone()));
                }
                "conversations.moveTab"
            }
            Action::Remove { id } => {
                fields.extend([("id", id.clone()), ("keep_file_share", "true".into())]);
                "conversations.removeTab"
            }
        };
        client.rest_form(session, endpoint, fields)
    }
    /// Positive readback only. Absence after an uncertain request never proves
    /// it was unsent and never returns permission to replay that request.
    pub fn observe(&self, current: &Inventory) -> Observation {
        if self.base == *current {
            return Observation::Unchanged;
        }
        if current.channel != self.base.channel {
            return Observation::Changed;
        }
        let mut props = current.properties.clone();
        props.as_object_mut().unwrap().remove("tabz");
        let mut before_props = self.base.properties.clone();
        before_props.as_object_mut().unwrap().remove("tabz");
        if props != before_props {
            return Observation::Changed;
        }
        let mut expected = self.base.rows.clone();
        match &self.action {
            Action::Rename { id: key, label } => {
                let row = expected.iter_mut().find(|r| id(r) == Some(key)).unwrap();
                row["label"] = label.clone().into();
            }
            Action::Remove { id: key } => expected.retain(|r| id(r) != Some(key)),
            Action::Move { id: key, after } => {
                let i = expected.iter().position(|r| id(r) == Some(key)).unwrap();
                let row = expected.remove(i);
                let i = after
                    .as_ref()
                    .and_then(|a| expected.iter().position(|r| id(r) == Some(a)))
                    .map_or(0, |i| i + 1);
                expected.insert(i, row);
            }
            Action::AddCanvas {
                file, label, after, ..
            } => {
                let candidates = current
                    .rows
                    .iter()
                    .filter(|r| {
                        file_id(r) == Some(file)
                            && r["type"] == "canvas"
                            && super::label(r) == label
                            && id(r).is_some_and(tab_id)
                    })
                    .collect::<Vec<_>>();
                if candidates.len() != 1 {
                    return Observation::Changed;
                }
                let row = candidates[0];
                if self.base.rows.iter().any(|r| id(r) == id(row)) {
                    return Observation::Changed;
                }
                // Source omits after for normal append. Move's omitted after is
                // first dynamic position; add's omitted after is append.
                let i = after
                    .as_ref()
                    .and_then(|a| expected.iter().position(|r| id(r) == Some(a)))
                    .map_or(expected.len(), |i| i + 1);
                expected.insert(i, row.clone());
            }
        }
        if expected == current.rows {
            Observation::Satisfied
        } else {
            Observation::Changed
        }
    }
}
fn valid_label(s: &str) -> bool {
    s.encode_utf16().count() <= 256 && !s.contains('\0')
}
fn anchor(base: &Inventory, after: &Option<String>, target: Option<&str>) -> Result<(), Fault> {
    if let Some(after) = after {
        // The observed worker only sends after for a non-static tab identity.
        if !tab_id(after) {
            return Err(Fault::Unsupported);
        }
        if target == Some(after) || base.row(after).is_none() {
            return Err(Fault::Missing);
        }
    }
    Ok(())
}
fn system(row: &Value) -> bool {
    matches!(
        row["type"].as_str(),
        Some(
            "channel"
                | "record_overview"
                | "record_summary"
                | "channel_canvas"
                | "create_canvas"
                | "creating_canvas"
                | "workflows"
                | "files"
                | "pins"
                | "bookmarks"
                | "threads"
                | "slackbot_ai_conversation"
                | "slackbot_ai_history"
                | "slackbot_ai_skills"
                | "slackbot_ai_tasks"
                | "app_3p_agent_tasks"
                | "channel_tasks"
                | "import_emails"
                | "emails"
                | "app_home"
                | "app_about"
                | "app_chat"
                | "app_history"
                | "app_memories"
                | "app_tasks"
                | "code"
                | "code_channels"
        )
    )
}

/// Source-backed eligibility read before sharing a Canvas into a channel tab.
pub fn can_share_request(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    file: &str,
) -> Result<PreparedRequest, Fault> {
    if !channel_id(channel) || !resource_id(file) {
        return Err(Fault::Shape);
    }
    Ok(client.rest_form(
        session,
        "files.canShare",
        vec![
            ("file_id", file.to_owned()),
            ("channel_id", channel.to_owned()),
        ],
    ))
}
