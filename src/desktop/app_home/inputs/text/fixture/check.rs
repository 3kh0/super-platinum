use super::*;
use crate::app_home::inputs::external::fixture::capture;
use dioxus::desktop::DesktopContext;
pub(crate) mod helpers;
mod lanes;
use helpers::*;
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &name)).await {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":error,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":case(),"error":"Text dispatch fixture timeout","model":snapshot(&s.peek())})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("app-home-text-dispatch-check: {result}")
        })
        .await;
    });
}
async fn run(s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    settle(s, d).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(s,d,n,"initial",json!({"homeOpenedFixtureAdapter":true,"inputEventAdapter":"native DOM listeners; synthetic fixture events"}),&mut stages,&mut captures).await?;
    match case().as_str() {
        "admission" => admission::run(s, d, n, &mut stages, &mut captures).await?,
        "events" => lanes::events(s, d, n, &mut stages, &mut captures).await?,
        "state" => lanes::state(s, d, n, &mut stages, &mut captures).await?,
        "lifecycle" => lanes::lifecycle(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown text case".into()),
    }
    Ok(
        json!({"ok":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0,"visualReview":"Individual original PNG review required"}),
    )
}
