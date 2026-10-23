use super::*;
use crate::{app_home::channel_fetches, media::MediaUri};
use std::sync::{Arc, Mutex, OnceLock};
use super_platinum_core::{
    MediaAssetKind,
    config::{Session, WorkspaceSession},
    slack::{Error, Transport, api, client::RequestBody},
    state::{ChannelMessages, Workspace},
};

pub(super) const ACCOUNT: &str = "af304cc3-6681-4acd-bf35-3c27c68c555c";
pub(super) const TEAM: &str = "T1";
pub(super) const USER: &str = "UCHANEMOJI";
pub(super) const CHANNEL: &str = "CCHANEMOJI";
pub(super) const ROOT: &str = "1781524800.000001";
pub(super) const REPLY: &str = "1781524801.000001";
pub(super) const URL: &str = "https://fixture.invalid/channel-thread-emoji.png";
pub(super) const CHANNEL_DRAFT: &str = "Channel draft caf\u{e9} \u{4e2d}";
pub(super) const THREAD_DRAFT: &str = "Thread draft kept \u{1f642}";
const COOKIE: &str = "synthetic-channel-thread-cookie";
const TOKEN: &str = "synthetic-channel-thread-token";
struct Closed {
    transport: Arc<Transport>,
    epoch: u64,
    socket: u64,
    boot: Value,
    requests: Vec<Value>,
    witnesses: Vec<channel_fetches::EmojiStandard>,
}
static CLOSED: OnceLock<Mutex<Option<Closed>>> = OnceLock::new();
fn closed() -> &'static Mutex<Option<Closed>> {
    CLOSED.get_or_init(|| Mutex::new(None))
}
pub(crate) fn context_enabled(shell: &ShellState) -> bool {
    if !enabled()
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
            .is_some_and(|t| Arc::ptr_eq(t, &c.transport))
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
}
pub(super) fn raw(ts: &str) -> Value {
    json!({"type":"message","ts":ts,"thread_ts":ROOT,"user":USER,"text":"Known unknown invalid","reply_count":if ts==ROOT{1}else{0},"blocks":[{"type":"rich_text","block_id":"channel-thread-source","opaque":{"keep":true},"elements":[{"type":"rich_text_section","elements":[
      {"type":"text","text":"Known "},{"type":"emoji","name":"conflicting_original","unicode":"1f44d-1f3fd","skin_tone":6,"display_alias":"wrong_alias","display_url":URL},
      {"type":"text","text":" Unknown "},{"type":"emoji","name":"unknown_original","unicode":"0041","display_url":"https://fixture.invalid/unknown-never.png"},
      {"type":"text","text":" Invalid "},{"type":"emoji","name":"invalid_original","unicode":"1F44D","display_url":"https://fixture.invalid/invalid-never.png"}
    ]}]}]})
}
pub(crate) fn install(shell: &mut ShellState) {
    assert!(
        enabled()
            && shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.cache.is_none()
    );
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        let session = WorkspaceSession {
            team_id: TEAM.into(),
            user_id: USER.into(),
            enterprise_id: None,
            name: "Emoji projection".into(),
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
        shell.core.active_channel = Some(CHANNEL.into());
        shell.core.session = Some(Session {
            d_cookie: COOKIE.into(),
            unified_user_client_enabled: Default::default(),
            workspaces: [(TEAM.into(), session.clone())].into(),
        });
        shell.core.transport = Some(transport.clone());
        let user = json!({"id":USER,"name":"emoji-reviewer","team_id":TEAM,"profile":{"display_name":"Emoji reviewer"}});
        let boot = json!({"ok":true,"team":{"id":TEAM,"name":"Emoji projection"},"self":user,"users":[user],"channels":[{"id":CHANNEL,"name":"emoji-projection","is_channel":true,"is_member":true,"updated":100,"previous_names":[]}],"prefs":{}});
        let read = super_platinum_core::state::posting_metadata::authority::BootRead::begin_full(
            &mut shell.core,
            &transport,
            &session,
        )
        .unwrap();
        assert!(read.apply(&mut shell.core, boot.clone(), true).unwrap());
        shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 7;
        let mut channel = ChannelMessages::default();
        channel.upsert(serde_json::from_value(raw(ROOT)).unwrap());
        let mut thread = channel.clone();
        thread.upsert(serde_json::from_value(raw(REPLY)).unwrap());
        shell
            .core
            .workspaces
            .get_mut(TEAM)
            .unwrap()
            .messages
            .insert(CHANNEL.into(), channel);
        shell.core.threads.clear();
        shell
            .core
            .threads
            .insert((TEAM.into(), CHANNEL.into(), ROOT.into()), thread);
        shell.core.pending_scroll_to = None;
        shell.thread_root = Some(ROOT.into());
        shell.thread_target = None;
        shell.thread_at_bottom = true;
        shell.main_view = crate::state::MainView::Home;
        shell.channel_tab = crate::state::ChannelTab::Messages;
        shell.overlay = None;
        shell.loading = false;
        shell.toast = None;
        shell.core.composer.text = CHANNEL_DRAFT.into();
        shell.core.thread_composer.text = THREAD_DRAFT.into();
        shell.core.composer_attachments.clear();
        shell.core.thread_composer_attachments.clear();
        shell.core.drafts = Default::default();
        *closed().lock().unwrap() = Some(Closed {
            transport,
            epoch: shell.core.account_epoch,
            socket: 7,
            boot,
            requests: Vec::new(),
            witnesses: Vec::new(),
        });
        shell.refresh_from_core();
        shell.checkpoint_composers();
        assert!(shell.channel_window_is_canonical());
    }
}
pub(super) async fn assignments(state: Signal<ShellState>) -> Result<Value, String> {
    if !context_enabled(&state.peek()) {
        return Err("Original fixture context unavailable".into());
    }
    let witness=channel_fetches::fixture_emoji_assignments(state,TEAM,move|request|async move{
        let shell=state.peek();if !context_enabled(&shell){return Err(Error::Offline("Original fixture owner retired".into()));}
        let original=api::list_experiments(&shell.core.client,&shell.core.session.as_ref().unwrap().workspaces[TEAM]);
        if request.method!=original.method||request.headers!=original.headers||request_url(&request.url).zip(request_url(&original.url)).is_none_or(|(a,b)|a!=b){return Err(Error::Offline("Assignment factory changed".into()));}
        let (RequestBody::Form(actual),RequestBody::Form(expected))=(&request.body,&original.body)else{return Err(Error::Offline("Original assignment form absent".into()));};
        if actual!=expected{return Err(Error::Offline("Original assignment fields changed".into()));}
        let args:std::collections::BTreeMap<_,_>=actual.iter().filter(|(k,_)|k!="token").cloned().collect();
        let raw=json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}});
        let socket=shell.core.workspaces[TEAM].rt_generation;
        let mut lock=closed().lock().unwrap();let c=lock.as_mut().unwrap();
        if c.requests.len()>=2||socket!=7+c.requests.len()as u64{return Err(Error::Offline("Unexpected original assignment read".into()));}
        c.requests.push(json!({"serial":c.requests.len()+1,"socket":socket,"method":"POST","path":"/api/experiments.getByUser","args":args,"tokenPresent":actual.iter().filter(|(k,v)|k=="token"&&v==TOKEN).count()==1,"factoryMatched":true,"response":raw}));Ok(raw)
    }).await?;
    if !witness.current(&state.peek()) {
        return Err("Accepted assignment retired".into());
    }
    let receipt = witness.snapshot(&state.peek());
    closed()
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .witnesses
        .push(witness);
    Ok(receipt)
}
pub(super) fn retire(shell: &mut ShellState) -> Result<Value, String> {
    if !context_enabled(shell) || shell.core.workspaces[TEAM].rt_generation != 7 {
        return Err("Original socket unavailable".into());
    }
    let original = closed()
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .witnesses
        .first()
        .cloned()
        .ok_or("Original assignment unavailable")?;
    let before = original.snapshot(shell);
    shell.core.workspaces.get_mut(TEAM).unwrap().rt_generation = 8;
    closed().lock().unwrap().as_mut().unwrap().socket = 8;
    let _ = channel_fetches::connected(shell, TEAM, 8);
    let after = original.snapshot(shell);
    let available = channel_fetches::emoji_standard(shell, TEAM).is_some();
    if before["current"] != true || after["current"] != false || available {
        return Err("Original socket map did not retire".into());
    }
    Ok(
        json!({"kind":"synthetic-socket-connected","before":before,"after":after,"socket":8,"accessorAvailable":available,"networkDispatched":false,"workResumed":false}),
    )
}
pub(super) fn media(shell: &ShellState) -> Result<Value, String> {
    if !context_enabled(shell) {
        return Err("Original fixture context missing".into());
    }
    let id = shell
        .media
        .register_image(MediaAssetKind::Emoji, URL, "image/png");
    if shell.media.is_ready(&id) {
        return Err("Original image already ready".into());
    }
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/png/icon_16.png"
    ));
    let before = id.webview_uri_at(shell.media_epoch);
    shell
        .media
        .insert(id.clone(), "image/png", Arc::<[u8]>::from(bytes.as_slice()));
    Ok(
        json!({"url":URL,"id":id.uri(),"uriBefore":before,"epochBefore":shell.media_epoch,"bytes":bytes.len(),"mime":"image/png","readyBefore":false,"readyAfter":shell.media.is_ready(&id)}),
    )
}
pub(super) fn snapshot(shell: &ShellState) -> Value {
    let (boot, requests, witnesses) = {
        let lock = closed().lock().unwrap();
        let c = lock.as_ref().unwrap();
        (c.boot.clone(), c.requests.clone(), c.witnesses.clone())
    };
    let w = &shell.core.workspaces[TEAM];
    let bag = &w.messages[CHANNEL];
    let thread = &shell.core.threads[&(TEAM.into(), CHANNEL.into(), ROOT.into())];
    let drafts: Vec<_> = shell
        .core
        .drafts
        .items
        .iter()
        .map(|(k, v)| json!({"key":k,"value":v}))
        .collect();
    json!({"contextCurrent":context_enabled(shell),"account":shell.core.active_account,"epoch":shell.core.account_epoch,"team":TEAM,"user":USER,"socket":w.rt_generation,
      "boot":boot,"requests":requests,"witnesses":witnesses.iter().map(|w|w.snapshot(shell)).collect::<Vec<_>>(),"selected":channel_fetches::emoji_standard(shell,TEAM).map(|w|w.snapshot(shell)),
      "canonical":shell.channel_window_is_canonical(),"channel":CHANNEL,"root":ROOT,"channelGeneration":shell.channel_generation,"channelProjection":shell.channel_projection_generation,"threadProjection":shell.thread_window.projection_generation,
      "channelScope":shell.timeline_scope(),"threadScope":shell.thread_scope(),"channelRaw":bag.messages,"threadRaw":thread.messages,"channelPending":bag.pending,"threadPending":thread.pending,"pendingScroll":shell.core.pending_scroll_to.as_ref().map(|(channel,target)|json!({"channel":channel,"target":format!("{target:?}")})),
      "channelRows":shell.channel_rows.iter().map(|r|&r.id).collect::<Vec<_>>(),"threadRows":shell.thread_window.rows.iter().map(|r|&r.ts).collect::<Vec<_>>(),"channelWindow":[shell.timeline_start,shell.timeline_end],"threadWindow":[shell.thread_window.start,shell.thread_window.end],
      "composer":shell.core.composer.text,"threadComposer":shell.core.thread_composer.text,"drafts":{"channel":shell.core.drafts.channel,"thread":shell.core.drafts.thread,"items":drafts},"attachments":shell.core.composer_attachments,"threadAttachments":shell.core.thread_composer_attachments,
      "customEmoji":w.custom_emoji,"customRevision":w.custom_emoji_revision,"mediaEpoch":shell.media_epoch,"registered":{"known":shell.media.registered_source_count(URL),"unknown":shell.media.registered_source_count("https://fixture.invalid/unknown-never.png"),"invalid":shell.media.registered_source_count("https://fixture.invalid/invalid-never.png")}})
}

fn request_url(raw: &str) -> Option<(url::Url, std::collections::BTreeMap<String, String>)> {
    let mut url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut query = std::collections::BTreeMap::new();
    for (key, value) in url.query_pairs() {
        let value = match key.as_ref() {
            "_x_id" | "_x_csid" | "fp" => {
                let bytes = value.as_bytes();
                if bytes.len() != 32
                    || !bytes
                        .iter()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
                    || bytes[12] != b'4'
                    || !matches!(bytes[16], b'8' | b'9' | b'a' | b'b')
                {
                    return None;
                }
                String::new()
            }
            "_x_desktop_ia"
            | "_x_foreground"
            | "_x_frontend_build_type"
            | "_x_gantry"
            | "_x_num_retries"
            | "_x_version_ts"
            | "slack_route" => value.into_owned(),
            _ => return None,
        };
        if query.insert(key.into_owned(), value).is_some() {
            return None;
        }
    }
    if query.len() != 10 {
        return None;
    }
    url.set_query(None);
    Some((url, query))
}
