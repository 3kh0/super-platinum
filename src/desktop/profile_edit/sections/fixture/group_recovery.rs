//! Named closed transport for automatic absent groups and explicit Retry.
//! Replies are raw API bodies;
//! the production decoder and owner/source admission remain responsible for use.
use super::*;
use crate::directory_member_fetches::Context;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::client::{PreparedRequest, RequestBody};
use tokio::sync::oneshot;
pub(crate) mod known_freshness;
mod mixed;
pub(crate) mod mixed_absent;
pub(crate) use mixed::{MEMBER, MEMBER_LABEL};

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_CHECK";
pub(crate) const GROUP: &str = "SMISSING01";
pub(crate) const OTHER: &str = "SMISSING02";
pub(crate) const LABEL: &str = "@@ Cafe\u{0301} crew  ";
pub(crate) const OTHER_LABEL: &str = "@Review crew";
pub(crate) const DRAFT: &str = "Keep my unsaved description with ";
pub(crate) const EDITED: &str = "Native edited \u{4e2d}\u{1f98a} & description with ";
type Reply = Result<Value, Error>;
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
            Ok("profile-group-recovery" | "profile-group-recovery-narrow")
        )
}
pub(crate) fn enabled() -> bool {
    if super::group_title::named() {
        return true;
    }
    cfg!(any(test, feature = "protected-native-fixture"))
        && named()
        && std::env::var(FLAG).as_deref() == Ok("1")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_GROUP_RECOVERY_CASE").unwrap_or_else(|_| "legacy".into())
}
pub(crate) fn ids() -> Vec<String> {
    if case() != "legacy" {
        vec![GROUP.into(), OTHER.into()]
    } else {
        vec![GROUP.into()]
    }
}
pub(crate) fn blocks(text: &str) -> Vec<Value> {
    let mut elements = vec![json!({"type":"text","text":text})];
    for (occurrence, id) in [GROUP, GROUP]
        .into_iter()
        .chain((case() != "legacy").then_some(OTHER))
        .enumerate()
    {
        elements.push(
            json!({"type":"usergroup","usergroup_id":id,"fixture_occurrence":occurrence+1,
            "tab_id":"opaque-tab","opaque":{"keep":true}}),
        );
        if known_freshness::enabled() && occurrence < 2 {
            elements.last_mut().unwrap()["style"] = if occurrence == 0 {
                json!({"bold":true})
            } else {
                json!({"italic":true})
            };
        }
    }
    if matches!(case().as_str(), "mixed-member" | "mixed-file-group") {
        elements.push(
            json!({"type":"user","user_id":MEMBER,"fixture_occurrence":4,
            "tab_id":"opaque-member-tab","opaque":{"keep":true}}),
        );
    }
    if mixed_absent::enabled() {
        elements.extend(mixed_absent::tokens());
    }
    if case() == "mixed-file-group" || mixed_absent::enabled() {
        let file_blocks = super::files::blocks(text);
        elements.extend(
            file_blocks[0]["elements"][0]["elements"]
                .as_array()
                .unwrap()[1..]
                .iter()
                .cloned(),
        );
    }
    vec![json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":elements}]})]
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
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !enabled() {
        return Err("Closed group recovery fixture required".into());
    }
    if known_freshness::enabled() {
        return known_freshness::seed(state).await;
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing group fixture owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let mut users = vec![user.clone()];
    if matches!(case().as_str(), "mixed-member" | "mixed-file-group") {
        users.push(mixed::raw(&owner.team));
    }
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Group Retry"},"self":user,"users":users,
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Group Retry"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    if mixed_absent::enabled() {
        mixed_absent::record_seed(&boot, &init)?;
    }
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    if case() == "legacy" {
        // A legacy cache projection has membership but no actual label witness.
        let legacy = serde_json::from_value(json!({"id":GROUP,"team_id":owner.team,
            "handle":"Legacy name must not win","users":[owner.user,"UKEEP001"],"members_loaded":true}))
            .map_err(|_| "Decode legacy group fixture")?;
        state
            .write()
            .core
            .workspaces
            .get_mut(&owner.team)
            .ok_or("Missing group workspace")?
            .usergroups
            .insert(GROUP.into(), legacy);
    } else if case() == "ready" {
        for id in ids() {
            event(&mut state.write(), raw(&id))?;
        }
    }
    Ok(())
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    context: &Context,
    request: &PreparedRequest,
    kind: &str,
) -> Reply {
    if !enabled() || !context.current(&state.peek()) {
        return Err(Error::Offline("closed group fixture".into()));
    }
    let method = match kind {
        "discovery" => "api.getFlannelHttpUrl",
        "directory" => "usergroups/info",
        "direct" => "subteams.info",
        _ => return Err(Error::Api("fixture_group_request_kind".into())),
    };
    if request.method != "POST" || !request.url.contains(method) {
        return Err(Error::Api("fixture_group_request_route".into()));
    }
    let body = match &request.body {
        RequestBody::Form(pairs) => pairs
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "token" | "enterprise_token"))
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        RequestBody::Json(value) => {
            let mut body = value.clone();
            let map = body
                .as_object_mut()
                .ok_or_else(|| Error::Api("fixture_group_request_shape".into()))?;
            map.remove("token");
            map.remove("enterprise_token");
            body
        }
    };
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
        .map_err(|_| Error::Offline("closed group receiver retired".into()))?
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
        Err("Group receiver retired before reply".into())
    }
}
pub(crate) fn receiver_retired(serial: u64) -> bool {
    ledger()
        .lock()
        .unwrap()
        .held
        .iter()
        .any(|(id, sender)| *id == serial && sender.is_closed())
}
/// Only the explicit cancellation/account-retirement acceptance path expects
/// a closed receiver. Ordinary callbacks still require actual delivery.
pub(crate) fn release_retired(serial: u64, response: Reply) -> Result<Value, String> {
    if !receiver_retired(serial) {
        return Err("Original group receiver is not retired".into());
    }
    let observed = release_observed(serial, response)?;
    if observed["receiverRetired"] == true && observed["delivered"] == false {
        Ok(observed)
    } else {
        Err("Retired group callback unexpectedly delivered".into())
    }
}
pub(super) fn release_observed(serial: u64, response: Reply) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed group callback required".into());
    }
    let sender = {
        let mut book = ledger().lock().unwrap();
        let index = book
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Missing held group callback")?;
        let row = book
            .requests
            .iter_mut()
            .find(|r| r["serial"] == serial)
            .unwrap();
        row["released"] = json!(true);
        if known_freshness::enabled() {
            row["response"] = match &response {
                Ok(raw) => json!({"raw":raw}),
                Err(Error::Offline(_)) => json!({"error":"offline"}),
                Err(Error::RateLimited { retry_after_secs }) => {
                    json!({"error":"rate-limited","retryAfterSeconds":retry_after_secs})
                }
                Err(_) => json!({"error":"other"}),
            };
        }
        row["outcome"] = json!(if response.is_ok() {
            "response"
        } else if matches!(&response, Err(Error::RateLimited { .. })) {
            "rate-limited"
        } else {
            "offline"
        });
        if let Err(Error::RateLimited { retry_after_secs }) = &response {
            row["retryAfterSeconds"] = json!(retry_after_secs);
        }
        book.held.remove(index).1
    };
    let retired = sender.is_closed();
    let delivered = sender.send(response).is_ok();
    let mut book = ledger().lock().unwrap();
    let row = book
        .requests
        .iter_mut()
        .find(|row| row["serial"] == serial)
        .unwrap();
    row["receiverRetired"] = json!(retired);
    row["delivered"] = json!(delivered);
    Ok(json!({"serial":serial,"receiverRetired":retired,"delivered":delivered}))
}
pub(crate) fn raw(id: &str) -> Value {
    json!({"id":id,"team_id":"TINIT0001","handle":if id==GROUP{LABEL}else{"Review crew"},
        "name":"Fixture crew","date_delete":0,"is_visible":true,"fixture_metadata":{"keep":true}})
}
pub(crate) fn event(shell: &mut ShellState, raw: Value) -> Result<(), String> {
    if !enabled() {
        return Err("Closed group event required".into());
    }
    let event = serde_json::from_value(json!({"type":"subteam_updated","subteam":raw}))
        .map_err(|_| "Decode group event")?;
    let team = shell
        .core
        .active_team
        .clone()
        .ok_or("Missing group event team")?;
    let w = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing group event workspace")?;
    super_platinum_core::slack::models::apply_group_event(
        &mut w.usergroups,
        &w.self_user_id,
        &event,
    );
    shell.refresh_from_core();
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let w = shell
        .core
        .active_team
        .as_ref()
        .and_then(|t| shell.core.workspaces.get(t));
    let groups: serde_json::Map<String, Value> = ids()
        .into_iter()
        .map(|id| {
            let row = w.and_then(|w| w.usergroups.get(&id));
            (
                id,
                json!({"native":row,"source":row.and_then(|g|g.label_source.as_ref()),
            "label":row.and_then(|g|g.profile_label())}),
            )
        })
        .collect();
    let mut snapshot = json!({"profile":mentions::snapshot(shell),"account":shell.core.active_account,"epoch":shell.core.account_epoch,
        "groups":groups,"requests":requests(),"pending":pending(),
        "mixedMember":mixed::snapshot(shell),
        "driver":crate::usergroups::recovery::fixture_status(shell)});
    if case() == "mixed-file-group" || mixed_absent::enabled() {
        snapshot["mixedFile"] = json!({
            "source":super::files::status(),
            "cache":super::super::rich::files::fixture_status(shell, super::files::ID),
            "opening":shell.profile_edit.sections.draft.as_ref().map(|d|json!({
                "visit":d.visit,"serial":d.serial,"key":d.rich_opening,
                "reading":d.rich_reading,"mounted":d.rich.is_some()}))
        });
    }
    if mixed_absent::enabled() {
        snapshot["mixedAbsentMember"] = mixed_absent::snapshot(shell);
    }
    if case() == "rate-limit" {
        snapshot["rate"] = shell
            .core
            .active_team
            .as_deref()
            .map_or(Value::Null, |team| {
                crate::usergroups::recovery::rate_snapshot(shell, team)
            });
    }
    if known_freshness::enabled() {
        snapshot["knownFreshness"] = known_freshness::snapshot(shell);
    }
    snapshot
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
    json!({"before":before,"after":snapshot(&state.write()),"actualInstallLoaded":true})
}
