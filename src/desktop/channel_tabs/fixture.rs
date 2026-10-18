//! Closed, disconnected fixture backend. Never installed credentials or Transport.
use super::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{Error, PreparedRequest, SlackClient, client::RequestBody, models::File},
};
const CHANNEL: &str = "C00000001";
#[derive(Clone)]
pub(super) struct Capability(Arc<Mutex<Record>>);
struct Record {
    key: Key,
    epoch: u64,
    raw: Value,
    requests: Vec<PreparedRequest>,
    uncertain: bool,
    manager_response: Value,
    revoke_on_share: bool,
    manager_rate_limited: bool,
    enterprise: Option<String>,
    permission_response: Value,
    knowledge_loss: u8,
    knowledge_file_only: bool,
    folder_bookmarks: Value,
    #[cfg(test)]
    folder_file_error: Option<Option<u64>>,
    #[cfg(test)]
    folder_file_pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    #[cfg(test)]
    manager_pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}
fn enabled() -> bool {
    cfg!(test)
        || (matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-tabs-canvas"
                | "channel-tabs-canvas-narrow"
                | "channel-canvas-surface"
                | "channel-canvas-surface-narrow"
                | "channel-tabs-manager"
                | "channel-tabs-manager-narrow"
                | "channel-tabs-enterprise"
                | "channel-tabs-enterprise-narrow"
                | "channel-tabs-knowledge"
                | "channel-tabs-knowledge-narrow"
                | "channel-tabs-folder"
                | "channel-tabs-folder-narrow"
                | "channel-tabs-folder-manage"
                | "channel-tabs-folder-manage-narrow")
        ) && std::env::var("SUPER_PLATINUM_CHANNEL_TABS_CHECK").as_deref() == Ok("1"))
}
pub(crate) fn install(shell: &mut ShellState) {
    let Some(team) = shell.core.active_team.clone() else {
        return;
    };
    let Some(w) = shell.core.workspaces.get_mut(&team) else {
        return;
    };
    let raw = json!({"id":CHANNEL,"name":"channel-tabs","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"properties":{"who_can_manage_tabs":"everyone","tabz":[{"id":"Ct00000001","type":"canvas","label":"Project notes","data":{"file_id":"F00000001","unknown":"retained"}},{"id":"Ct00000002","type":"future_resource","label":"Future resource","opaque":{"ordered":[3,1,2]}}]}});
    w.channels
        .insert(CHANNEL.into(), serde_json::from_value(raw.clone()).unwrap());
    shell.core.active_channel = Some(CHANNEL.into());
    shell.core.active_account = Some("channel-tabs-fixture".into());
    shell.core.session = None;
    shell.core.transport = None;
    shell.refresh_from_core();
    let Some(key) = key(shell) else {
        return;
    };
    shell
        .core
        .channel_files
        .ensure_scope(shell.core.account_epoch, &team);
    shell.core.channel_files.query = super_platinum_core::files::FilesQuery {
        channel: Some(CHANNEL.into()),
        kind: super_platinum_core::files::FileKind::Canvases,
        ..Default::default()
    };
    shell.core.channel_files.items = vec![file("F00000001"), file("F00000003")];
    let cap = Capability(Arc::new(Mutex::new(Record {
        key: key.clone(),
        epoch: shell.core.account_epoch,
        raw: raw.clone(),
        requests: Vec::new(),
        uncertain: false,
        manager_response: json!({"ok":true,"role_assignments":[{"role_id":"Rl0A","users":[key.user]}]}),
        revoke_on_share: false,
        manager_rate_limited: false,
        enterprise: None,
        permission_response: json!({"ok":true,"allowed":["MANAGE_CHANNEL_TABS"]}),
        knowledge_loss: 0,
        knowledge_file_only: false,
        folder_bookmarks: json!([]),
        #[cfg(test)]
        folder_file_error: None,
        #[cfg(test)]
        folder_file_pause: None,
        #[cfg(test)]
        manager_pause: None,
    })));
    shell.channel_tabs.entries.insert(
        key,
        Entry {
            inventory: Some(Inventory::from_channel(&raw).unwrap()),
            permission: Some(Permission::Allowed),
            ..Default::default()
        },
    );
    if enabled() {
        shell.channel_tabs.fixture = Some(cap);
    }
}
pub(crate) fn install_manager(shell: &mut ShellState) {
    install(shell);
    let Some(k) = key(shell) else {
        return;
    };
    let Some(w) = shell.core.workspaces.get_mut(&k.team) else {
        return;
    };
    let Some(channel) = w.channels.get_mut(&k.channel) else {
        return;
    };
    let mut raw = serde_json::to_value(&*channel).unwrap();
    raw["properties"]["who_can_manage_tabs"] = "managers".into();
    *channel = serde_json::from_value(raw.clone()).unwrap();
    if let Some(cap) = &shell.channel_tabs.fixture {
        cap.0.lock().unwrap().raw = raw.clone();
    }
    if let Some(entry) = shell.channel_tabs.entries.get_mut(&k) {
        entry.inventory = Inventory::from_channel(&raw).ok();
        entry.permission = Some(Permission::Unavailable);
    }
}
pub(crate) fn install_enterprise(shell: &mut ShellState) {
    install_manager(shell);
    let Some(k) = key(shell) else {
        return;
    };
    let Some(w) = shell.core.workspaces.get_mut(&k.team) else {
        return;
    };
    let Some(channel) = w.channels.get_mut(&k.channel) else {
        return;
    };
    let mut raw = serde_json::to_value(&*channel).unwrap();
    raw["properties"]
        .as_object_mut()
        .unwrap()
        .remove("who_can_manage_tabs");
    *channel = serde_json::from_value(raw.clone()).unwrap();
    if let Some(cap) = &shell.channel_tabs.fixture {
        let mut r = cap.0.lock().unwrap();
        r.raw = raw.clone();
        r.enterprise = Some("E00000001".into());
    }
    if let Some(entry) = shell.channel_tabs.entries.get_mut(&k) {
        entry.inventory = Inventory::from_channel(&raw).ok();
        entry.permission = Some(Permission::Unavailable);
    }
}
pub(crate) fn install_knowledge(shell: &mut ShellState) {
    install(shell);
    let Some(k) = key(shell) else {
        return;
    };
    let Some(w) = shell.core.workspaces.get_mut(&k.team) else {
        return;
    };
    let Some(channel) = w.channels.get_mut(&k.channel) else {
        return;
    };
    let mut raw = serde_json::to_value(&*channel).unwrap();
    raw["properties"]["channel_knowledge_canvas"] = json!({"file_id":"F00000001","is_enabled":true,"date_last_generated":123,"opaque":{"keep":[3,1]}});
    raw["properties"]["tabz"][0]["label"] = "Channel Knowledge".into();
    *channel = serde_json::from_value(raw.clone()).unwrap();
    if let Some(cap) = &shell.channel_tabs.fixture {
        cap.0.lock().unwrap().raw = raw.clone();
    }
    if let Some(entry) = shell.channel_tabs.entries.get_mut(&k) {
        entry.inventory = Inventory::from_channel(&raw).ok();
    }
}

pub(crate) fn install_folder(shell: &mut ShellState) {
    install(shell);
    let key = key(shell).unwrap();
    let raw = json!({"id":CHANNEL,"name":"channel-folders","is_channel":true,"is_member":true,"is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"properties":{"who_can_manage_tabs":"everyone","tabz":[
        {"id":"Ct00000001","type":"folder","label":"Resources","data":{"folder_bookmark_id":"Bk1","opaque":"retain"}},
        {"id":"Ct00000002","type":"folder","label":"Bookmarks"},
        {"id":"Ct00000003","type":"future_resource","opaque":[3,1,2]}
    ]}});
    let bookmarks = json!([
        {"id":"Bk1","type":"local_folder","title":"Resources"},
        {"id":"Bk2","type":"link","parent_id":"Bk1","title":"Launch guide","link":"https://example.com/guide","rank":"a"},
        {"id":"Bk3","type":"file","parent_id":"Bk1","title":"Project notes","entity_id":"F00000001","rank":"b"},
        {"id":"Bk4","type":"future_resource","parent_id":"Bk1","title":"Retained future item","rank":"c","opaque":[3,1]},
        {"id":"Bk5","type":"link","title":"Top-level bookmark","link":"https://example.com/top"}
    ]);
    shell
        .core
        .workspaces
        .get_mut(&key.team)
        .unwrap()
        .channels
        .insert(
            key.channel.clone(),
            serde_json::from_value(raw.clone()).unwrap(),
        );
    let base = Inventory::from_channel(&raw).unwrap();
    let folder =
        super_platinum_core::channel_tabs::folder::Folder::capture(&base, "Ct00000001").unwrap();
    let contents = super_platinum_core::channel_tabs::folder::Contents::parse(
        &base,
        &folder,
        &json!({"ok":true,"bookmarks":bookmarks}),
    )
    .unwrap();
    let e = shell.channel_tabs.entries.get_mut(&key).unwrap();
    e.inventory = Some(base);
    e.folder.contents = Some(contents);
    if let Some(cap) = shell.channel_tabs.fixture.as_ref() {
        let mut r = cap.0.lock().unwrap();
        r.raw = raw;
        r.folder_bookmarks = bookmarks;
    }
    selection::record(shell, selection::Route::Folder(folder), None);
    shell.channel_tab = crate::state::ChannelTab::Bookmarks;
}

