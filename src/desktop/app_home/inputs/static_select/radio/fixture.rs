//! Offline radio callbacks through the production DOM and request boundary.
use super::super::fixture as base;
use super::super::*;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::button::fixture as buttons;
use dioxus::desktop::DesktopContext;
mod helpers;
mod lanes;
mod navigation;
use helpers::refresh;
use helpers::*;
use navigation::{reset, revisit};

pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-radio-dispatch" | "app-home-radio-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_RADIO_DISPATCH_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "selection" | "validation" | "retirement")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_RADIO_DISPATCH_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    base::key(block)
}
pub(crate) fn install(s: &mut ShellState) {
    base::install(s);
    let a = base::option("alpha", "Alpha");
    let b = base::option("beta", "Beta");
    let mut g = base::option("gamma", "Gamma");
    g["text"] = json!({"type":"mrkdwn","text":"*Gamma*"});
    g["description"] = json!({"type":"mrkdwn","text":"_Staged_ release"});
    let e = base::option("", "No release");
    let element = |initial: Value| json!({"type":"radio_buttons","action_id":"value","initial_option":initial,"options":[a.clone(),b.clone(),g.clone(),e.clone()]});
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release choice"},"element":element(a.clone())},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required release"},"element":element(a.clone())},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet release"},"element":element(a.clone())},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
    raw["blocks"][1]["element"]
        .as_object_mut()
        .unwrap()
        .remove("initial_option");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    buttons::initialize(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Radio fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-radio-dispatch-check: {result}")
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
        json!({"events":"actual current-row DOM radio and Button callbacks; offline synthetic invocation; UA radio change may be trusted"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "validation" => lanes::validation(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown radio case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
