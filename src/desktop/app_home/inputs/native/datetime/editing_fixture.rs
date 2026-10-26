//! Offline Datetime editing/editing acceptance through the real mounted control.
use super::fixture as existing;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::{self, static_select, text};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::InputKey;
mod helpers;
mod lanes;
mod navigation;
use helpers::refresh;
use helpers::*;

const FIELDS: [&str; 4] = ["pick", "required", "quiet", "notes"];
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-datetime-editing" | "app-home-datetime-editing-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_DATETIME_EDITING_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "editing" | "counterpart" | "admission")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_DATETIME_EDITING_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn id(block: &str) -> String {
    if block == "home-empty" {
        block.into()
    } else {
        crate::view::app_modal::field_id(&key(block))
    }
}
pub(crate) fn install(s: &mut ShellState) {
    existing::install(s);
    s.core.workspaces.get_mut("T1").unwrap().locale = Some("de-DE".into());
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]["focus_on_load"] = json!(true);
    raw["blocks"][2]["optional"] = json!(false);
    raw["blocks"][2]["label"]["text"] = json!("Required date and time with value");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    if case() == "admission" {
        text::fixture::initialize_observation(s);
    }
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = static_select::fixture::snapshot(s);
    text::button::fixture::augment(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(super::proof(s).as_object().unwrap().clone());
    x["buttonProcessing"] = actions::wrappers_fixture_processing(s);
    x["case"] = json!(case());
    x["named"] = json!(named());
    x.as_object_mut()
        .unwrap()
        .extend(inputs::focus::proof(s).as_object().unwrap().clone());
    text::fixture::admission::augment_observation(s, &mut x);
    x
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let n = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &n)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":e,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":"Datetime editing fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-datetime-editing-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    settle(s, d).await?;
    let observer = probe("begin", "pick", json!({})).await?;
    no_field_focus(&observer["effect"])?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        s,
        d,
        n,
        "initial",
        "pick",
        json!({"observer":observer}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if let Err(error) = lanes::run(s, d, n, &mut stages, &mut captures).await {
        return Ok(json!({"ok":false,"fixture":n,"case":case(),"error":error,
            "readiness":readiness,"stages":stages,"captures":captures,
            "model":snapshot(&s.peek()),"captureChecksPassed":false,"networkCalls":0}));
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
