//! Exact named multi-member fixture, sharing the accepted source/read manager.
use super::*;
pub(crate) mod check;
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-user-select" | "app-home-multi-user-select-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_USER_SELECT_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "selection" | "props" | "failure" | "retirement"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_USER_SELECT_CASE").unwrap_or_default()
}
/// Actual current view_updated receipt. Initial None means retain, Null removes
/// the raw property; array changes only the mounted widget prop.
pub(super) fn refresh(
    s: &mut ShellState,
    optional: bool,
    maximum: usize,
    initial: Option<Value>,
    saved: Option<Vec<&str>>,
) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("multi-user-refresh-{before}"));
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
                .remove("initial_users");
        } else {
            block["element"]["initial_users"] = initial;
        }
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_users_select","selected_users":saved}}}});
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
        return Err("Owned multi-user Home refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
