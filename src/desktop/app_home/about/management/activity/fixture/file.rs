//! Exact named offline file receipts. Metadata and navigation remain separate reads.
use super::*;
use std::time::Duration;
use super_platinum_core::slack::{
    Transport, api::custom::texty::file_title_source::OrderedFileInfo,
};

pub(crate) const ACCOUNT: &str = "synthetic-workflow-activity-file";
pub(crate) const LIST: &str = "FCANON003";
pub(crate) const CANVAS: &str = "FCANON002";
pub(crate) const LIST_LABEL: &str = "R\u{e9}vision & \u{4e2d} List";
pub(crate) const CANVAS_LABEL: &str = "Canvas & caf\u{e9} \u{4e2d}";
pub(crate) const ROW: &str = "Original Activity List destination row";
const LIST_RAW: &str = include_str!("file/list.json");
const CANVAS_RAW: &str = include_str!("file/canvas.json");
mod recovery;

pub(crate) fn emoji_case(value: &str) -> bool {
    matches!(
        value,
        "file-emoji" | "file-emoji-retirement" | "file-emoji-cold"
    )
}
pub(crate) fn emoji_requested() -> bool {
    requested() && emoji_case(&case())
}
pub(crate) fn cold_requested() -> bool {
    requested() && case() == "file-emoji-cold"
}
pub(crate) fn encoded_requested() -> bool {
    requested() && case() == "file-encoded"
}
pub(crate) fn list_label() -> &'static str {
    if encoded_requested() {
        "R\u{e9}vision <b> &lt;"
    } else if cold_requested() {
        "R\u{e9}vision & \u{4e2d} :wave: :activity_cold: :unknown:"
    } else if emoji_requested() {
        "R\u{e9}vision & \u{4e2d} :wave: :activity_ready_alias: :unknown:"
    } else {
        LIST_LABEL
    }
}
pub(crate) fn destination_label() -> &'static str {
    if encoded_requested() {
        "R\u{e9}vision &lt;b&gt; &amp;lt;"
    } else {
        list_label()
    }
}
pub(crate) fn canvas_label() -> &'static str {
    if encoded_requested() {
        "Canvas &lt;i&gt; &amp;lt;"
    } else if emoji_requested() {
        "Canvas & caf\u{e9} :wave: :activity_cold: :unknown:"
    } else {
        CANVAS_LABEL
    }
}
fn original(id: &str, emoji: bool) -> Result<&'static str, Error> {
    match (id, emoji) {
        (LIST, true) => Ok(include_str!("file/emoji-list.json")),
        (CANVAS, true) => Ok(include_str!("file/emoji-canvas.json")),
        (LIST, false) => Ok(LIST_RAW),
        (CANVAS, false) => Ok(CANVAS_RAW),
        _ => Err(closed()),
    }
}

