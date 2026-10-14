//! Inline multi-conversation selection retains dirty arrays and records no network writes.
use super::*;
use serde_json::json;
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_MULTI_CONVERSATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-multi-conversation" | "app-modal-multi-conversation-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    conversation_fixture::install(shell);
    let recorder = shell.app_modal.as_mut().unwrap().users_fixture.take();
    // Exercise the same visible-conversation attestation as production dispatch.
    // The source-origin and explicit initial deliberately name a different room.
    shell.overlay = None;
    shell.main_view = crate::model::MainView::Home;
    shell.channel_tab = crate::model::ChannelTab::Messages;
    shell.core.active_channel = Some("C00000006".into());
    shell.refresh_from_core();
    let viewed = super::origin::capture(shell).expect("fixture visible conversation");
    let mut session = Session::fixture_with_context(
        view(),
        Some(conversation_fixture::ORIGIN.into()),
        Some(viewed),
    );
    session.users_fixture = recorder;
    shell.app_modal = Some(session);
    shell.overlay = Some(Overlay::AppModal);
}
pub(crate) fn view() -> Value {
    let mut v = conversation_fixture::view();
    v["title"]["text"] = json!("Choose conversations");
    v["blocks"][1]["label"]["text"] = json!("Conversations");
    v["blocks"][1]["element"] = json!({"type":"multi_conversations_select","action_id":"value","initial_conversations":["C00000001"],"max_selected_items":2,"default_to_current_conversation":true,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find conversations"}});
    v["blocks"][2]["element"] = json!({"type":"multi_conversations_select","action_id":"value","filter":{"include":["public","private","mpim","im"],"exclude_bot_users":true,"exclude_external_shared_channels":true},"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find optional conversations"}});
    v.as_object_mut().unwrap().remove("state");
    v
}
