//! Closed external-option dispatch through real query, action and Button paths.
use crate::app_home::inputs::external::fixture::{self as suggestions, capture};
use crate::app_home::inputs::static_select::fixture as base;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::{self, static_select};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::{InputKey, InputKind, InputValue};
mod admission;
mod gestures;
mod helpers;
mod navigation;
mod props;
mod retirement;
mod selection;
use gestures::*;
use helpers::refresh;
use helpers::*;
use navigation::{reset, revisit};
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-external-dispatch" | "app-home-external-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_EXTERNAL_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "selection" | "props" | "retirement" | "admission"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_EXTERNAL_DISPATCH_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
fn installed() -> Value {
    suggestions::option("Installed release", "installed")
}
fn alpha() -> Value {
    suggestions::option("Release Alpha", "alpha,desktop")
}
fn beta() -> Value {
    suggestions::option("Release Beta", "beta")
}
fn gamma() -> Value {
    let mut v = suggestions::option("Release Gamma", "alpha,desktop");
    v["opaque"]["nested"] = json!(["changed",{"n":8}]);
    v
}
pub(crate) fn install(s: &mut ShellState) {
    crate::app_home::context::fixture::install(s);
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team, false);
    let previous = inputs::owner(s).unwrap();
    inputs::external::dispatch::reset(s, &previous);
    inputs::native::reset(s, &previous);
    static_select::reset(s, &previous);
    s.app_home.forms.remove(&previous);
    s.app_home.input_revisions.remove(&previous);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw.as_object_mut().unwrap().remove("state");
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release option"},"element":{"type":"external_select","action_id":"value","min_query_length":0,"initial_option":installed(),"placeholder":{"type":"plain_text","text":"Find release"}}},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required option"},"element":{"type":"external_select","action_id":"value","min_query_length":0,"placeholder":{"type":"plain_text","text":"Find required release"}}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet option"},"element":{"type":"external_select","action_id":"value","min_query_length":0,"initial_option":installed(),"placeholder":{"type":"plain_text","text":"Find quiet release"}}},
        {"type":"input","block_id":"multi","optional":true,"label":{"type":"plain_text","text":"Quiet options"},"element":{"type":"multi_external_select","action_id":"value","min_query_length":0,"initial_options":[installed()],"max_selected_items":2,"placeholder":{"type":"plain_text","text":"Find releases"}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    base::initialize(s);
    buttons::initialize(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(inputs::native::date::proof(s).as_object().unwrap().clone());
    x.as_object_mut()
        .unwrap()
        .extend(super::proof(s).as_object().unwrap().clone());
    let q = suggestions::snapshot(s);
    x["suggestions"] = json!({"requests":q["requests"],"releases":q["releases"],"pending":q["pending"],"query":q["query"]});
    x["pickerBlock"] = json!(
        s.app_home
            .external
            .picker
            .as_ref()
            .map(|p| &p.ticket.key.block_id)
    );
    x["validated"] = q["validated"].clone();
    x["activeConversation"] =
        json!({"channel":s.core.active_channel,"generation":s.channel_generation});
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"External dispatch fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-external-dispatch-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let source = crate::app_home::context::fixture::publish(s, false, "TWORK0002").await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"source":source,"events":"actual external picker DOM callbacks; synthetic events, no physical OS input","homeOpenedFixtureAdapter":true}),&mut stages,&mut captures).await?;
    match case().as_str() {
        "selection" => selection::run(s, d, n, &mut stages, &mut captures).await?,
        "props" => props::run(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => retirement::run(s, d, n, &mut stages, &mut captures).await?,
        "admission" => admission::run(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown external dispatch case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
