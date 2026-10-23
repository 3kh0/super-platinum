//! Closed supplied-URL appearance; no workspace emoji publication or network.
use super::*;
use crate::media::MediaUri;
use std::sync::OnceLock;
use super_platinum_core::{
    MediaAssetKind,
    config::Session,
    slack::{Transport, api},
};
const USER: &str = "UCOMBINED01";
const COOKIE: &str = "synthetic-combined-cookie";
const TOKEN: &str = "synthetic-combined-token";
struct Closed {
    transport: Arc<Transport>,
    epoch: u64,
    seed: Value,
    requests: Vec<Value>,
    witness: Option<crate::app_home::channel_fetches::EmojiStandard>,
    socket: Option<u64>,
}
static CLOSED: OnceLock<Mutex<Option<Closed>>> = OnceLock::new();
fn closed() -> &'static Mutex<Option<Closed>> {
    CLOSED.get_or_init(|| Mutex::new(None))
}
pub(crate) fn context_enabled(shell: &ShellState) -> bool {
    if !named()
        || shell.core.active_account.as_deref() != Some("8624eb74-23f4-4bee-9872-e37f392485c0")
        || shell.core.active_team.as_deref() != Some("T1")
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
                && s.workspaces.get("T1").is_some_and(|w| {
                    w.team_id == "T1"
                        && w.user_id == USER
                        && w.token == TOKEN
                        && w.url == "https://example.test"
                        && w.name == "Combined emoji fixture"
                        && w.enterprise_id.is_none()
                })
        })
        && shell.core.workspaces.get("T1").is_some_and(|w| {
            w.self_user_id == USER && c.socket.is_none_or(|g| w.rt_generation == g)
        })
}
pub(crate) fn install(shell: &mut ShellState) {
    if !named() {
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
            team_id: "T1".into(),
            user_id: USER.into(),
            enterprise_id: None,
            name: "Combined emoji fixture".into(),
            url: "https://example.test".into(),
            token: TOKEN.into(),
        };
        let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
        let w = shell
            .core
            .workspaces
            .get_mut("T1")
            .expect("original fixture workspace");
        w.users.clear();
        w.channels.clear();
        w.self_user_id = USER.into();
        w.rt_generation = 6;
        shell.self_account.user_id = USER.into();
        shell.core.session = Some(Session {
            d_cookie: COOKIE.into(),
            unified_user_client_enabled: Default::default(),
            workspaces: [("T1".into(), session)].into(),
        });
        shell.core.transport = Some(transport.clone());
        *closed().lock().unwrap() = Some(Closed {
            transport,
            epoch: shell.core.account_epoch,
            seed: Value::Null,
            requests: Vec::new(),
            witness: None,
            socket: None,
        });
    }
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !context_enabled(&state.peek()) {
        return Err("Original synthetic context missing".into());
    }
    let user = json!({"id":USER,"name":"combined-self","team_id":"T1","profile":{}});
    let boot = json!({"ok":true,"team":{"id":"T1","name":"Combined emoji fixture"},"self":user,"users":[user],"channels":[{"id":"CCOMMENT01","name":"combined-comments","team_id":"T1","is_channel":true,"is_member":true,"updated":100,"previous_names":[]}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":"T1","name":"Combined emoji fixture"},"workspaces":[{"id":"T1"}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    closed().lock().unwrap().as_mut().unwrap().seed =
        json!({"boot":boot,"init":init,"accepted":false});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    for _ in 0..100 {
        if crate::app_home::channel_fetches::canonical::capture(&mut state.write(), "T1").is_ok() {
            let socket = state.peek().core.workspaces["T1"].rt_generation;
            if socket != 7 {
                return Err("Original accepted socket generation changed".into());
            }
            let mut lock = closed().lock().unwrap();
            let c = lock.as_mut().unwrap();
            c.socket = Some(socket);
            c.seed["accepted"] = json!(true);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err("Canonical synthetic Boot/Init unavailable".into())
}
pub(crate) async fn assignments(state: Signal<ShellState>) -> Result<Value, String> {
    if !context_enabled(&state.peek()) {
        return Err("Original synthetic context retired".into());
    }
    let witness=crate::app_home::channel_fetches::fixture_emoji_assignments(state,"T1",move |request|async move{
      let shell=state.peek();if !context_enabled(&shell){return Err(Error::Offline("Combined context retired".into()));}
      let session=&shell.core.session.as_ref().unwrap().workspaces["T1"];
      if !own_edit::same(&request,&api::list_experiments(&shell.core.client,session)){return Err(Error::Offline("Wrong original assignments factory".into()));}
      let raw=json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}});
      let wire=own_edit::wire(&request)?;
      let token_present=match &request.body{RequestBody::Form(fields)=>fields.iter().filter(|(k,v)|k=="token"&&v==TOKEN).count()==1,_=>false};
      let mut lock=closed().lock().unwrap();let c=lock.as_mut().unwrap();if !c.requests.is_empty(){return Err(Error::Offline("Duplicate assignment read".into()));}
      c.requests.push(json!({"request":wire,"response":raw,"tokenPresent":token_present,"factoryMatched":true}));Ok(raw)
    }).await?;
    if !witness.current(&state.peek()) {
        return Err("Original shared assignment receipt retired".into());
    }
    let proof = witness.snapshot(&state.peek());
    closed().lock().unwrap().as_mut().unwrap().witness = Some(witness);
    Ok(proof)
}
pub(crate) const CASE: &str = "rich-emoji-combined";
pub(crate) const TS: &str = "1781524801.000001";
pub(crate) const URL_A: &str = "https://fixture.invalid/combined-emoji-a.png";
pub(crate) const URL_B: &str = "https://fixture.invalid/combined-emoji-b.png";
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_EMOJI_COMBINED_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok(CASE)
}
pub(crate) fn blocks(replacement: bool) -> Value {
    json!([{"type":"rich_text","block_id":"original-combined-emoji","opaque":{"keep":true},"elements":[{"type":"rich_text_section","elements":[
      {"type":"text","text":"Known "},
      {"type":"emoji","name":"conflicting_original","unicode":"1f44d-1f3fd","skin_tone":6,"display_alias":"wrong_alias","display_url":if replacement{URL_B}else{URL_A},"opaque":{"keep":"known"}},
      {"type":"text","text":" Unknown "},
      {"type":"emoji","name":"unknown_original","unicode":"0041","display_alias":"wrong_unknown","display_url":"https://fixture.invalid/unknown-never-image.png"},
      {"type":"text","text":" Invalid "},
      {"type":"emoji","name":"invalid_original","unicode":"1F44D","display_alias":"wrong_invalid","display_url":"https://fixture.invalid/invalid-never-image.png"},
      {"type":"text","text":" Standard "},{"type":"emoji","name":"wave"}
    ]}]}])
}
pub(crate) fn decorate(ts: &str, next: bool, response: &mut Value) {
    if !named() || next || ts != ROOT {
        return;
    }
    let row = &mut response["messages"][1];
    assert_eq!(row["ts"], TS);
    row["text"] = json!(
        "Known :conflicting_original: Unknown :unknown_original: Invalid :invalid_original: Standard :wave:"
    );
    row["blocks"] = blocks(false);
    row["combined_fixture_opaque"] = json!({"keep":"received"});
}
pub(crate) fn replacement() -> Value {
    json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":{"ts":TS,"thread_ts":ROOT,"user":"U1","text":"Known :conflicting_original: Unknown :unknown_original: Invalid :invalid_original: Standard :wave:","blocks":blocks(true)}})
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let enabled = context_enabled(shell);
    let (seed, requests, witness) = {
        let lock = closed().lock().unwrap();
        let c = lock.as_ref().unwrap();
        (c.seed.clone(), c.requests.clone(), c.witness.clone())
    };
    let w = shell.core.workspaces.get("T1");
    json!({"named":named(),"syntheticContext":enabled,"noLiveNetwork":true,"httpCapability":false,"noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none(),"customEmoji":w.map(|w|&w.custom_emoji),"customRevision":w.map(|w|w.custom_emoji_revision),"mediaEpoch":shell.media_epoch,"seed":seed,"requests":requests,"standard":witness.map(|w|w.snapshot(shell)),"registered":{"a":shell.media.registered_source_count(URL_A),"b":shell.media.registered_source_count(URL_B),"unknown":shell.media.registered_source_count("https://fixture.invalid/unknown-never-image.png"),"invalid":shell.media.registered_source_count("https://fixture.invalid/invalid-never-image.png")}})
}
pub(crate) fn deliver_media(shell: &ShellState, replacement: bool) -> Result<Value, String> {
    if !context_enabled(shell) {
        return Err("Closed original display media required".into());
    }
    let url = if replacement { URL_B } else { URL_A };
    let id = shell
        .media
        .register_image(MediaAssetKind::Emoji, url, "image/png");
    if shell.media.is_ready(&id) {
        return Err("Pending original URL already has bytes".into());
    }
    let old = shell
        .media
        .register_image(MediaAssetKind::Emoji, URL_A, "image/png");
    if replacement && (old == id || !shell.media.is_ready(&old)) {
        return Err("Replacement must retain distinct original ready asset".into());
    }
    let bytes: &[u8] = if replacement {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/png/icon_32.png"
        ))
    } else {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/png/icon_16.png"
        ))
    };
    let uri = id.webview_uri_at(shell.media_epoch);
    shell
        .media
        .insert(id.clone(), "image/png", Arc::<[u8]>::from(bytes));
    Ok(
        json!({"url":url,"id":id.uri(),"uriBefore":uri,"epochBefore":shell.media_epoch,"readyBefore":false,"readyAfter":shell.media.is_ready(&id),"originalId":old.uri(),"originalStillReady":shell.media.is_ready(&old),"replacement":replacement,"bytes":bytes.len(),"mime":"image/png"}),
    )
}
