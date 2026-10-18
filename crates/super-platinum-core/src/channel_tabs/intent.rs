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
    AddFolder {
        label: String,
    },
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
    created_folder: Option<Value>,
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
            Action::AddFolder { label } => {
                if label.is_empty() || label.trim() != label || !valid_label(label) {
                    return Err(Fault::Shape);
                }
            }
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
                let row = editable(&base, id)?;
                if row["type"] == "folder" && (label.is_empty() || label.trim() != label) {
                    return Err(Fault::Shape);
                }
                if super::label(row) == label {
                    return Err(Fault::Changed);
                }
                if !valid_label(label) {
                    return Err(Fault::Shape);
                }
            }
            Action::Move { id, after } => {
                editable(&base, id)?;
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
        Ok(Self {
            base,
            action,
            created_folder: None,
        })
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
            Action::AddFolder { label } => {
                // Same existing request builder as the retained folder creator;
                // creation does not dispatch conversations.addTab.
                return crate::slack::api::bookmark_folder_save(
                    client,
                    session,
                    &self.base.channel,
                    None,
                    label,
                )
                .expect("validated folder title");
            }
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
    /// Retain actual creation response identity before checking UI ownership.
    /// A same-name row can never substitute for this returned bookmark ID.
    pub fn acknowledge(&mut self, raw: &Value) -> Result<(), Fault> {
        let Action::AddFolder { label } = &self.action else {
            return Ok(());
        };
        if raw["ok"] != true || raw.get("error").is_some_and(|v| !v.is_null()) {
            return Err(Fault::Shape);
        }
        let bookmark = raw.get("bookmark").ok_or(Fault::Shape)?;
        let id = bookmark["id"]
            .as_str()
            .filter(|s| {
                !s.is_empty() && s.len() <= 128 && s.bytes().all(|c| c.is_ascii_alphanumeric())
            })
            .ok_or(Fault::Shape)?;
        if bookmark["type"] != "local_folder"
            || bookmark["title"].as_str() != Some(label)
            || bookmark
                .get("channel_id")
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(self.base.channel()))
            || serde_json::to_vec(bookmark)
                .map_err(|_| Fault::Shape)?
                .len()
                > 64 * 1024
            || self.base.rows().iter().any(|r| {
                r.pointer("/data/folder_bookmark_id")
                    .and_then(Value::as_str)
                    == Some(id)
            })
        {
            return Err(Fault::Shape);
        }
        if self
            .created_folder
            .as_ref()
            .is_some_and(|old| old != bookmark)
        {
            return Err(Fault::Changed);
        }
        self.created_folder = Some(bookmark.clone());
        Ok(())
    }
    pub fn created_folder(&self) -> Option<&str> {
        self.created_folder.as_ref()?.get("id")?.as_str()
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
            Action::AddFolder { label } => {
                let Some(bookmark) = self.created_folder() else {
                    return Observation::Changed;
                };
                let added = current
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| {
                        row["type"] == "folder"
                            && row
                                .pointer("/data/folder_bookmark_id")
                                .and_then(Value::as_str)
                                == Some(bookmark)
                            && super::label(row) == label
                            && id(row).is_some_and(tab_id)
                    })
                    .collect::<Vec<_>>();
                if added.len() != 1 {
                    return Observation::Changed;
                }
                let (index, row) = added[0];
                if self.base.rows.iter().any(|old| id(old) == id(row)) {
                    return Observation::Changed;
                }
                // Preserve every existing sibling and its relative position. The
                // response owns the new tab's actual position; none was requested.
                if index > expected.len() {
                    return Observation::Changed;
                }
                expected.insert(index, row.clone());
            }
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
fn editable<'a>(base: &'a Inventory, id: &str) -> Result<&'a Value, Fault> {
    if base.row(id).is_some_and(|r| r["type"] == "folder") {
        if !tab_id(id) {
            return Err(Fault::Unsupported);
        }
        super::folder::Folder::capture(base, id)?;
        base.row(id).ok_or(Fault::Missing)
    } else {
        base.canvas(id)
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
