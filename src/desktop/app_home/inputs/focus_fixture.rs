//! Offline Home focus-on-load acceptance; explicit intent is separately recorded.
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::fixture as base;
use crate::app_home::inputs::{self, text};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::InputKey;
mod guards;
mod helpers;
mod lanes;
mod navigation;
use helpers::*;

const FIELDS: [&str; 6] = ["qsingle", "single", "qmulti", "multi", "amount", "email"];
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-focus-on-load" | "app-home-focus-on-load-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_FOCUS_ON_LOAD_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "mount" | "updates" | "lifecycle" | "guards"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_FOCUS_ON_LOAD_CASE").unwrap_or_default()
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
    inputs::static_select::reset(s, &previous);
    s.app_home.forms.remove(&previous);
    s.app_home.input_revisions.remove(&previous);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw.as_object_mut().unwrap().remove("state");
    raw["blocks"] = json!([
      {"type":"input","block_id":"qsingle","optional":true,"label":{"type":"plain_text","text":"Quiet text"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Quiet alpha"}},
      {"type":"input","block_id":"single","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Release text"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Release alpha","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
      {"type":"input","block_id":"qmulti","optional":true,"label":{"type":"plain_text","text":"Quiet notes"},"element":{"type":"plain_text_input","action_id":"value","multiline":true,"initial_value":"Quiet line one\nQuiet line two"}},
      {"type":"input","block_id":"multi","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","multiline":true,"initial_value":"Release line one\nRelease line two","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
      {"type":"input","block_id":"amount","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Release quantity"},"element":{"type":"number_input","action_id":"value","is_decimal_allowed":true,"initial_value":"5.00","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
      {"type":"input","block_id":"email","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Release email"},"element":{"type":"email_text_input","action_id":"value","initial_value":"focus@example.com","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
      {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value","initial_date":"2026-10-26"}}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    base::initialize_observation(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    base::admission::augment_observation(s, &mut x);
    for p in [
        inputs::focus::proof(s),
        text::email::proof(s),
        text::number::proof(s),
        inputs::native::date::proof(s),
    ] {
        x.as_object_mut()
            .unwrap()
            .extend(p.as_object().unwrap().clone());
    }
    x["case"] = json!(case());
    x["named"] = json!(named());
    x["activeConversation"] =
        json!({"channel":s.core.active_channel,"generation":s.channel_generation});
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Focus-on-load fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-focus-on-load-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let source = crate::app_home::context::fixture::publish(s, false, "TWORK0002").await?;
    settle(s, d).await?;
    let initial = probe("begin", "qsingle", json!({})).await?;
    no_field_focus(&initial["effect"])?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        s,
        d,
        n,
        "initial",
        "qsingle",
        json!({"source":source,"observer":initial}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "mount" => lanes::mount(s, d, n, &mut stages, &mut captures).await?,
        "updates" => lanes::updates(s, d, n, &mut stages, &mut captures).await?,
        "lifecycle" => lanes::lifecycle(s, d, n, &mut stages, &mut captures).await?,
        "guards" => guards::run(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown focus-on-load lane".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
