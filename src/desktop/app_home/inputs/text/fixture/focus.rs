//! Focus-once native continuation and explicit intent ownership.
use super::*;
use crate::app_home::inputs::external::fixture::capture;
use dioxus::desktop::DesktopContext;
mod lanes;
pub(crate) fn named() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-text-focus" | "app-home-text-focus-narrow")
        )
        && std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_FOCUS_CHECK").as_deref() == Ok("1")
        && matches!(case().as_str(), "continuity" | "ownership")
}
pub(crate) fn case() -> String {
    std::env::var("SUPER_PLATINUM_APP_HOME_TEXT_FOCUS_CASE").unwrap_or_default()
}
pub(crate) fn install(s: &mut ShellState) {
    super::install(s);
    let h = s.app_home.surface.as_mut().unwrap();
    let mut raw = h.view.as_ref().unwrap().raw().clone();
    raw["blocks"][1]["element"]["initial_value"] = json!("alpha beta");
    h.view = Some(View::decode(h.home.as_ref().unwrap(), raw).unwrap());
    h.revision = next();
    inputs::hydrate(s);
}
pub(crate) fn start(s: Signal<ShellState>, d: DesktopContext) {
    if !named() {
        return;
    }
    let n = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(Duration::from_secs(240), run(s, &d, &n)).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"harnessCompleted":false,"fixture":n,"case":case(),"error":e,"model":snapshot(&s.peek())})
            }
            Err(_) => {
                json!({"ok":false,"harnessCompleted":false,"fixture":n,"case":case(),"error":"Focus fixture timeout"})
            }
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("app-home-text-focus-check: {result}"))
                .await;
    });
}
fn id(block: &str) -> String {
    crate::view::app_modal::field_id(&key(block))
}
async fn read(action: &str, block: &str, arg: Value) -> Result<Value, String> {
    capture::dom(&format!(
        "(()=>{{{}\n{}\nreturn focusProbe({},{},{});}})()",
        capture::ACTION_GUARD,
        include_str!("focus.js"),
        json!(action),
        json!(id(block)),
        arg
    ))
    .await
}
async fn setup(s: Signal<ShellState>, d: &DesktopContext, block: &str) -> Result<Value, String> {
    check::helpers::settle(s, d).await?;
    let v = read("setup", block, json!({})).await?;
    let o = &v["observation"];
    if o["activeIsTarget"] != true
        || o["selectionStart"] != 1
        || o["selectionEnd"] != 4
        || o["selectionDirection"] != "backward"
        || o["readOnly"] != false
    {
        return Err("Exact initial focus/range unavailable".into());
    }
    Ok(v)
}
async fn acquire_held(mut s: Signal<ShellState>, block: &str) -> Result<Value, String> {
    s.write().app_home.text.fixture.as_mut().unwrap().hold_gate = true;
    let event = read("enter", block, json!({})).await?;
    if event["event"]["prevented"] != true {
        return Err("Actual Enter not admitted".into());
    }
    for _ in 0..200 {
        if snapshot(&s.peek())["gateWaiting"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if snapshot(&s.peek())["gateWaiting"] != true {
        return Err("Actual lease gate not reached".into());
    }
    let during = read("read", block, json!({})).await?;
    let o = &during["observation"];
    if o["lease"] != true || o["readOnly"] != true || o["disabled"] != false {
        return Err("Readonly custody not observed".into());
    }
    Ok(json!({"event":event,"during":during}))
}
fn release_gate(s: &mut ShellState) -> Result<(), String> {
    let f = s.app_home.text.fixture.as_mut().unwrap();
    f.hold_gate = false;
    f.gate_waiting = false;
    f.gate
        .take()
        .ok_or("Missing gate")?
        .send(())
        .map_err(|_| "Gate receiver retired".into())
}
async fn wait_release(block: &str) -> Result<Value, String> {
    for _ in 0..200 {
        let v = read("read", block, json!({})).await?;
        let o = &v["observation"];
        if o["lease"] == false
            && o["disabled"] == false
            && o["readOnly"] == false
            && o["seq"] == o["acked"]
        {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Exact native readonly lease did not release".into())
}
fn preserved(before: &Value, after: &Value) -> bool {
    let a = &before["observation"];
    let b = &after["observation"];
    b["activeIsTarget"] == true
        && b["sameNode"] == true
        && b["sameEntry"] == true
        && b["current"] == true
        && b["selectionStart"] == a["selectionStart"]
        && b["selectionEnd"] == a["selectionEnd"]
        && b["selectionDirection"] == a["selectionDirection"]
}
async fn continue_at(s: Signal<ShellState>, block: &str, away: bool) -> Result<Value, String> {
    let v = read(
        if away { "continue-away" } else { "continue" },
        block,
        json!({}),
    )
    .await?;
    if v["continuation"]["performed"] != true {
        return Err(format!("No-refocus continuation refused: {v}"));
    }
    let target = if away { "quiet" } else { block };
    let expected = v["continuation"]["after"]["value"].clone();
    for _ in 0..150 {
        let snap = snapshot(&s.peek());
        if snap["fields"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|r| r["block"] == target && r["literal"] == expected)
        }) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("Continuation did not reach exact field".into())
}
async fn finish_replies(mut s: Signal<ShellState>, requests: usize) -> Result<(), String> {
    let ids = snapshot(&s.peek())["pendingIds"]
        .as_array()
        .ok_or("Missing pending ledger")?
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    for id in ids {
        if !release(&mut s.write(), id, Reply::Accepted) {
            return Err("Held reply missing".into());
        }
    }
    check::helpers::counts(s, requests, requests).await
}
async fn shot(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    name: &str,
    block: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let before = snapshot(&s.peek());
    let code = format!(
        "(()=>{{{}\nreturn focusGeometry({});}})()",
        include_str!("focus.js"),
        json!(id(block))
    );
    let mut g = crate::slack_links_check::wait_dom(d, &code).await?;
    capture::capture_image(d, n, name, &mut g, captures).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err("Focus capture changed Home model".into());
    }
    stages.push(json!({"stage":name,"geometry":g,"model":after,"proof":proof}));
    Ok(())
}
async fn run(mut s: Signal<ShellState>, d: &DesktopContext, n: &str) -> Result<Value, String> {
    let readiness = capture::readiness(n).await?;
    configure(&mut s.write(), Reply::Held);
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    match case().as_str() {
        "continuity" => lanes::continuity(s, d, n, &mut stages, &mut captures).await?,
        "ownership" => lanes::ownership(s, d, n, &mut stages, &mut captures).await?,
        _ => return Err("Unknown focus lane".into()),
    };
    let final_focus = read(
        "finish",
        if case() == "continuity" {
            "characters"
        } else {
            "enter"
        },
        json!({}),
    )
    .await?;
    Ok(
        json!({"ok":true,"harnessCompleted":true,"focusChecksPassed":true,"fixture":n,"case":case(),"readiness":readiness,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"finalFocus":final_focus,"model":snapshot(&s.peek()),"captureChecksPassed":true,"networkCalls":0,"scope":"Synthetic focus-once continuation and explicit intent; no OS typing/window activation/IME proof"}),
    )
}
