//! Closed nested Canvas-title member and complete hidden-policy inputs.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{block_actions::name_policy, slack::PreparedRequest};

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_USER_TITLE_CHECK";
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_USER_TITLE_CASE").unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && std::env::var(files::FLAG).as_deref() == Ok("1")
        && std::env::var(files::BLOCKS_FLAG).as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "known" | "hidden" | "recovery" | "failure" | "cancel"
        )
}
pub(crate) fn label() -> &'static str {
    if case() == "hidden" {
        "Review with Hidden user / Hidden user &amp; plan"
    } else {
        "Review with @  Cafe\u{0301} Harbor   / @  Cafe\u{0301} Harbor   &amp; plan"
    }
}
pub(crate) fn title_blocks() -> Value {
    json!([{"type":"rich_text","block_id":"file-title-block",
    "opaque_block":{"retain":["block",7]},"elements":[{
    "type":"rich_text_section","opaque_section":{"retain":true},"elements":[
        {"type":"text","text":"Review with ","opaque_run":{"index":0}},
        {"type":"user","user_id":member_recovery::MEMBER,"style":{"italic":true},"opaque_run":{"index":1}},
        {"type":"text","text":" / ","opaque_run":{"index":2}},
        {"type":"user","user_id":member_recovery::MEMBER,"style":{"bold":true},"opaque_run":{"index":3}},
        {"type":"text","text":" &amp; plan","opaque_run":{"index":4}}
    ]}]}])
}
fn policy() -> Value {
    json!({"ok":true,"prefs":{"display_real_names_override":1,
        "hidden_users":if case()=="hidden"{member_recovery::MEMBER}else{""}},
        "opaque_policy":"original-complete-policy"})
}
#[derive(Default)]
struct Ledger {
    seed: Value,
    policy: Vec<Value>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !mentions::enabled(&state.peek()) {
        return Err("Closed user-title owner required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let mut users = vec![user.clone()];
    if matches!(case().as_str(), "known" | "hidden") {
        users.push(member_recovery::success(&owner.team)["results"][0].clone());
    }
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"User title"},"self":user,"users":users,
        "workspaces":[{"id":owner.team}],"channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"User title"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    let evidence = json!({"boot":boot,"init":init,"account":owner.account,"team":owner.team,"user":owner.user});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    ledger().lock().unwrap().seed = evidence;
    Ok(())
}
pub(crate) async fn policy_read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    if super::completion::title_users::named() {
        return super::completion::request(&state.peek(), request).map(|v| v.0);
    }
    let s = state.peek();
    if !named() || !mentions::enabled(&s) {
        return Err(Error::Offline("closed title policy".into()));
    }
    let owner = crate::directory_member_fetches::Context::capture(
        &s,
        s.core
            .active_team
            .as_deref()
            .ok_or_else(|| Error::Api("fixture_team".into()))?,
    )?;
    let expected = name_policy::request::user(&owner.client, &owner.session);
    if !files::validate(request, &expected) {
        return Err(Error::Api("closed_title_policy_factory".into()));
    }
    let raw = policy();
    let mut book = ledger().lock().unwrap();
    let serial = book.policy.len() + 1;
    book.policy.push(
        json!({"serial":serial,"method":"users.prefs.get","body":{},"account":owner.account,
        "team":owner.session.team_id,"user":owner.session.user_id,"epoch":owner.epoch,"raw":raw}),
    );
    Ok(raw)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let book = ledger().lock().unwrap();
    json!({"case":case(),"seed":book.seed,"files":files::status(),"member":member_recovery::snapshot(shell),"policy":book.policy,"networkCalls":0})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_nested_users_retain_order_and_outer_wire() {
        let raw = title_blocks();
        let ids = super_platinum_core::slack::api::custom::texty::file_titles::user_ids(
            &json!({"title_blocks":raw}),
        )
        .unwrap();
        assert_eq!(
            ids.into_iter().collect::<Vec<_>>(),
            vec![member_recovery::MEMBER]
        );
        assert_eq!(
            raw[0]["elements"][0]["elements"][1]["style"],
            json!({"italic":true})
        );
        assert_eq!(
            raw[0]["elements"][0]["elements"][3]["style"],
            json!({"bold":true})
        );
        assert_eq!(raw[0]["elements"][0]["elements"][4]["text"], " &amp; plan");
    }
}
