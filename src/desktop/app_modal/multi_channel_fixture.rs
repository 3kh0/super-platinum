//! Inline multi-channel selection retains dirty arrays and records no network writes.
use super::*;
use serde_json::json;
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_MULTI_CHANNEL_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-multi-channel" | "app-modal-multi-channel-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    channel_fixture::install(shell);
    let recorder = shell.app_modal.as_mut().unwrap().users_fixture.take();
    let mut session =
        Session::fixture_with_origin(view(), Some(conversation_fixture::ORIGIN.into()));
    session.users_fixture = recorder;
    shell.app_modal = Some(session);
}
pub(crate) fn view() -> Value {
    let mut v = channel_fixture::view();
    v["title"]["text"] = json!("Choose public channels");
    v["blocks"][1]["label"]["text"] = json!("Channels");
    v["blocks"][1]["element"] = json!({"type":"multi_channels_select","action_id":"value","initial_channels":["C00000006"],"max_selected_items":2,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find public channels"}});
    v["blocks"][2]["element"] = json!({"type":"multi_channels_select","action_id":"value","response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find optional channels"}});
    v
}
