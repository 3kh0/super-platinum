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
}
fn enabled() -> bool {
    cfg!(test)
        || (matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("channel-tabs-canvas"
                | "channel-tabs-canvas-narrow"
                | "channel-canvas-surface"
                | "channel-canvas-surface-narrow")
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
    Some(Owner {
        key: key.clone(),
        epoch: shell.core.account_epoch,
        generation: shell.channel_generation,
        transport: None,
        fixture: Some(cap),
        client: SlackClient::default(),
        session: WorkspaceSession {
            team_id: key.team.clone(),
            enterprise_id: None,
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
        let mut r = self
            .0
            .lock()
            .map_err(|_| Error::Api("fixture_poisoned".into()))?;
        r.requests.push(request.clone());
        match endpoint.as_str() {
            "conversations.info" if fields.get("channel").is_some_and(|c| c == CHANNEL) => {
                return Ok(json!({"ok":true,"channel":r.raw}));
            }
            "files.info" => {
                return Ok(
                    json!({"ok":true,"file":file(fields.get("file").map(String::as_str).unwrap_or(""))}),
                );
            }
            "files.canShare" if fields.get("channel_id").is_some_and(|c| c == CHANNEL) => {
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
                if fields.get("keep_file_share").map(String::as_str) == Some("true") =>
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
        Ok(json!({"ok":true,"tabz":rows}))
    }
}
#[cfg(test)]
mod tests;
