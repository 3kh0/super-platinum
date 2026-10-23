//! Exact opt-in List Canvas-title fixtures. No client capable of HTTP is installed.
use super::*;
use crate::directory_member_fetches::Context;
use std::sync::OnceLock;
use super_platinum_core::block_actions::{
    directory_endpoint::{DirectoryEndpoint, member_info::Read},
    name_policy,
};

pub(crate) const FLAG: &str = "SUPER_PLATINUM_LIST_CANVAS_USER_TITLE_CHECK";
pub(crate) const MEMBER: &str = "UMISSING01";
pub(crate) const LABEL: &str = "  Cafe\u{0301} Harbor  ";
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default()
}
pub(crate) fn own_edit_named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_USER_TITLE_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "own-edit-title-known"
                | "own-edit-title-hidden"
                | "own-edit-title-recovery"
                | "own-edit-title-held-cancel"
        )
}
pub(crate) fn named() -> bool {
    if own_edit_named() {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "user-title-known"
                | "user-title-hidden"
                | "user-title-recovery"
                | "user-title-draft-recovery"
                | "user-title-retirement"
        )
}
pub(crate) fn label() -> &'static str {
    if super::link_title::named() {
        return super::link_title::LABEL;
    }
    if super::empty_canvas::named() {
        return super::empty_canvas::LABEL;
    }
    if matches!(
        case().as_str(),
        "user-title-hidden" | "own-edit-title-hidden"
    ) {
        "Review with Hidden user / Hidden user &amp; plan"
    } else {
        "Review with @  Cafe\u{0301} Harbor   / @  Cafe\u{0301} Harbor   &amp; plan"
    }
}
pub(crate) fn title_blocks() -> Value {
    json!([{"type":"rich_text","block_id":"file-title-block","opaque_block":{"retain":["block",7]},"elements":[{
    "type":"rich_text_section","opaque_section":{"retain":true},"elements":[
        {"type":"text","text":"Review with ","opaque_run":{"index":0}},
        {"type":"user","user_id":MEMBER,"style":{"italic":true},"opaque_run":{"index":1}},
        {"type":"text","text":" / ","opaque_run":{"index":2}},
        {"type":"user","user_id":MEMBER,"style":{"bold":true},"opaque_run":{"index":3}},
        {"type":"text","text":" &amp; plan","opaque_run":{"index":4}}
    ]}]}])
}
pub(crate) fn file_response() -> String {
    // Insert into the original bytes; never roundtrip the ordered channel_map.
    files::RESPONSE.replacen(
        "\"title\":\"Alex Canvas\",",
        &format!(
            "\"title\":\"Alex Canvas\",\"title_blocks\":{},",
            title_blocks()
        ),
        1,
    )
}
pub(crate) fn policy() -> Value {
    json!({"ok":true,"prefs":{"frecency":"{}","display_real_names_override":1,"hidden_users":if matches!(case().as_str(), "user-title-hidden" | "own-edit-title-hidden"){MEMBER}else{""}},"opaque_policy":"list-original-complete-policy"})
}
pub(crate) fn success() -> Value {
    json!({"ok":true,"results":[{"id":MEMBER,"team_id":"T1","name":"username-must-not-win","real_name":"Real fallback",
    "profile":{"display_name":LABEL,"real_name":"Profile fallback"},"updated":101}],"can_interact":{(MEMBER):true}})
}
pub(super) fn seed_boot(boot: &mut Value) {
    if named()
        && matches!(
            case().as_str(),
            "user-title-known"
                | "user-title-hidden"
                | "user-title-draft-recovery"
                | "own-edit-title-known"
                | "own-edit-title-hidden"
                | "own-edit-title-recovery"
                | "own-edit-title-held-cancel"
        )
    {
        boot["users"]
            .as_array_mut()
            .unwrap()
            .push(success()["results"][0].clone());
    }
}
type MemberReply = Result<Reply, Error>;
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    policy: Vec<Value>,
    files: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<MemberReply>)>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static VALUE: OnceLock<Mutex<Ledger>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
