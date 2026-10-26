//! Exact named multi-conversation fixture, sharing the accepted source/read manager.
use super::*;
pub(crate) mod check;
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-conversation-select" | "app-home-multi-conversation-select-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_SELECT_CHECK").as_deref()
            == Ok("1")
        && matches!(
            case().as_str(),
            "selection" | "props" | "filter" | "recovery"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_CONVERSATION_SELECT_CASE").unwrap_or_default()
}
pub(crate) fn field(block: &mut Value) {
    block["optional"] = json!(case() != "selection");
    block["label"]["text"] = json!("Release conversations");
    block["element"] = json!({"type":"multi_conversations_select","action_id":"value","max_selected_items":2,"default_to_current_conversation":true,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find conversations"}});
    if case() != "selection" {
        block["element"]["initial_conversations"] = json!([ALPHA]);
    }
    if case() == "filter" {
        block["element"]["filter"] =
            json!({"only_from_team_id":"TWORK0002","exclude_bot_users":true});
    }
}
/// Actual current view_updated receipt. Initial None means retain, Null removes
/// the raw property; array changes only the mounted widget prop.
pub(crate) fn refresh(
    s: &mut ShellState,
    optional: bool,
    maximum: usize,
    initial: Option<Value>,
    saved: Option<Vec<&str>>,
) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("multi-conversation-refresh-{before}"));
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(optional);
    block["element"]["max_selected_items"] = json!(maximum);
    if let Some(initial) = initial {
        if initial.is_null() {
            block["element"]
                .as_object_mut()
                .unwrap()
                .remove("initial_conversations");
        } else {
            block["element"]["initial_conversations"] = initial;
        }
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_conversations_select","selected_conversations":saved}}}});
    }
    let event = RtEvent::Unknown(
        serde_json::from_value(
            json!({"type":"view_updated","app_id":"A_HOME","view_id":"V_HOME","view":raw.clone()}),
        )
        .unwrap(),
    );
    crate::app_home::realtime(s, &team, rt, &event);
    if !s.app_home.surface.as_ref().is_some_and(|h| {
        h.owner.current(s)
            && h.revision != before
            && h.view.as_ref().is_some_and(|v| v.raw() == &raw)
    }) {
        return Err("Owned multi-conversation Home refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
