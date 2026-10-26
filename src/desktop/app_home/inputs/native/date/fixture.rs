//! Offline date commit callbacks through the existing calendar registry.
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::static_select::fixture as base;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::{self, static_select};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputKind, InputValue};
pub(crate) mod gestures;
pub(crate) mod helpers;
use gestures::*;
mod lanes;
pub(crate) mod navigation;
use helpers::refresh;
use helpers::*;
use navigation::{reset, revisit};

pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-date-dispatch" | "app-home-date-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "selection" | "retirement")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_DATE_DISPATCH_CASE").unwrap_or_default()
}
pub(crate) fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    base::install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release date"},"element":{"type":"datepicker","action_id":"value","initial_date":"2026-10-25","placeholder":{"type":"plain_text","text":"Choose release date"}}},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet date"},"element":{"type":"datepicker","action_id":"value","initial_date":"2026-10-25"}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    buttons::initialize(s);
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(super::proof(s).as_object().unwrap().clone());
    x.as_object_mut()
        .unwrap()
        .extend(inputs::focus::proof(s).as_object().unwrap().clone());
    x["case"] = json!(case());
    x["buttonProcessing"] = actions::wrappers_fixture_processing(s);
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Date fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-date-dispatch-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        s,
        d,
        n,
        "initial",
        json!({"events":"actual owned editable Date focus/input observations, auxiliary picker change and text Enter/blur commits; synthetic events only, no physical picker claim"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown date case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
