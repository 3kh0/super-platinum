//! Closed email input acceptance through the mounted email control.
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::button::fixture as buttons;
use crate::app_home::inputs::text::fixture as base;
use crate::app_home::inputs::{self, text};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
use super_platinum_core::block_actions::form::InputKey;
mod admission;
mod events;
mod gestures;
mod helpers;
mod navigation;
mod retirement;
mod state;
use gestures::*;
use helpers::refresh;
use helpers::*;
pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-email-input" | "app-home-email-input-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_EMAIL_INPUT_CHECK").as_deref() == Ok("1")
        && matches!(
            case().as_str(),
            "events" | "state" | "retirement" | "admission"
        )
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_EMAIL_INPUT_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: "value".into(),
    }
}
pub(crate) fn event(s: &mut ShellState, e: &Value, k: &InputKey) {
    base::email_event(s, e, k)
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
     {"type":"input","block_id":"enter","dispatch_action":true,"label":{"type":"plain_text","text":"Release email"},"element":{"type":"email_text_input","action_id":"value","initial_value":"Initial@Example.com","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
     {"type":"input","block_id":"live","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Live email"},"element":{"type":"email_text_input","action_id":"value","initial_value":"live@example.com","dispatch_action_config":{"trigger_actions_on":["on_character_entered"]}}},
     {"type":"input","block_id":"default","optional":true,"dispatch_action":true,"label":{"type":"plain_text","text":"Optional email"},"element":{"type":"email_text_input","action_id":"value"}},
     {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet email"},"element":{"type":"email_text_input","action_id":"value","initial_value":"quiet@example.com","dispatch_action_config":{"trigger_actions_on":["on_enter_pressed"]}}},
     {"type":"input","block_id":"amount","optional":true,"label":{"type":"plain_text","text":"Quiet quantity"},"element":{"type":"number_input","action_id":"value","is_decimal_allowed":true,"initial_value":"5.00"}},
     {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
     {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value","initial_date":"2026-10-25"}},
     {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    base::initialize_observation(s);
    buttons::initialize(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    base::admission::augment_observation(s, &mut x);
    x.as_object_mut()
        .unwrap()
        .extend(text::email::proof(s).as_object().unwrap().clone());
    x.as_object_mut()
        .unwrap()
        .extend(inputs::native::date::proof(s).as_object().unwrap().clone());
    x.as_object_mut()
        .unwrap()
        .extend(text::number::proof(s).as_object().unwrap().clone());
    x["case"] = json!(case());
    x["named"] = json!(named());
    x["activeConversation"] =
        json!({"channel":s.core.active_channel,"generation":s.channel_generation});
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Email fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("app-home-email-input-check: {result}"))
                .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let source = crate::app_home::context::fixture::publish(s, false, "TWORK0002").await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"source":source,"homeOpenedFixtureAdapter":true,"events":"actual mounted email input DOM; synthetic events, no physical OS input"}),&mut stages,&mut captures).await?;
    match case().as_str() {
        "events" => events::run(s, d, n, &mut stages, &mut captures).await?,
        "state" => state::run(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => retirement::run(s, d, n, &mut stages, &mut captures).await?,
        "admission" => admission::run(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown email fixture".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
