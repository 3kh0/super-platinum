//! Finite Home multi fixture. Endpoint and native actions use existing production paths.
use super::*;
use serde_json::json;
use std::time::Duration;
mod capture;
pub(crate) mod check;
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_EXTERNAL_SELECT_CHECK").as_deref()
            == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-external-select" | "app-home-multi-external-select-narrow")
        )
        && matches!(
            case().as_str(),
            "selection" | "validation" | "failure" | "retirement" | "unified"
        )
}
pub(crate) fn unified() -> bool {
    named() && case() == "unified"
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_EXTERNAL_SELECT_CASE").unwrap_or_default()
}
pub(crate) fn install(s: &mut ShellState) {
    if case() == "unified" {
        crate::app_home::context::fixture::install(s);
    } else {
        fixture::install(s);
    }
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    let block = raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap();
    block["optional"] = json!(case() != "validation");
    block["label"]["text"] = json!("Release options");
    let initial = if case() == "validation" {
        vec![
            fixture::option("Installed release", "installed"),
            fixture::option("Release Alpha", "alpha,desktop"),
            fixture::option("Release Beta", "beta"),
        ]
    } else {
        vec![fixture::option("Installed release", "installed")]
    };
    block["element"] = json!({"type":"multi_external_select","action_id":"value","min_query_length":3,
        "initial_options":initial,"max_selected_items":2,"placeholder":{"type":"plain_text","text":"Find releases"}});
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    super::super::hydrate(s);
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    let mut value = fixture::snapshot(s);
    value["named"] = json!(named());
    value["case"] = json!(case());
    value["widget"] = multiple::snapshot(s, &fixture::key());
    value["contextSource"] = if case() == "unified" {
        crate::app_home::context::fixture::snapshot(s)
    } else {
        Value::Null
    };
    value
}
/// A real owned inline view update, checked by its committed raw payload/revision.
pub(super) fn refresh(
    s: &mut ShellState,
    max: usize,
    initial: Option<Value>,
    saved: Option<Value>,
) -> Result<Value, String> {
    let h = s.app_home.surface.as_ref().ok_or("Missing Home")?;
    let (team, rt, before) = (h.owner.ctx.session.team_id.clone(), h.owner.rt, h.revision);
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["hash"] = json!(format!("multi-refresh-{before}"));
    let element = &mut raw["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["block_id"] == "home-choice")
        .unwrap()["element"];
    element["max_selected_items"] = json!(max);
    if let Some(initial) = initial {
        element["initial_options"] = initial;
    }
    if let Some(saved) = saved {
        raw["state"] = json!({"values":{"home-choice":{"value":{"type":"multi_external_select","selected_options":saved}}}});
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
        return Err("Owned multi Home refresh refused".into());
    }
    Ok(
        json!({"beforeRevision":before,"afterRevision":s.app_home.surface.as_ref().unwrap().revision,"raw":raw}),
    )
}