#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    account: Option<String>,
    epoch: u64,
    credentials: String,
    transport: Arc<Transport>,
    requests: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    mode: Mode,
    metadata: recovery::Plan,
    held: Option<(usize, oneshot::Sender<()>)>,
}
#[derive(Clone, Copy, Default)]
pub(crate) enum Mode {
    #[default]
    Success,
    Malformed,
    WrongId,
    Unsupported,
    Deleted,
    Offline,
    Held,
    HeldMalformed,
}
impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Malformed => "malformed",
            Self::WrongId => "wrong-id",
            Self::Unsupported => "unsupported",
            Self::Deleted => "deleted",
            Self::Offline => "offline",
            Self::Held => "held",
            Self::HeldMalformed => "held-malformed",
        }
    }
}
pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("workflow-activity" | "workflow-activity-narrow")
        )
        && matches!(
            case().as_str(),
            "file"
                | "file-retirement"
                | "file-emoji"
                | "file-emoji-retirement"
                | "file-emoji-cold"
                | "file-encoded"
                | "file-recovery"
                | "file-recovery-retirement"
                | "mixed-navigation"
                | "mixed-navigation-reverse"
                | "mixed-navigation-success"
        )
}
pub(crate) fn recovery_requested() -> bool {
    requested()
        && matches!(
            case().as_str(),
            "file-recovery" | "file-recovery-retirement"
        )
}
#[cfg(not(any(test, feature = "protected-native-fixture")))]
pub(super) fn install(_: &mut ShellState) {}
#[cfg(any(test, feature = "protected-native-fixture"))]
pub(super) fn install(s: &mut ShellState) {
    // Initial synthetic fixture setup only, before any draft or native lease.
    // The ordinary installer supplies the real owners; recapture against the
    // disconnected transport so this new case has no HTTP capability.
    s.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    s.refresh_from_core();
    s.channel_tab = crate::model::ChannelTab::AppHome;
    let home = crate::app_home::Owner::capture(s).unwrap();
    let h = s.app_home.surface.as_mut().unwrap();
    h.id = home.home_visit;
    h.owner = home;
    s.channel_tab = crate::model::ChannelTab::AppAbout;
    let about = crate::app_home::about::Owner::capture(s).unwrap();
    s.app_home.about.surface.as_mut().unwrap().owner = about;
    let data = Data {
        account: s.core.active_account.clone(),
        epoch: s.core.account_epoch,
        credentials: serde_json::to_string(s.core.session.as_ref().unwrap()).unwrap(),
        transport: s.core.transport.clone().unwrap(),
        requests: vec![],
        settlements: vec![],
        releases: vec![],
        mode: Mode::Success,
        metadata: recovery::Plan::new(&case()),
        held: None,
    };
    s.app_home
        .about
        .management
        .activity_fixture
        .as_ref()
        .unwrap()
        .0
        .lock()
        .unwrap()
        .file = Some(Model(Arc::new(Mutex::new(data))));
}
fn closed() -> Error {
    Error::Offline("Closed Activity file reader unavailable".into())
}
fn model(s: &ShellState) -> Result<Model, Error> {
    if !requested()
        || s.core.active_account.as_deref() != Some(ACCOUNT)
        || s.core.active_team.as_deref() != Some("TWORKFLOW")
        || s.self_account.user_id != "UWORKFLOW"
        || s.core
            .workspaces
            .get("TWORKFLOW")
            .is_none_or(|w| w.self_user_id != "UWORKFLOW")
    {
        return Err(closed());
    }
    let model = s
        .app_home
        .about
        .management
        .activity_fixture
        .as_ref()
        .and_then(|m| m.0.lock().unwrap().file.clone())
        .ok_or_else(closed)?;
    let m = model.0.lock().unwrap();
    if s.core.active_account != m.account
        || s.core.account_epoch != m.epoch
        || s.core
            .session
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok())
            .as_ref()
            != Some(&m.credentials)
        || s.core
            .transport
            .as_ref()
            .is_none_or(|t| !Arc::ptr_eq(t, &m.transport) || t.http().is_ok())
    {
        return Err(closed());
    }
    drop(m);
    Ok(model)
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    model(s).is_ok()
}
pub(crate) fn messages(row: &mut Value) {
    row["trigger_message"] = json!({"template":"Open {list} then {list}",
        "variables":{"list":{"type":"LIST","value":LIST}}});
    row["step_events"][0]["outcome_message"] = json!({"template":"Read {canvas}",
        "variables":{"canvas":{"type":"CANVAS","value":CANVAS}}});
}
pub(crate) fn raw(id: &str) -> Result<&'static str, Error> {
    if encoded_requested() {
        match id {
            LIST => Ok(include_str!("file/encoded-list.json")),
            CANVAS => Ok(include_str!("file/encoded-canvas.json")),
            _ => Err(closed()),
        }
    } else if cold_requested() && id == LIST {
        Ok(include_str!("file/emoji-cold-list.json"))
    } else {
        original(id, emoji_requested())
    }
}
pub(crate) fn file() -> Value {
    serde_json::from_str::<Value>(raw(LIST).unwrap()).unwrap()["file"].clone()
}
fn response(id: &str, mode: Mode) -> Result<String, Error> {
    let raw = raw(id)?;
    Ok(match mode {
        Mode::Malformed | Mode::HeldMalformed => r#"{"ok":true,"file":false}"#.into(),
        Mode::WrongId => raw.replace(LIST, "FFOREIGN01"),
        Mode::Unsupported => raw
            .replace("\"mode\":\"list\"", "\"mode\":\"hosted\"")
            .replace("\"filetype\":\"list\"", "\"filetype\":\"pdf\""),
        Mode::Deleted => raw.replace("\"is_deleted\":false", "\"is_deleted\":true"),
        _ => raw.into(),
    })
}
pub(crate) fn mode(s: &ShellState, mode: Mode) -> Result<(), String> {
    if matches!(mode, Mode::HeldMalformed) && !super::mixed::is_case(&case()) {
        return Err("Held malformed file response requires the exact mixed case".into());
    }
    let model = model(s).map_err(|_| "File fixture unavailable")?;
    let mut m = model.0.lock().unwrap();
    if m.held.is_some() {
        return Err("Original file read is held".into());
    }
    m.mode = mode;
    Ok(())
}
fn request_id(s: &ShellState, request: &PreparedRequest) -> Result<String, Error> {
    let RequestBody::Form(fields) = &request.body else {
        return Err(closed());
    };
    let id = fields
        .iter()
        .find(|(k, _)| k == "file")
        .map(|(_, v)| v.as_str())
        .ok_or_else(closed)?;
    raw(id)?;
    let session = &s.core.session.as_ref().ok_or_else(closed)?.workspaces["TWORKFLOW"];
    let expected = api::file_info(&s.core.client, session, id);
    // Real factories regenerate trace UUIDs. Endpoint and complete ordered form,
    // including credential, remain exact. Reject unknown/duplicate query keys.
    if request.method != expected.method
        || request.body != expected.body
        || request.headers != expected.headers
        || !same_url(&request.url, &expected.url)
    {
        return Err(closed());
    }
    Ok(id.into())
}
fn same_url(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Option<(String, std::collections::BTreeMap<String, String>)> {
        let url = url::Url::parse(v).ok()?;
        let mut query = std::collections::BTreeMap::new();
        for (k, v) in url.query_pairs() {
            let value = if matches!(k.as_ref(), "_x_id" | "_x_csid" | "fp") {
                if v.len() != 32
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || uuid::Uuid::parse_str(&v).ok()?.get_version_num() != 4
                {
                    return None;
                }
                "trace".into()
            } else {
                v.into_owned()
            };
            if query.insert(k.into_owned(), value).is_some() {
                return None;
            }
        }
        if url.fragment().is_some() {
            return None;
        }
        Some((v.split('?').next()?.into(), query))
    };
    parse(a).zip(parse(b)).is_some_and(|(a, b)| a == b)
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
    purpose: &str,
) -> Result<OrderedFileInfo, Error> {
    let (model, id) = {
        let s = state.peek();
        (model(&s)?, request_id(&s, request)?)
    };
    if !matches!(purpose, "metadata" | "navigation")
        || purpose == "navigation" && (id != LIST || recovery_requested())
    {
        return Err(closed());
    }
    let (serial, mode, bytes, held) = {
        let mut m = model.0.lock().unwrap();
        if m.held.is_some() {
            return Err(closed());
        }
        let mode = if purpose == "navigation" {
            std::mem::take(&mut m.mode)
        } else {
            m.metadata.next(&id)
        };
        let bytes = response(&id, mode)?;
        let serial = m.requests.len() + 1;
        let RequestBody::Form(fields) = &request.body else {
            return Err(closed());
        };
        m.requests.push(json!({"serial":serial,"purpose":purpose,"method":"files.info",
            "httpMethod":request.method,"url":request.url,
            "args":fields.iter().filter(|(k,_)|k!="token").map(|(k,v)|(k.clone(),json!(v))).collect::<serde_json::Map<_,_>>(),
            "fields":fields.iter().filter(|(k,_)|k!="token").collect::<Vec<_>>(),
            "id":id,"mode":mode.label(),"responseText":if matches!(mode,Mode::Offline){Value::Null}else{json!(bytes)},
            "raw":if matches!(mode,Mode::Offline){Value::Null}else{serde_json::from_str::<Value>(&bytes).unwrap()}}));
        let held = if matches!(mode, Mode::Held | Mode::HeldMalformed) {
            let (send, recv) = oneshot::channel();
            m.held = Some((serial, send));
            Some(recv)
        } else {
            None
        };
        (serial, mode, bytes, held)
    };
    struct Finish {
        model: Model,
        serial: usize,
        returned: bool,
    }
    impl Drop for Finish {
        fn drop(&mut self) {
            self.model.0.lock().unwrap().settlements.push(
                json!({"serial":self.serial,"returned":self.returned,"cancelled":!self.returned}),
            );
        }
    }
    let mut finish = Finish {
        model,
        serial,
        returned: false,
    };
    if let Some(receiver) = held {
        tokio::time::timeout(Duration::from_secs(45), receiver)
            .await
            .map_err(|_| closed())?
            .map_err(|_| closed())?;
    }
    finish.returned = true;
    if matches!(mode, Mode::Offline) {
        return Err(Error::Offline("Synthetic file read unavailable".into()));
    }
    OrderedFileInfo::decode(bytes.as_bytes())
}
pub(crate) fn release(s: &ShellState) -> Result<Value, String> {
    let model = model(s).map_err(|_| "File fixture unavailable")?;
    let mut m = model.0.lock().unwrap();
    let (serial, sender) = m.held.take().ok_or("No original held file read")?;
    let receiver_retired = sender.is_closed();
    let result = json!({"serial":serial,"receiverRetired":receiver_retired,"delivered":sender.send(()).is_ok()});
    m.releases.push(result.clone());
    Ok(result)
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let Ok(model) = model(s) else {
        return Value::Null;
    };
    let m = model.0.lock().unwrap();
    json!({"requests":m.requests,"settlements":m.settlements,"releases":m.releases,
        "completed":m.settlements.len(),"cancelled":m.settlements.iter().filter(|v|v["cancelled"]==true).count(),
        "held":m.held.as_ref().map(|(n,_)|n),"networkCalls":0,"httpCapability":false,
        "originals":{"list":serde_json::from_str::<Value>(raw(LIST).unwrap()).unwrap(),"canvas":serde_json::from_str::<Value>(raw(CANVAS).unwrap()).unwrap()}})
}

