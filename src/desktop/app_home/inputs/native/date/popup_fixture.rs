//! Offline app-owned calendar interactions; no OS input or Slack transport.
use super::fixture as existing;
use crate::app_home::inputs::external::fixture::capture;
use crate::app_home::inputs::{self, text};
use crate::app_home::*;
use dioxus::desktop::DesktopContext;
use serde_json::{Value, json};
mod dismissal;
mod helpers;
mod lanes;
mod props;
use helpers::*;

pub(crate) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-date-popup" | "app-home-date-popup-narrow")
        )
}
pub(crate) fn named() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_APP_HOME_DATE_POPUP_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "popup" | "guards" | "dismissal" | "props")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_DATE_POPUP_CASE").unwrap_or_default()
}
pub(crate) fn install(s: &mut ShellState) {
    existing::install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][0]["element"]["initial_date"] = json!("2026-10-26");
    raw["blocks"][1]["element"]["initial_date"] = json!("2026-10-27");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
fn snapshot(s: &ShellState) -> Value {
    let mut x = existing::snapshot(s);
    x["case"] = json!(case());
    x
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let n = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(180), run(s, &d, &n)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":e,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":n,"case":case(),"error":"Date popup fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("app-home-date-popup-check: {result}"))
                .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    settle(s, d).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        s,
        d,
        n,
        "initial",
        "pick",
        json!({"syntheticDomIntent":true,"physicalInput":false}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case() == "props" {
        props::run(s, d, n, &mut stages, &mut captures).await?;
    } else if case() == "dismissal" {
        dismissal::run(s, d, n, &mut stages, &mut captures).await?;
    } else {
        lanes::run(s, d, n, &mut stages, &mut captures).await?;
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,
        "viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,
        "model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0}),
    )
}
