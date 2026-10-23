//! Explicit nested-user completion fixtures; existing completion remains unchanged.
use super::super::{member_recovery as member, user_title};
use super::*;
use std::sync::OnceLock;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CHECK";
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_PROFILE_USER_TITLE_COMPLETION_CASE").unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && files::named()
        && std::env::var(FLAG).as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "known" | "hidden" | "recovery" | "retirement"
        )
}
pub(crate) fn label() -> &'static str {
    if case() == "hidden" {
        "Review with Hidden user / Hidden user &amp; plan"
    } else {
        "Review with @  Cafe\u{0301} Harbor   / @  Cafe\u{0301} Harbor   &amp; plan"
    }
}
pub(super) fn prefs() -> Value {
    if !named() {
        return json!({"ok":true,"prefs":{"frecency":"{}"}});
    }
    json!({"ok":true,"prefs":{"frecency":"{}","display_real_names_override":1,
        "hidden_users":if case()=="hidden"{member::MEMBER}else{""}},
        "opaque_policy":"original-completion-title-policy"})
}
fn with_title(raw: &str) -> String {
    // Keep channel_map key order: do not re-serialize the search envelope.
    raw.replacen(
        "\"title\":\"Alex Canvas\"",
        &format!(
            "\"title\":\"Alex Canvas\",\"title_blocks\":{}",
            user_title::title_blocks()
        ),
        1,
    )
}
pub(crate) fn response() -> String {
    if super::super::link_completion::named() {
        return super::super::link_completion::response();
    }
    if super::super::empty_canvas::completion() {
        return super::super::empty_canvas::response();
    }
    if named() {
        with_title(files::RESPONSE)
    } else {
        files::RESPONSE.into()
    }
}
fn seed_record() -> &'static Mutex<Value> {
    static SEED: OnceLock<Mutex<Value>> = OnceLock::new();
    SEED.get_or_init(|| Mutex::new(Value::Null))
}
pub(crate) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !named() || !enabled(&state.peek()) {
        return Err("Closed title completion required".into());
    }
    let owner = super::super::Owner::capture(&state.peek()).ok_or("Missing original owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"is_admin":true,"profile":{}});
    let local = json!({"id":mentions::USER,"name":"local-fixture","team_id":owner.team,
        "profile":{"display_name":"Alex Local","real_name":"Local real"}});
    let mut users = vec![user.clone(), local];
    if matches!(case().as_str(), "known" | "hidden") {
        users.push(member::success(&owner.team)["results"][0].clone());
    }
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Title completion"},"self":user,"users":users,
        "channels":[{"id":"CINIT0001","name":"warm-harbor","is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Title completion"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    let proof = json!({"boot":boot,"init":init,"account":owner.account,"team":owner.team,"user":owner.user});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot, init).await?;
    *seed_record().lock().unwrap() = proof;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    json!({"case":case(),"seed":seed_record().lock().unwrap().clone(),"fileResponseBytes":response(),
        "policy":prefs(),"member":member::snapshot(shell),"networkCalls":0})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_user_bytes_preserve_actual_file_order_and_plain_rank_title() {
        let bytes = with_title(files::RESPONSE);
        let decoded =
            super_platinum_core::slack::api::custom::texty::files::OrderedResponse::decode(
                bytes.as_bytes(),
            )
            .unwrap();
        assert_eq!(decoded.rows()[0]["title"], "Alex Canvas");
        assert_eq!(
            decoded.rows()[0]["title_blocks"],
            user_title::title_blocks()
        );
        assert!(decoded.rows()[1].get("title_blocks").is_none());
        assert_eq!(decoded.rows()[2]["mode"], "hosted");
        let mut store =
            super_platinum_core::slack::api::custom::texty::file_title_source::Store::new();
        let owner: WorkspaceSession=serde_json::from_value(json!({"team_id":"TINIT0001","user_id":"UINIT0001","name":"Fixture","url":"https://fixture.invalid","token":"synthetic"})).unwrap();
        store
            .ingest_batch_ordered(
                decoded.raw(),
                decoded.rows(),
                &owner,
                decoded.channel_map_order(),
            )
            .unwrap();
        assert_eq!(
            store
                .candidate(files::CANVAS)
                .unwrap()
                .channel_map_keys()
                .unwrap(),
            ["z-last", "a-first", "m-foreign"]
        );
    }
}
