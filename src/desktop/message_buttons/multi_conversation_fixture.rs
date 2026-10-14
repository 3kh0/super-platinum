//! Ordered public-channel selections, with disconnected mutation recording.
use crate::state::ShellState;
use serde_json::json;
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_MULTI_CONVERSATION_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-multi-conversations-select" | "message-multi-conversations-select-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    super::conversation_fixture::install(shell);
    let m = shell
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .messages
        .get_mut("C2")
        .unwrap()
        .messages
        .iter_mut()
        .find(|m| m.ts.as_deref() == Some(super::user_fixture::MAIN_TS))
        .unwrap();
    m.blocks[0]["text"]["text"] = json!(
        "Choose conversations. Apply submits one action; Cancel keeps the saved destinations."
    );
    let confirm = m.blocks[1]["accessory"]["confirm"].clone();
    m.blocks[1]["accessory"] = json!({"type":"multi_conversations_select","action_id":"pick","initial_conversations":["C00000001"],"max_selected_items":2,"default_to_current_conversation":false,"filter":{"include":["public","private","mpim","im"]},"required":true,"placeholder":{"type":"plain_text","text":"Choose conversations"},"confirm":confirm,"response_url_enabled":true});
    m.blocks[2]["elements"][0] = json!({"type":"multi_conversations_select","action_id":"other","filter":{"include":["public","private","mpim","im"],"exclude_bot_users":true,"exclude_external_shared_channels":true},"placeholder":{"type":"plain_text","text":"Choose follow-up conversations"},"response_url_enabled":true});
    super::user_fixture::reproject(shell);
}