pub(crate) fn custody(s: &ShellState) -> Value {
    json!({"documents":s.app_home.about.management.documents.iter().map(|d|json!({
        "id":d.id,"revision":d.revision,"title":d.title,"description":d.description,
        "dirty":d.dirty(),"pending":d.pending,"workflow":d.editor.raw_workflow(),
        "trigger":d.editor.trigger(),"preferences":d.editor.external_preferences(),
        "copyPermissions":d.editor.copy_permissions(),"runPermissions":d.editor.run_permissions(),
        "restriction":d.editor.restriction()
    })).collect::<Vec<_>>(),
    "account":s.core.active_account,"epoch":s.core.account_epoch,
    "channel":s.core.active_channel,"channelGeneration":s.channel_generation})
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod emoji_contract {
    use super::*;

    #[test]
    fn encoded_envelopes_change_only_original_title_and_do_not_enable_emoji_seed() {
        for (id, bytes, title) in [
            (
                LIST,
                include_bytes!("file/encoded-list.json").as_slice(),
                "R\u{e9}vision &amp;lt;b&amp;gt; &amp;amp;lt;",
            ),
            (
                CANVAS,
                include_bytes!("file/encoded-canvas.json").as_slice(),
                "Canvas &amp;lt;i&amp;gt; &amp;amp;lt;",
            ),
        ] {
            let original =
                OrderedFileInfo::decode(original(id, false).unwrap().as_bytes()).unwrap();
            let encoded = OrderedFileInfo::decode(bytes).unwrap();
            let mut expected = original.raw().clone();
            expected["file"]["title"] = json!(title);
            assert_eq!(&expected, encoded.raw());
        }
        assert!(!emoji_case("file-encoded"));
    }

    #[test]
    fn cold_header_envelope_changes_only_the_original_alias_in_title() {
        let ready = OrderedFileInfo::decode(original(LIST, true).unwrap().as_bytes()).unwrap();
        let cold = OrderedFileInfo::decode(include_bytes!("file/emoji-cold-list.json")).unwrap();
        let mut expected = ready.raw().clone();
        expected["file"]["title"] = json!(
            ready.file()["title"]
                .as_str()
                .unwrap()
                .replace(":activity_ready_alias:", ":activity_cold:")
        );
        assert_eq!(&expected, cold.raw());
        assert_eq!(cold.file()["id"], LIST);
        assert!(cold.file()["title"].as_str().unwrap().contains("&amp;"));
        assert!(
            !cold.file()["title"]
                .as_str()
                .unwrap()
                .contains(":activity_ready_alias:")
        );
    }

    #[test]
    fn emoji_variants_preserve_every_original_wire_field_except_title() {
        for id in [LIST, CANVAS] {
            let old = OrderedFileInfo::decode(original(id, false).unwrap().as_bytes()).unwrap();
            let new = OrderedFileInfo::decode(original(id, true).unwrap().as_bytes()).unwrap();
            let mut expected = old.raw().clone();
            expected["file"]["title"] = new.file()["title"].clone();
            assert_eq!(&expected, new.raw());
            let title = new.file()["title"].as_str().unwrap();
            assert!(title.contains("&amp;"));
            assert!(!title.contains("&amp;amp;"));
            assert!(title.contains(":wave:"));
            assert!(title.contains(":unknown:"));
            assert_eq!(new.file()["id"], id);
        }
        assert!(original("FFOREIGN01", true).is_err());
    }

    #[test]
    fn emoji_cases_are_exact_and_do_not_admit_recovery_or_ordinary_variants() {
        assert!(emoji_case("file-emoji"));
        assert!(emoji_case("file-emoji-retirement"));
        assert!(emoji_case("file-emoji-cold"));
        for value in [
            "file",
            "file-retirement",
            "file-recovery",
            "file-emoji-foreign",
            "",
        ] {
            assert!(!emoji_case(value));
        }
    }

    #[test]
    fn named_emoji_seed_uses_real_alias_and_same_ready_asset_without_http() {
        use super_platinum_core::state::{EmojiSource, custom_emoji_source};
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        super::super::install_case(&mut shell, "file-emoji");
        let workspace = &shell.core.workspaces["TWORKFLOW"];
        let ready = "https://fixture.invalid/activity-ready.png";
        assert!(matches!(
            custom_emoji_source(&workspace.custom_emoji, "activity_ready_alias"),
            Some(EmojiSource::Url(url)) if url == ready
        ));
        assert!(matches!(
            custom_emoji_source(&workspace.custom_emoji, "wave"),
            Some(EmojiSource::Url(url)) if url == ready
        ));
        let alias = shell.media.register_emoji("activity_ready_alias", ready);
        assert!(shell.media.is_ready(&alias));
        let cold = shell
            .media
            .register_emoji("activity_cold", "https://fixture.invalid/activity-cold.png");
        assert!(!shell.media.is_ready(&cold));
        assert!(shell.core.transport.as_ref().unwrap().http().is_err());
        assert!(shell.slack_lists.fixture.is_none());
    }
}
