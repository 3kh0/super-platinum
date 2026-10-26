use super::*;
use capture::{click, dom, type_query, wait};
use dioxus::desktop::DesktopContext;
mod helpers;
mod lanes;
mod recovery;
use helpers::{ready, shot};
pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(240),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":error,"model":snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Home conversation fixture timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-conversation-select-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    let source = crate::app_home::context::fixture::publish(s, false, "TWORK0002").await?;
    ready(s).await?;
    if case() == "selection" {
        helpers::selected(s, None, None).await?;
    }
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        s,
        d,
        n,
        "initial",
        json!({"source":source,"homeOpenedFixtureAdapter":true,"defaultProvider":null}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(s, d, n, &mut stages, &mut captures).await?,
        "props" => lanes::props(s, d, n, &mut stages, &mut captures).await?,
        "filter" => lanes::filter(s, d, n, &mut stages, &mut captures).await?,
        "recovery" => recovery::run(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown conversation case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
