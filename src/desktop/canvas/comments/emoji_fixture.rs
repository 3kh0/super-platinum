//! Exact disconnected Canvas source; original reads and selected-map custody.
use super::*;
use crate::{app_home::channel_fetches, media::MediaUri};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use super_platinum_core::{
    MediaAssetKind,
    config::{Session, WorkspaceSession},
    slack::{Transport, client::RequestBody},
    state::Workspace,
};
mod source;
pub(in crate::canvas::comments) use source::{assignments, media, retire};
pub(in crate::canvas) const ACCOUNT: &str = "ce733150-11d9-476f-80ef-9c49462dde33";
const TEAM: &str = "T1";
const USER: &str = "UCANEMOJI";
pub(in crate::canvas) const ROOT: &str = "1719800000.000100";
const REPLY: &str = "1719800001.000100";
const HIDDEN: &str = "1719800020.000100";
const FILE_ROOT: &str = "1719800030.000100";
const URL: &str = "https://fixture.invalid/canvas-comment-emoji.png";
const HIDDEN_URL: &str = "https://fixture.invalid/canvas-hidden-never.png";
const FILE_URL: &str = "https://fixture.invalid/canvas-override-never.png";
const COOKIE: &str = "synthetic-canvas-emoji-cookie";
const TOKEN: &str = "synthetic-canvas-emoji-token";
struct Closed {
    transport: Arc<Transport>,
    epoch: u64,
    socket: u64,
    boot: Value,
    requests: Vec<Value>,
    witnesses: Vec<channel_fetches::EmojiStandard>,
    reads: Vec<Value>,
    retained: Value,
}
static CLOSED: OnceLock<Mutex<Option<Closed>>> = OnceLock::new();
fn closed() -> &'static Mutex<Option<Closed>> {
    CLOSED.get_or_init(|| Mutex::new(None))
}
pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_CANVAS_COMMENT_EMOJI_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-comment-emoji" | "canvas-comment-emoji-narrow")
        )
}
pub(crate) fn context_enabled(shell: &ShellState) -> bool {
    if !requested()
        || shell.core.active_account.as_deref() != Some(ACCOUNT)
        || shell.core.active_team.as_deref() != Some(TEAM)
        || shell.core.cache.is_some()
    {
        return false;
    }
    let lock = closed().lock().unwrap();
    let Some(c) = lock.as_ref() else {
        return false;
    };
    shell.core.account_epoch == c.epoch
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| Arc::ptr_eq(t, &c.transport) && t.http().is_err())
        && shell.core.session.as_ref().is_some_and(|s| {
            s.d_cookie == COOKIE
                && s.unified_user_client_enabled.is_empty()
                && s.workspaces.len() == 1
                && s.workspaces.get(TEAM).is_some_and(|w| {
                    w.team_id == TEAM
                        && w.user_id == USER
                        && w.token == TOKEN
                        && w.url == "https://example.test"
                        && w.enterprise_id.is_none()
                })
        })
        && shell
            .core
            .workspaces
            .get(TEAM)
            .is_some_and(|w| w.self_user_id == USER && w.rt_generation == c.socket)
        && shell.self_account.user_id == USER
        && shell.canvas.as_ref().is_some_and(|f| f.id == "Ffixture")
}
pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() {
        return;
    }
    assert!(
        shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.cache.is_none()
    );
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        let session = WorkspaceSession {
            team_id: TEAM.into(),
            user_id: USER.into(),
            enterprise_id: None,
            name: "Canvas emoji".into(),
            url: "https://example.test".into(),
            token: TOKEN.into(),
        };
        let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
        shell.core.workspaces.clear();
        shell
            .core
            .workspaces
            .insert(TEAM.into(), Workspace::from_session(&session));
        shell.core.active_account = Some(ACCOUNT.into());
        shell.core.active_team = Some(TEAM.into());
        shell.core.active_channel = Some("C1".into());
        shell.core.session = Some(Session {
            d_cookie: COOKIE.into(),
            unified_user_client_enabled: Default::default(),
            workspaces: [(TEAM.into(), session.clone())].into(),
        });
        shell.core.transport = Some(transport.clone());
        let user = json!({"id":USER,"team_id":TEAM,"name":"canvas-reviewer","profile":{"display_name":"Canvas reviewer"}});
        let boot = json!({"ok":true,"team":{"id":TEAM,"name":"Canvas emoji"},"self":user,"users":[user],"channels":[{"id":"C1","name":"canvas-review","is_channel":true,"is_member":true,"updated":100,"previous_names":[]}],"prefs":{},"subteams":{"self":[]}});
        let read = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
            &mut shell.core,
            &transport,
            &session,
        )
        .unwrap();
        assert!(read.apply(&mut shell.core, boot.clone(), true).unwrap());
        shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 7;
        *closed().lock().unwrap() = Some(Closed {
            transport,
            epoch: shell.core.account_epoch,
            socket: 7,
            boot,
            requests: vec![],
            witnesses: vec![],
            reads: vec![],
            retained: Value::Null,
        });
        shell.refresh_from_core();
        assert!(context_enabled(shell));
    }
}
pub(in crate::canvas) fn reply_path(shell: &ShellState) -> Option<PathBuf> {
    if !context_enabled(shell) {
        return None;
    }
    let base = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")?);
    let path = PathBuf::from(std::env::var_os("SUPER_PLATINUM_CANVAS_REPLY_DB")?);
    (base.is_absolute() && base.is_dir() && path == base.join("replies.sqlite")).then_some(path)
}
fn raw(ts: &str, label: &str, url: &str) -> Value {
    let mut row = json!({"type":"message","ts":ts,"user":USER,"text":format!("DECOY {label}"),"blocks":[{"type":"rich_text","block_id":format!("original-{ts}"),"opaque":{"keep":[false,0,null]},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":format!("{label} ")},{"type":"emoji","name":"conflicting_original","unicode":"1f44d-1f3fd","skin_tone":6,"display_alias":"wrong_alias","display_url":url},{"type":"text","text":" unknown "},{"type":"emoji","name":"unknown_original","unicode":"0041","display_url":"https://fixture.invalid/canvas-unknown-never.png"},{"type":"text","text":" invalid "},{"type":"emoji","name":"invalid_original","unicode":"1F44D","display_url":"https://fixture.invalid/canvas-invalid-never.png"}]}]}]});
    if ts == REPLY {
        row["thread_ts"] = json!(ROOT);
    } else {
        row["subtype"] = json!("document_comment_root");
        row["reply_count"] = json!(if ts == ROOT { 1 } else { 0 });
        row["document_comment"] = json!({"thread_id":format!("quip-{ts}"),"is_visible":ts!=HIDDEN});
    }
    if ts == FILE_ROOT {
        row["files"] = json!([{"id":"FCANEMOJI","mode":"hosted","filetype":"svg","name":"Original canonical file","title":"Not the file name","created":101,"permalink":"https://example.test/canonical/FCANEMOJI","is_deleted":false}]);
    }
    row
}
pub(in crate::canvas::comments) fn response(
    shell: &ShellState,
    ticket: &Ticket,
) -> Result<Value, Error> {
    if !context_enabled(shell) || !ticket.owner.current(shell) || ticket.cursor.is_some() {
        return Err(Error::Offline("Canvas emoji fixture retired".into()));
    }
    let response = match &ticket.query {
        Query::Stream(Filter::Recent) => {
            json!({"ok":true,"has_more":false,"messages":[raw(ROOT,"Root",URL),raw(REPLY,"Last reply",URL),raw(HIDDEN,"Hidden",HIDDEN_URL),raw(FILE_ROOT,"Superseded",FILE_URL)]})
        }
        Query::Replies(root) if root == ROOT => {
            json!({"ok":true,"has_more":false,"messages":[raw(ROOT,"Root",URL),raw(REPLY,"Last reply",URL)]})
        }
        _ => return Err(Error::Offline("Unexpected Canvas emoji query".into())),
    };
    let request = super::request(
        &shell.core.client,
        &shell.core.session.as_ref().unwrap().workspaces[TEAM],
        ticket,
    )?;
    let RequestBody::Form(fields) = request.body else {
        return Err(Error::Offline("Comment form absent".into()));
    };
    let token_present = fields
        .iter()
        .filter(|(k, v)| k == "token" && v == TOKEN)
        .count()
        == 1;
    let mut lock = closed().lock().unwrap();
    let c = lock.as_mut().unwrap();
    c.reads.push(json!({"serial":ticket.serial,"query":format!("{:?}",ticket.query),"method":request.method,"url":request.url,"fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>(),"tokenPresent":token_present,"response":response}));
    Ok(response)
}
pub(in crate::canvas) fn observe(shell: &ShellState, model: &State) {
    if !context_enabled(shell) {
        return;
    }
    let rows=model.rows.iter().map(|r| {
        let token=super::file_source::token(shell,model,&r.message);
        json!({"message":r.message,"preview":r.preview,"body":format!("{:?}",r.body.body),"previewBody":r.preview_body.as_ref().map(|v|format!("{:?}",v.body)),"file":token.map(|t|json!({"id":t.id(),"name":t.name(),"url":t.url(),"ts":t.message_ts()}))})
    }).collect::<Vec<_>>();
    let retained = json!({"serial":model.serial,"query":format!("{:?}",model.query),"ownerCurrent":model.owner.as_ref().is_some_and(|o|o.current(shell)),"owner":model.owner.as_ref().map(|o|json!({"file":o.file,"visit":o.visit,"team":o.scope.team})),"loading":model.loading,"error":model.error,"rows":rows,"root":model.root,"selected":model.emoji_standard.as_ref().map(|w|w.snapshot(shell))});
    closed().lock().unwrap().as_mut().unwrap().retained = retained;
}
pub(in crate::canvas::comments) fn isolation(shell: &ShellState) -> Value {
    json!({"syntheticContext":context_enabled(shell),"noHttp":shell.core.transport.as_ref().is_some_and(|t|t.http().is_err()),"networkDispatched":false,"account":ACCOUNT,"team":TEAM,"user":USER})
}
pub(in crate::canvas::comments) fn snapshot(shell: &ShellState) -> Value {
    let (boot, requests, reads, witnesses, retained) = {
        let lock = closed().lock().unwrap();
        let c = lock.as_ref().unwrap();
        (
            c.boot.clone(),
            c.requests.clone(),
            c.reads.clone(),
            c.witnesses.clone(),
            c.retained.clone(),
        )
    };
    let w = &shell.core.workspaces[TEAM];
    json!({"contextCurrent":context_enabled(shell),"account":ACCOUNT,"epoch":shell.core.account_epoch,"team":TEAM,"user":USER,"socket":w.rt_generation,"visit":shell.shortcut_generation,"boot":boot,"requests":requests,"reads":reads,"witnesses":witnesses.iter().map(|w|w.snapshot(shell)).collect::<Vec<_>>(),"selected":channel_fetches::emoji_standard(shell,TEAM).map(|w|w.snapshot(shell)),"retained":retained,"mediaEpoch":shell.media_epoch,"customEmoji":w.custom_emoji,"customRevision":w.custom_emoji_revision,"registered":{"known":shell.media.registered_source_count(URL),"hidden":shell.media.registered_source_count(HIDDEN_URL),"file":shell.media.registered_source_count(FILE_URL),"unknown":shell.media.registered_source_count("https://fixture.invalid/canvas-unknown-never.png"),"invalid":shell.media.registered_source_count("https://fixture.invalid/canvas-invalid-never.png")}})
}
