//! Ordered public-channel selections, with disconnected mutation recording.
use crate::state::ShellState;
use serde_json::json;
pub(crate) fn enabled() -> bool {
    std::env::var("SUPER_PLATINUM_MESSAGE_MULTI_CHANNEL_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("message-multi-channels-select" | "message-multi-channels-select-narrow")
        )
}
pub(crate) fn install(shell: &mut ShellState) {
    super::channel_fixture::install(shell);
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
        "Choose public channels. Apply submits one action; Cancel keeps the saved destinations."
    );
    let confirm = m.blocks[1]["accessory"]["confirm"].clone();
    m.blocks[1]["accessory"] = json!({"type":"multi_channels_select","action_id":"pick","initial_channels":["C00000001"],"max_selected_items":2,"required":true,"placeholder":{"type":"plain_text","text":"Choose channels"},"confirm":confirm,"response_url_enabled":true});
    m.blocks[2]["elements"][0] = json!({"type":"multi_channels_select","action_id":"other","placeholder":{"type":"plain_text","text":"Choose follow-up channels"},"response_url_enabled":true});
    super::user_fixture::reproject(shell);
}