fn admit(shell: &ShellState, ctx: &Context) -> Result<Model, Error> {
    let model = handle(shell)?;
    if !named()
        || !ctx.current(shell)
        || Some(&ctx.account) != model.account.as_ref()
        || ctx.epoch != model.epoch
        || !crate::emoji_preferences::same_session(&ctx.session, &model.session)
        || !Arc::ptr_eq(&ctx.transport, &model.transport)
    {
        return Err(refused());
    }
    Ok(model)
}
pub(crate) async fn policy_read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if !named() {
        return None;
    }
    Some((|| {
        let s = state.peek();
        let m = handle(&s)?;
        let expected = name_policy::request::user(&s.core.client, &m.session);
        if !protocol::same_request(request, &expected) {
            return Err(refused());
        }
        let raw = policy();
        let mut l = ledger().lock().unwrap();
        let serial = l.policy.len() + 1;
        l.policy.push(json!({"serial":serial,"method":"users.prefs.get","raw":raw,"account":m.account,"epoch":m.epoch,"team":m.session.team_id,"user":m.session.user_id}));
        Ok(raw)
    })())
}
async fn hold(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    expected: &PreparedRequest,
    kind: &str,
    lane: &str,
) -> MemberReply {
    admit(&state.peek(), ctx)?;
    if !protocol::same_request(request, expected) {
        return Err(refused());
    }
    let mut args = match &request.body {
        super_platinum_core::slack::client::RequestBody::Json(v) => v.clone(),
        super_platinum_core::slack::client::RequestBody::Form(v) => {
            Value::Object(v.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
        }
    };
    args.as_object_mut().ok_or_else(refused)?.remove("token");
    args.as_object_mut().unwrap().remove("enterprise_token");
    let (tx, rx) = oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let serial = l.requests.len() + 1;
        l.requests.push(json!({"serial":serial,"kind":kind,"lane":lane,"method":if kind=="discovery"{"api.getFlannelHttpUrl"}else{"users/info"},"args":args,"released":false,"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id}));
        l.held.push((serial, tx));
    }
    rx.await
        .map_err(|_| Error::Offline("Closed List member receiver retired".into()))?
}
pub(crate) fn discovery_raw(shell: &ShellState) -> Result<Value, Error> {
    Ok(protocol::discovery(&handle(shell)?.session))
}
pub(crate) async fn discovery(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    discovery_lane(state, ctx, request, "absent").await
}
pub(crate) async fn known_discovery(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    discovery_lane(state, ctx, request, "known").await
}
async fn discovery_lane(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    lane: &str,
) -> Result<Value, Error> {
    let expected = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
    hold(state, ctx, request, &expected, "discovery", lane)
        .await
        .map(|v| v.0)
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    read: &Read,
) -> MemberReply {
    member_lane(state, ctx, request, read, "absent").await
}
pub(crate) async fn known_member(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    read: &Read,
) -> MemberReply {
    member_lane(state, ctx, request, read, "known").await
}
async fn member_lane(
    state: Signal<ShellState>,
    ctx: &Context,
    request: &PreparedRequest,
    read: &Read,
    lane: &str,
) -> MemberReply {
    if read.ids() != [MEMBER.to_owned()] {
        return Err(refused());
    }
    let now = Instant::now();
    let endpoint =
        DirectoryEndpoint::decode(&ctx.session, &protocol::discovery(&ctx.session), now)?;
    let expected = endpoint.member_info(&ctx.client, &ctx.session, now, read)?;
    hold(state, ctx, request, &expected, "member", lane).await
}
pub(crate) fn pending() -> Vec<Value> {
    let l = ledger().lock().unwrap();
    l.requests
        .iter()
        .filter(|r| l.held.iter().any(|(n, _)| r["serial"] == *n))
        .cloned()
        .collect()
}
pub(crate) fn release(serial: usize, response: MemberReply) -> Result<Value, String> {
    if !named() {
        return Err("Closed List title fixture required".into());
    }
    let tx = {
        let mut l = ledger().lock().unwrap();
        let i = l
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Missing original callback")?;
        let row = l
            .requests
            .iter_mut()
            .find(|r| r["serial"] == serial)
            .unwrap();
        row["released"] = json!(true);
        row["response"] = match &response {
            Ok((raw, keys)) => json!({"raw":raw,"keys":keys}),
            Err(Error::Offline(_)) => json!({"error":"offline"}),
            Err(_) => json!({"error":"other"}),
        };
        l.held.remove(i).1
    };
    let retired = tx.is_closed();
    let delivered = tx.send(response).is_ok();
    let proof = json!({"serial":serial,"receiverRetired":retired,"delivered":delivered});
    let mut l = ledger().lock().unwrap();
    let row = l
        .requests
        .iter_mut()
        .find(|r| r["serial"] == serial)
        .unwrap();
    row["disposition"] = proof.clone();
    Ok(proof)
}
pub(crate) fn invalidate(shell: &mut ShellState) -> Result<(), Error> {
    let m = handle(shell)?;
    if !named() {
        return Err(refused());
    }
    shell
        .core
        .workspaces
        .get_mut(&m.session.team_id)
        .ok_or_else(refused)?
        .directory_members
        .invalidate(MEMBER);
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let source = handle(shell).ok().and_then(|m| {
        let w = shell.core.workspaces.get(&m.session.team_id)?;
        w.directory_members
            .observations(
                m.account.as_deref()?,
                &m.session,
                m.epoch,
                "list-title-fixture",
            )
            .ok()
    });
    let source_raw = source.as_ref().and_then(|o| {
        let m = handle(shell).ok()?;
        let w = shell.core.workspaces.get(&m.session.team_id)?;
        o.rows(&w.directory_members, &[MEMBER.to_owned()])
            .ok()?
            .first()
            .map(|row| row.source.clone())
    });
    let source_ready = handle(shell).ok().is_some_and(|m| {
        shell
            .core
            .workspaces
            .get(&m.session.team_id)
            .is_some_and(|w| {
                m.account.as_deref().is_some_and(|account| {
                    w.directory_members
                        .ready(account, &m.session, m.epoch, "list-title-fixture", &w.users)
                        .is_ok()
                })
            })
    });
    let (requests, policy, files) = {
        let l = ledger().lock().unwrap();
        (l.requests.clone(), l.policy.clone(), l.files.clone())
    };
    json!({"case":case(),"fileResponseBytes":file_response(),"requests":requests,"policy":policy,"files":files,"pending":pending(),
    "sourceRaw":source_raw,"sourceReady":source_ready,"knownDriver":crate::directory_member_fetches::known::fixture_status(shell),
    "sourcePresent":source.as_ref().is_some_and(|o|o.source_ids().any(|id|id==MEMBER)),"sourceObserved":source.is_some(),
    "native":shell.core.workspaces.get("T1").and_then(|w|w.users.get(MEMBER)),"driver":crate::directory_member_fetches::driver::fixture_status(shell)})
}

pub(crate) async fn file_read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo, Error>
{
    let s = state.peek();
    let m = handle(&s)?;
    if !named() {
        return Err(refused());
    }
    let super_platinum_core::slack::client::RequestBody::Form(body) = &request.body else {
        return Err(refused());
    };
    let id = body
        .iter()
        .find(|(k, _)| k == "file")
        .map(|(_, v)| v.as_str())
        .filter(|id| matches!(*id, files::CANVAS | files::LIST))
        .ok_or_else(refused)?;
    let expected = s.core.client.rest_form(
        &m.session,
        "files.info",
        vec![
            ("file", id.into()),
            ("page", "1".into()),
            ("count", "500".into()),
            ("truncate", "true".into()),
        ],
    );
    if !protocol::same_request(request, &expected) {
        return Err(refused());
    }
    let bytes = file_response();
    let line = bytes
        .lines()
        .find(|line| line.contains(&format!("\"id\":\"{id}\"")))
        .ok_or_else(refused)?
        .trim()
        .trim_end_matches(',');
    let text = format!("{{\"ok\":true,\"file\":{line}}}");
    let raw: Value = serde_json::from_str(&text).map_err(|_| refused())?;
    {
        let mut l = ledger().lock().unwrap();
        let serial = l.files.len() + 1;
        l.files.push(
            json!({"serial":serial,"method":"files.info","id":id,"raw":raw,"responseText":text}),
        );
    }
    super_platinum_core::slack::api::custom::texty::file_title_source::OrderedFileInfo::decode(
        text.as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_title_fixture_changes_only_canvas_title_blocks_and_preserves_order() {
        use super_platinum_core::slack::api::custom::texty::{
            file_title_source::Store, files::OrderedResponse,
        };
        let original = OrderedResponse::decode(files::RESPONSE.as_bytes()).unwrap();
        let received = OrderedResponse::decode(file_response().as_bytes()).unwrap();
        assert_eq!(&received.rows()[1..], &original.rows()[1..]);
        let mut canvas = received.rows()[0].clone();
        assert_eq!(
            canvas.as_object_mut().unwrap().remove("title_blocks"),
            Some(title_blocks())
        );
        assert_eq!(canvas, original.rows()[0]);
        let owner = WorkspaceSession {
            team_id: "T1".into(),
            user_id: SELF.into(),
            ..super::super::super::session()
        };
        let mut store = Store::new();
        store
            .ingest_batch_ordered(
                received.raw(),
                received.rows(),
                &owner,
                received.channel_map_order(),
            )
            .unwrap();
        let mut original_store = Store::new();
        original_store
            .ingest_batch_ordered(
                original.raw(),
                original.rows(),
                &owner,
                original.channel_map_order(),
            )
            .unwrap();
        for id in [files::CANVAS, files::LIST] {
            assert_eq!(
                store.candidate(id).unwrap().channel_map_keys(),
                original_store.candidate(id).unwrap().channel_map_keys()
            );
        }
        assert_eq!(
            store
                .candidate(files::CANVAS)
                .unwrap()
                .channel_map_keys()
                .unwrap(),
            ["z-last", "a-first", "m-foreign"]
        );
        assert_eq!(
            store.candidate(files::CANVAS).unwrap().effective()["title"],
            "Alex Canvas"
        );
    }
    #[test]
    fn list_title_fixture_two_leaves_are_one_distinct_dependency() {
        let ids = super_platinum_core::slack::api::custom::texty::file_titles::user_ids(
            &json!({"title_blocks":title_blocks()}),
        )
        .unwrap();
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![MEMBER]);
        assert_eq!(
            title_blocks()[0]["elements"][0]["elements"][1]["style"],
            json!({"italic":true})
        );
        assert_eq!(
            title_blocks()[0]["elements"][0]["elements"][3]["style"],
            json!({"bold":true})
        );
    }
}
