use super::*;
use capture::{click, dom, type_query, wait};
use dioxus::desktop::DesktopContext;
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
                json!({"ok":false,"fixture":name,"case":case(),"error":"Home channel fixture timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-channel-select-check: {result}")
        })
        .await;
    });
}
async fn run(state: Signal<ShellState>, d: &DesktopContext, name: &str) -> Result<Value, String> {
    let readiness = capture::readiness(name).await?;
    let source = crate::app_home::context::fixture::publish(state, false, "TWORK0002").await?;
    ready(state).await?;
    if case() == "selection" {
        helpers::open(d).await?;
        let model = snapshot(&state.peek());
        let ids = model["query"]["rows"]
            .as_array()
            .ok_or("Missing ordinary choices")?
            .iter()
            .map(|v| v["id"].clone())
            .collect::<Vec<_>>();
        if ids != vec![json!(ALPHA), json!(BETA), json!(GAMMA)]
            || model["mode"]["enterprise"] != false
            || model["context"]["team"] != "TWORK0002"
            || model["channelPolicy"] != json!({})
        {
            return Err("Ordinary context incorrectly restricted channel choices".into());
        }
    }
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
        "retirement" => lanes::retirement(state, d, name, &mut stages, &mut captures).await?,
        _ => return Err("Unknown user case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&state.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
