use super::*;
use capture::{click, dom, type_query, wait};
use dioxus::desktop::DesktopContext;
mod helpers;
mod lanes;
mod retirement;
use helpers::{ready, shot};
pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(180),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":error,"model":snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Home multi-user fixture timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-multi-user-select-check: {result}")
        })
        .await;
    });
}
async fn run(state: Signal<ShellState>, d: &DesktopContext, name: &str) -> Result<Value, String> {
    let readiness = capture::readiness(name).await?;
    let source = crate::app_home::context::fixture::publish(state, false, "TWORK0002").await?;
    ready(state).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        state,
        d,
        name,
        "initial",
        json!({"source":source,"homeOpenedFixtureAdapter":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(state, d, name, &mut stages, &mut captures).await?,
        "props" => lanes::props(state, d, name, &mut stages, &mut captures).await?,
        "failure" => lanes::failure(state, d, name, &mut stages, &mut captures).await?,
        "retirement" => retirement::retirement(state, d, name, &mut stages, &mut captures).await?,
        _ => return Err("Unknown user case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&state.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
