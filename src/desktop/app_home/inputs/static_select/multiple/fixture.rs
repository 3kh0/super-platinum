//! Offline multi-static callbacks through the production DOM and request boundary.
use super::super::fixture as base;
use super::super::*;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::button::fixture as buttons;
use dioxus::desktop::DesktopContext;
mod helpers;
mod lanes;
use helpers::refresh;
use helpers::*;

pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-static-dispatch" | "app-home-multi-static-dispatch-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_STATIC_DISPATCH_CHECK").as_deref()
            == Ok("1")
        && matches!(case().as_str(), "selection" | "validation" | "retirement")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_MULTI_STATIC_DISPATCH_CASE").unwrap_or_default()
}
fn key(block: &str) -> InputKey {
    base::key(block)
}
pub(crate) fn install(s: &mut ShellState) {
    base::install(s);
    let a = base::option("alpha", "Alpha");
    let b = base::option("beta", "Beta");
    let g = base::option("gamma", "Gamma");
    let e = base::option("", "No release");
    let element = |initial: Value| json!({"type":"multi_static_select","action_id":"value","placeholder":{"type":"plain_text","text":"Choose releases"},"initial_options":initial,"max_selected_items":2,"option_groups":[{"label":{"type":"plain_text","text":"Releases"},"options":[a.clone(),b.clone(),g.clone(),e.clone()]}]});
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release choices"},"element":element(json!([]))},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required releases"},"element":element(json!([a.clone()]))},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet releases"},"element":element(json!([a.clone()]))},
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
                json!({"ok":false,"fixture":n,"case":case(),"error":"Multi-static fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-multi-static-dispatch-check: {result}")
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
        json!({"events":"actual DOM checkbox/chip and Button callbacks; synthetic offline events"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "validation" => lanes::validation(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown multi-static case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
