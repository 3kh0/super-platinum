//! Offline editable Time callbacks through the existing calendar registry.
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::static_select::fixture as base;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::{self, static_select};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputKind, InputValue};
mod gestures;
mod helpers;
use gestures::*;
mod lanes;
mod navigation;
use helpers::refresh;
use helpers::*;
use navigation::{reset, revisit};

pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-time-dispatch" | "app-home-time-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "selection" | "props" | "retirement")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_TIME_DISPATCH_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    base::install(s);
    let w = s.core.workspaces.get_mut("T1").unwrap();
    w.locale = Some("en-US".into());
    w.notifications.user.insert("time24".into(), json!(false));
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release time"},"element":{"type":"timepicker","action_id":"value","initial_time":"09:30","timezone":"America/New_York","placeholder":{"type":"plain_text","text":"Choose release time"}}},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required time"},"element":{"type":"timepicker","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet time"},"element":{"type":"timepicker","action_id":"value","initial_time":"09:30","timezone":"America/New_York"}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    buttons::initialize(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(super::proof(s).as_object().unwrap().clone());
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Time fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-time-dispatch-check: {result}")
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
        json!({"events":"actual editable Time combobox input/option/key/blur observations; synthetic events, no physical OS/picker claim"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
        "props" => lanes::props(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown time case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
