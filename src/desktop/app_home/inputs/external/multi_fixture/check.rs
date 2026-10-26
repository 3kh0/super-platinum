use super::*;
use capture::{click, dom, type_query, wait};
use dioxus::desktop::DesktopContext;
use fixture::{Reply, configure};
mod helpers;
mod lanes;
use helpers::{ready, shot};
pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":e,"model":snapshot(&state.peek())})
            }
            Err(_) => json!({"ok":false,"fixture":name,"case":case(),"error":"Multi Home timeout"}),
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-multi-external-select-check: {result}")
        })
        .await;
    });
}
async fn run(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = capture::readiness(name).await?;
    let initial = if unified() {
        let source = crate::app_home::context::fixture::publish(state, false, "TWORK0002").await?;
        let opened = crate::app_home::context::fixture::opened(state).await?;
        json!({"source":source,"opened":opened})
    } else {
        Value::Null
    };
    ready(state, d).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        state,
        d,
        name,
        "initial",
        initial,
        &mut stages,
        &mut captures,
    )
    .await?;
    match case().as_str() {
        "selection" => lanes::selection(state, d, name, &mut stages, &mut captures).await?,
        "validation" => lanes::validation(state, d, name, &mut stages, &mut captures).await?,
        "failure" => lanes::failure(state, d, name, &mut stages, &mut captures).await?,
        "retirement" => lanes::retirement(state, d, name, &mut stages, &mut captures).await?,
        "unified" => lanes::unified_lane(state, d, name, &mut stages, &mut captures).await?,
        _ => return Err("Unknown multi case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&state.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
