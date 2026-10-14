//! Disconnected modal conversation selection with a correlated opening channel.
use super::*;
use serde_json::json;

pub(crate) const ORIGIN: &str = "C00000001";
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_APP_MODAL_CONVERSATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-modal-conversation" | "app-modal-conversation-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    users_fixture::install_view(shell, view());
    let recorder = shell.app_modal.as_mut().unwrap().users_fixture.take();
    let mut session = Session::fixture_with_origin(view(), Some(ORIGIN.into()));
    session.users_fixture = recorder;
    shell.app_modal = Some(session);
    let w = shell.core.workspaces.get_mut("T1").unwrap();
    w.channels.clear();
    for raw in [
        json!({"id":ORIGIN,"name":"launch-room","is_channel":true,"updated":10}),
        json!({"id":"C00000002","name":"design-review","is_channel":true,"updated":10}),
        json!({"id":"G00000003","name":"private-review","is_private":true,"is_group":true,"updated":10}),
        json!({"id":"G00000004","name":"Review group","is_mpim":true,"members":["U00000001","U00000002","U00000003"],"updated":10}),
        json!({"id":"C00000005","name":"archived-room","is_channel":true,"is_archived":true,"updated":10}),
        json!({"id":"C00000006","name":"external-room","is_channel":true,"is_ext_shared":true,"updated":10}),
    ] {
        let channel: super_platinum_core::slack::models::Channel =
            serde_json::from_value(raw).unwrap();
        w.channels.insert(channel.id.clone(), channel);
    }
}
pub(crate) fn view() -> Value {
    json!({"id":users_fixture::VIEW,"app_id":"A_FIXTURE","type":"modal",
        "title":{"type":"plain_text","text":"Choose a conversation"},
        "submit":{"type":"plain_text","text":"Submit destination"},"close":{"type":"plain_text","text":"Cancel"},
        "blocks":[
            {"type":"input","block_id":"note","optional":true,"label":{"type":"plain_text","text":"Note"},
                "element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained sibling draft"}},
            {"type":"input","block_id":"reviewers","label":{"type":"plain_text","text":"Destination"},
                "element":{"type":"conversations_select","action_id":"value","default_to_current_conversation":true,
                    "response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find a conversation"}}},
            {"type":"input","block_id":"optional","optional":true,"label":{"type":"plain_text","text":"Optional destination"},
                "element":{"type":"conversations_select","action_id":"value","default_to_current_conversation":true,
                    "initial_conversation":"C00000002","response_url_enabled":true,
                    "filter":{"include":["public"],"exclude_external_shared_channels":true},
                    "placeholder":{"type":"plain_text","text":"Find a public channel"}}}
        ],"state":{"values":{"optional":{"value":{"type":"conversations_select","selected_conversation":null}}}}})
}