fn file(id: &str) -> File {
    serde_json::from_value(json!({"id":id,"filetype":"quip","title":if id=="F00000001"{"Project notes"}else{"Launch checklist"},"mode":"quip","is_tombstoned":false})).unwrap()
}
pub(super) fn capture(shell: &ShellState) -> Option<Owner> {
    let cap = shell.channel_tabs.fixture.as_ref()?.clone();
    if !cap.current(shell) {
        return None;
    }
    let key = key(shell)?;
    if key != cap.0.lock().ok()?.key || shell.self_account.user_id != key.user {
        return None;
    }
    let w = shell.core.workspaces.get(&key.team)?;
    let enterprise = cap.0.lock().ok()?.enterprise.clone();
    Some(Owner {
        key: key.clone(),
        epoch: shell.core.account_epoch,
        generation: shell.channel_generation,
        transport: None,
        fixture: Some(cap),
        client: SlackClient::default(),
        session: WorkspaceSession {
            team_id: key.team.clone(),
            enterprise_id: enterprise,
            user_id: key.user.clone(),
            name: "Fixture".into(),
            url: "https://fixture.invalid".into(),
            token: "fixture-only".into(),
        },
        cookie: String::new(),
        member: w.users.get(&key.user)?.clone(),
        plan: Some("paid".into()),
        cached: serde_json::to_value(w.channels.get(&key.channel)?).ok()?,
        rt: w.rt_generation,
    })
}
impl Capability {
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        let Ok(r) = self.0.lock() else {
            return false;
        };
        enabled()
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.account_epoch == r.epoch
            && shell.core.active_account.as_ref() == Some(&r.key.account)
            && shell
                .core
                .workspaces
                .get(&r.key.team)
                .is_some_and(|w| w.self_user_id == r.key.user)
            && shell
                .channel_tabs
                .fixture
                .as_ref()
                .is_some_and(|c| c.same(self))
    }
    pub async fn execute(&self, request: PreparedRequest) -> Result<Value, Error> {
        if !enabled() {
            return Err(Error::TransportNotConfigured);
        }
        if let RequestBody::Json(body) = &request.body {
            let mut r = self
                .0
                .lock()
                .map_err(|_| Error::Api("fixture_poisoned".into()))?;
            let Some(_) = r.enterprise.as_ref() else {
                return Err(Error::Api("fixture_owner".into()));
            };
            let url = format!(
                "https://edgeapi.slack.com/cache/{}/permissions/info",
                r.key.team
            );
            if request.url.split('?').next() != Some(url.as_str())
                || body["user_id"] != r.key.user
                || body["channel_id"] != r.key.channel
                || body["permissions"] != json!(["MANAGE_CHANNEL_TABS"])
                || body["token"] != "fixture-only"
                || (r.key.team.starts_with('E') && body["enterprise_token"] != "fixture-only")
                || (!r.key.team.starts_with('E') && body.get("enterprise_token").is_some())
                || body.get("enterprise_id").is_some()
                || body.get("as_admin").is_some()
                || body.get("team_ids").is_some()
            {
                return Err(Error::Api("fixture_owner".into()));
            }
            r.requests.push(request.clone());
            if r.manager_rate_limited {
                return Err(Error::RateLimited {
                    retry_after_secs: Some(1),
                });
            }
            return Ok(r.permission_response.clone());
        }
        let RequestBody::Form(form) = &request.body else {
            return Err(Error::Api("fixture_shape".into()));
        };
        let fields = form.iter().cloned().collect::<BTreeMap<_, _>>();
        let endpoint = request
            .url
            .split('?')
            .next()
            .and_then(|s| s.strip_prefix("https://fixture.invalid/api/"))
            .ok_or_else(|| Error::Api("fixture_endpoint".into()))?
            .to_owned();
        #[cfg(test)]
        if matches!(
            endpoint.as_str(),
            "admin.roles.entity.listAssignments"
                | "api.getFlannelHttpUrl"
                | "ai.channelKnowledge.disable"
                | "bookmarks.list"
                | "bookmarks.add"
        ) {
            let pause = self
                .0
                .lock()
                .map_err(|_| Error::Api("fixture_poisoned".into()))?
                .manager_pause
                .clone();
            if let Some((entered, release)) = pause {
                entered.notify_one();
                release.notified().await;
            }
        }
        #[cfg(test)]
        if endpoint == "files.info" {
            let pause = self.0.lock().unwrap().folder_file_pause.clone();
            if let Some((entered, release)) = pause {
                entered.notify_one();
                release.notified().await;
            }
        }
        let mut r = self
            .0
            .lock()
            .map_err(|_| Error::Api("fixture_poisoned".into()))?;
        r.requests.push(request.clone());
        match endpoint.as_str() {
            "api.getFlannelHttpUrl"
                if r.enterprise.is_some()
                    && fields
                        .get("include_external_workspaces")
                        .map(String::as_str)
                        == Some("true") =>
            {
                if r.manager_rate_limited {
                    return Err(Error::RateLimited {
                        retry_after_secs: Some(1),
                    });
                }
                return Ok(
                    json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",r.key.team),"ttl_seconds":60}),
                );
            }
            "admin.roles.entity.listAssignments"
                if fields.get("entity_id").is_some_and(|c| c == CHANNEL) =>
            {
                if r.manager_rate_limited {
                    return Err(Error::RateLimited {
                        retry_after_secs: Some(1),
                    });
                }
                return Ok(r.manager_response.clone());
            }
            "conversations.info" if fields.get("channel").is_some_and(|c| c == CHANNEL) => {
                return Ok(json!({"ok":true,"channel":r.raw}));
            }
            "bookmarks.list"
                if fields.get("channel").is_some_and(|c| c == CHANNEL)
                    && fields.get("include_folders").map(String::as_str) == Some("true")
                    && fields.get("include_legacy_workflows").map(String::as_str)
                        == Some("true") =>
            {
                if r.manager_rate_limited {
                    return Err(Error::RateLimited {
                        retry_after_secs: Some(1),
                    });
                }
                return Ok(json!({"ok":true,"bookmarks":r.folder_bookmarks}));
            }
            "bookmarks.add"
                if fields.get("channel").is_some_and(|c| c == CHANNEL)
                    && fields.get("type").map(String::as_str) == Some("local_folder") =>
            {
                if r.uncertain {
                    return Err(Error::Offline("synthetic lost folder response".into()));
                }
                let bookmark = json!({"id":"Bk90000001","type":"local_folder","title":fields["title"],"channel_id":CHANNEL,"opaque":[3,1]});
                r.folder_bookmarks
                    .as_array_mut()
                    .unwrap()
                    .push(bookmark.clone());
                r.raw["properties"]["tabz"].as_array_mut().unwrap().push(json!({"id":"Ct90000001","type":"folder","label":fields["title"],"data":{"folder_bookmark_id":"Bk90000001"},"opaque":[7,2]}));
                return Ok(json!({"ok":true,"bookmark":bookmark}));
            }
            "files.info" => {
                #[cfg(test)]
                if let Some(error) = r.folder_file_error {
                    return Err(match error {
                        Some(seconds) => Error::RateLimited {
                            retry_after_secs: Some(seconds),
                        },
                        None => Error::Api("synthetic_file_unavailable".into()),
                    });
                }
                let id = fields.get("file").map(String::as_str).unwrap_or("");
                let mut file = serde_json::to_value(file(id)).unwrap();
                if id == "F00000001"
                    && (r.knowledge_file_only
                        || r.raw
                            .pointer("/properties/channel_knowledge_canvas/file_id")
                            .is_some())
                {
                    file["knowledge_canvas_channel_id"] = CHANNEL.into();
                }
                return Ok(json!({"ok":true,"file":file}));
            }
            "ai.channelKnowledge.disable"
                if fields.get("channel_id").is_some_and(|c| c == CHANNEL)
                    && r.raw
                        .pointer("/properties/channel_knowledge_canvas/file_id")
                        .and_then(Value::as_str)
                        == Some("F00000001") =>
            {
                r.raw["properties"]["channel_knowledge_canvas"]["is_enabled"] = false.into();
                if r.knowledge_loss == 1 {
                    return Err(Error::Offline("synthetic lost disable response".into()));
                }
                return Ok(json!({"ok":true}));
            }
            "files.canShare" if fields.get("channel_id").is_some_and(|c| c == CHANNEL) => {
                if r.revoke_on_share {
                    r.manager_response = json!({"ok":true,"role_assignments":[]});
                    r.permission_response = json!({"ok":true,"allowed":[]});
                }
                return Ok(json!({"ok":true,"can_share":true}));
            }
            _ => {}
        }
        if fields.get("channel_id").is_none_or(|c| c != CHANNEL) {
            return Err(Error::Api("fixture_owner".into()));
        }
        if r.uncertain {
            return Err(Error::Offline("synthetic lost response".into()));
        }
        let knowledge_disabled = r
            .raw
            .pointer("/properties/channel_knowledge_canvas/is_enabled")
            == Some(&Value::Bool(false));
        let rows = r.raw["properties"]["tabz"].as_array_mut().unwrap();
        let target = fields.get("id").and_then(|key| {
            rows.iter()
                .position(|row| super_platinum_core::channel_tabs::id(row) == Some(key))
        });
        match endpoint.as_str() {
            "conversations.addTab" => {
                rows.push(json!({"id":"Ct00000003","type":fields.get("type"),"label":fields.get("label"),"data":serde_json::from_str::<Value>(&fields["data"]).unwrap()}));
            }
            "conversations.updateTab" => {
                let i = target.ok_or_else(|| Error::Api("fixture_target".into()))?;
                rows[i]["label"] = fields["label"].clone().into();
            }
            "conversations.removeTab"
                if fields.get("keep_file_share").map(String::as_str) == Some("true")
                    || (fields.get("keep_file_share").is_none() && knowledge_disabled) =>
            {
                rows.remove(target.ok_or_else(|| Error::Api("fixture_target".into()))?);
            }
            "conversations.moveTab" => {
                let row = rows.remove(target.ok_or_else(|| Error::Api("fixture_target".into()))?);
                let i = fields
                    .get("after")
                    .and_then(|a| {
                        rows.iter()
                            .position(|r| super_platinum_core::channel_tabs::id(r) == Some(a))
                    })
                    .map_or(0, |i| i + 1);
                rows.insert(i, row);
            }
            _ => return Err(Error::Api("fixture_endpoint".into())),
        }
        let result = json!({"ok":true,"tabz":rows});
        if endpoint == "conversations.removeTab" && r.knowledge_loss == 2 {
            return Err(Error::Offline("synthetic lost remove response".into()));
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod folder_tests;

#[cfg(test)]
mod folder_operations_tests;
