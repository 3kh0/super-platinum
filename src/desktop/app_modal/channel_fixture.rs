//! cdYm normal input fixture: initial channel, public-only choices, local null clear.
use super::*;
use serde_json::json;

pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_CHANNEL_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-channel" | "app-modal-channel-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    conversation_fixture::install(shell);
    let recorder = shell.app_modal.as_mut().unwrap().users_fixture.take();
    // Retained origin differs from the explicit initial value and must not fill an empty field.
    let mut session =
        Session::fixture_with_origin(view(), Some(conversation_fixture::ORIGIN.into()));
    session.users_fixture = recorder;
    shell.app_modal = Some(session);
    let channel = serde_json::from_value(
        json!({"id":"D00000008","is_im":true,"user":"U00000002","updated":10}),
    )
    .unwrap();
    shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("D00000008".into(), channel);
}
pub(crate) fn view() -> Value {
    json!({"id":users_fixture::VIEW,"app_id":"A_FIXTURE","type":"modal",
    "title":{"type":"plain_text","text":"Choose a public channel"},
    "submit":{"type":"plain_text","text":"Submit channel"},"close":{"type":"plain_text","text":"Cancel"},
    "blocks":[
        {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Note"},
            "element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained sibling draft"}},
        {"type":"input","block_id":"reviewers","label":{"type":"plain_text","text":"Channel"},
            "element":{"type":"channels_select","action_id":"value","initial_channel":"C00000006",
                "response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find a public channel"}}},
        {"type":"input","block_id":"optional","optional":true,"label":{"type":"plain_text","text":"Optional channel"},
            "element":{"type":"channels_select","action_id":"value","response_url_enabled":true,
                "placeholder":{"type":"plain_text","text":"Find a public channel"}}}
    ]})
}
