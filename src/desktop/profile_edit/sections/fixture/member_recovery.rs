//! Closed absent-member transport. Production Read/Page and private adoption run
//! unchanged; only named synthetic request callbacks are held here.
use super::*;
use crate::directory_member_fetches::Context;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::client::{PreparedRequest, RequestBody};
use tokio::sync::oneshot;

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MEMBER_RECOVERY_CHECK";
pub(crate) const MEMBER: &str = "UMISSING01";
pub(crate) const LABEL: &str = "  Cafe\u{0301} Harbor  ";
pub(crate) const DRAFT: &str = "Keep my unsaved description with ";
type Reply = Result<(Value, Option<Vec<String>>), Error>;
#[derive(Default)]
struct Ledger {
    serial: u64,
    requests: Vec<Value>,
    held: Vec<(u64, oneshot::Sender<Reply>)>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Mutex::default)
}
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("profile-member-recovery" | "profile-member-recovery-narrow")
        )
}
pub(crate) fn enabled() -> bool {
    if (super::user_title::named() || super::completion::title_users::named()) {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && (named()
            || super::member_known::named()
            || super::group_recovery::mixed_absent::enabled()
            || (super::member_freshness::named()
                && std::env::var(super::member_freshness::FLAG).as_deref() == Ok("1")))
        && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_RECOVERY_CASE")
        .unwrap_or_else(|_| "recovery".into())
}
pub(crate) fn blocks(text: &str) -> Vec<Value> {
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":text},
            {"type":"user","user_id":MEMBER,"fixture_occurrence":1,"opaque":{"keep":true}},
            {"type":"user","user_id":MEMBER,"fixture_occurrence":2,"opaque":{"keep":true}}
        ]}]}),
    ]
}
pub(crate) fn install(shell: &mut ShellState) {
    mentions::install(shell);
    mentions::set_blocks(shell, blocks("Original description with "));
    let d = shell.profile_edit.sections.draft.as_ref().unwrap();
    let (visit, serial) = (d.visit, d.serial);
    assert!(change_shell(
        shell,
        visit,
        serial,
        "long",
        SectionValue::LongText(blocks(DRAFT))
    ));
}
/// Initial source only: self is present, the requested target has never existed.
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !enabled() {
        return Err("Closed member recovery required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing fixture owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Member recovery"},"self":user,"users":[user],
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Member recovery"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await
}
async fn hold(
    state: Signal<ShellState>,
    context: &Context,
    request: &PreparedRequest,
    kind: &str,
) -> Reply {
    if !enabled() || !context.current(&state.peek()) {
        return Err(Error::Offline("closed member fixture".into()));
    }
    let body = match &request.body {
        RequestBody::Form(pairs) => pairs
            .iter()
            .filter(|(k, _)| matches!(k.as_str(), "include_external_workspaces" | "_x_reason"))
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        RequestBody::Json(value) => {
            let mut body = value.clone();
            let map = body
                .as_object_mut()
                .ok_or_else(|| Error::Api("fixture_request_shape".into()))?;
            map.remove("token");
            map.remove("enterprise_token");
            body
        }
    };
    let method = if kind == "discovery" {
        "api.getFlannelHttpUrl"
    } else {
        "users/info"
    };
    if request.method != "POST" || !request.url.contains(method) {
        return Err(Error::Api("fixture_request_route".into()));
    }
    let (send, receive) = oneshot::channel();
    {
        let mut book = ledger().lock().unwrap();
        book.serial += 1;
        let serial = book.serial;
        book.requests.push(json!({"serial":serial,"kind":kind,"method":method,"body":body,
            "account":context.account,"team":context.session.team_id,"epoch":context.epoch,"released":false}));
        book.held.push((serial, send));
    }
    receive
        .await
        .map_err(|_| Error::Offline("closed member fixture receiver retired".into()))?
}
pub(crate) async fn discovery(
    state: Signal<ShellState>,
    context: &Context,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    hold(state, context, request, "discovery")
        .await
        .map(|v| v.0)
}
pub(crate) async fn member(
    state: Signal<ShellState>,
    context: &Context,
    request: &PreparedRequest,
) -> Reply {
    hold(state, context, request, "member").await
}
pub(crate) fn requests() -> Vec<Value> {
    ledger().lock().unwrap().requests.clone()
}
pub(crate) fn pending() -> Vec<Value> {
    let book = ledger().lock().unwrap();
    book.requests
        .iter()
        .filter(|r| book.held.iter().any(|(n, _)| r["serial"] == *n))
        .cloned()
        .collect()
}
pub(crate) fn release(serial: u64, response: Reply) -> Result<(), String> {
    let observed = release_observed(serial, response)?;
    if observed["delivered"] == true {
        Ok(())
    } else {
        Err("Closed receiver retired before response".into())
    }
}
pub(crate) fn release_observed(serial: u64, response: Reply) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed callback required".into());
    }
    let sender = {
        let mut book = ledger().lock().unwrap();
        let i = book
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Missing held callback")?;
        let outcome = if response.is_ok() {
            "response"
        } else {
            "offline"
        };
        let row = book
            .requests
            .iter_mut()
            .find(|r| r["serial"] == serial)
            .unwrap();
        row["released"] = json!(true);
        row["outcome"] = json!(outcome);
        if super::group_recovery::mixed_absent::enabled()
            || (super::user_title::named() || super::completion::title_users::named())
        {
            row["response"] = match &response {
                Ok((raw, disconnected)) => json!({"raw":raw,"disconnected":disconnected}),
                Err(Error::Offline(_)) => json!({"error":"offline"}),
                Err(_) => json!({"error":"other"}),
            };
        }
        book.held.remove(i).1
    };
    let retired = sender.is_closed();
    let delivered = sender.send(response).is_ok();
    let observed = json!({"serial":serial,"receiverRetired":retired,"delivered":delivered});
    if super::group_recovery::mixed_absent::enabled()
        || (super::user_title::named() || super::completion::title_users::named())
    {
        let mut book = ledger().lock().unwrap();
        let row = book
            .requests
            .iter_mut()
            .find(|row| row["serial"] == serial)
            .unwrap();
        row["receiverRetired"] = json!(retired);
        row["delivered"] = json!(delivered);
    }
    Ok(observed)
}
pub(crate) fn success(team: &str) -> Value {
    json!({"ok":true,"results":[{"id":MEMBER,"team_id":team,"name":"username-must-not-win",
        "real_name":"Real fallback","profile":{"display_name":LABEL,"real_name":"Profile fallback"},"updated":101}],
        "can_interact":{(MEMBER):true}})
}
pub(crate) fn invalidate(state: &mut ShellState) {
    let team = state.core.active_team.clone().unwrap();
    state
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .directory_members
        .invalidate(MEMBER);
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let native = shell
        .core
        .workspaces
        .get(team)
        .and_then(|w| w.users.get(MEMBER));
    let source = shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(team))
        .and_then(|session| {
            let w = shell.core.workspaces.get(team)?;
            let account = shell.core.active_account.as_deref()?;
            w.directory_members
                .observations(
                    account,
                    session,
                    shell.core.account_epoch,
                    "fixture-member-recovery",
                )
                .ok()
        });
    json!({"profile":mentions::snapshot(shell),"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "sourcePresent":source.as_ref().is_some_and(|o|o.source_ids().any(|id|id==MEMBER)),
        "sourceObserved":source.is_some(),"native":native,"requests":requests(),"pending":pending(),
        "driver":crate::directory_member_fetches::driver::fixture_status(shell)})
}
pub(crate) fn replace_account(mut state: Signal<ShellState>) -> Value {
    let (before, media, epoch) = {
        let s = state.peek();
        (snapshot(&s), s.media.clone(), s.core.account_epoch)
    };
    let mut loaded = ShellState::fixture(media);
    crate::app_home::channel_fetches::fixture::install(&mut loaded);
    loaded.core.account_epoch = epoch + 1;
    loaded.core.active_account = Some("00000000-0000-4000-8000-000000000082".into());
    crate::bootstrap::install_loaded(state, loaded);
    let after = snapshot(&state.write());
    json!({"before":before,"after":after,"actualInstallLoaded":true})
}
