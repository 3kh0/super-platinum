//! Exact named, closed wrapper fixture. All actions use the production paths.
use super::super::fixture as base;
use super::*;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::text::button::fixture as buttons;
use dioxus::desktop::DesktopContext;
mod helpers;
mod lanes;
use helpers::*;
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-static-wrappers" | "app-home-static-wrappers-narrow")
        )
        && std::env::var("SUPER_PLATINUM_APP_HOME_STATIC_WRAPPERS_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "selection" | "pending" | "retirement")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_STATIC_WRAPPERS_CASE").unwrap_or_default()
}
pub(crate) fn key(block: &str, action: &str) -> InputKey {
    InputKey {
        block_id: block.into(),
        action_id: action.into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    base::install(s);
    let a = base::option("alpha", "Alpha");
    let b = base::option("beta", "Beta");
    let g = base::option("gamma", "Gamma");
    let empty = base::option("", "No release");
    let select = |action: &str, label: &str| json!({"type":"static_select","action_id":action,"accessibility_label":label,"placeholder":{"type":"plain_text","text":label},"initial_option":a,"option_groups":[{"label":{"type":"plain_text","text":"Releases"},"options":[a,b,g,empty]}]});
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Input release"},"element":select("value","Choose input release")},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"actions","block_id":"controls","elements":[select("first","Primary release"),select("second","Secondary release"),{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]},
        {"type":"section","block_id":"summary","text":{"type":"mrkdwn","text":"Choose the release shown beside this summary."},"accessory":select("accessory","Summary release")}
    ]);
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
    buttons::initialize(s);
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let mut x = base::snapshot(s);
    buttons::augment(s, &mut x);
    x["case"] = json!(case());
    let o = owner(s);
    x["widgets"]=json!(s.app_home.static_select.widgets.iter().filter(|((f,_),_)|Some(f)==o.as_ref()).map(|((_,k),w)|json!({"block":k.block_id,"action":k.action_id,"visit":w.visit,"initial":w.initial,"options":w.options,"visible":w.visible})).collect::<Vec<_>>());
    x["mounts"]=json!(s.app_home.static_select.mounts.values().map(|m|json!({"mount":m.id,"seq":m.seq,"value":m.value,"block":m.binding.key.block_id,"action":m.binding.key.action_id,"current":m.binding.current(s)})).collect::<Vec<_>>());
    x["buttonProcessing"] = actions::wrappers_fixture_processing(s);
    x["buttonEnabled"] = json!(button_enabled(s));
    x
}
fn button_enabled(s: &ShellState) -> bool {
    let Some(h) = s.app_home.surface.as_ref() else {
        return false;
    };
    let metadata = super_platinum_core::block_actions::BlockElementMetadata {
        source: BlockElementSource::Button,
        block_id: Some("controls".into()),
        action_id: Some("send".into()),
        value: Some("send".into()),
        confirm: super_platinum_core::block_actions::BlockConfirmationState::Missing,
        accessibility_label: None,
    };
    actions::enabled(
        s,
        actions::Key {
            visit: h.id,
            revision: h.revision,
        },
        &metadata,
    )
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let n = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &n)).await {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":error,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":"Static wrapper fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-static-wrappers-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"homeOpenedFixtureAdapter":true,"events":"native select change and actual Button onclick; synthetic offline events"}),&mut stages,&mut captures).await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "pending" => lanes::pending(s, d, n, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown wrapper case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
