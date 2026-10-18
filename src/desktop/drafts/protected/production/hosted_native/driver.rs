//! Real picker, Editor, source guard and Send; fixed disconnected response only.
use super::*;
use std::time::Duration;
const NEWER: &str = "Newer native caption 日本語 e\u{301}";
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Native predicate: {value}")),
        _ => Err("Native evaluation timed out".into()),
    }
}
async fn dom(operation: &str, args: Value) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await hostedComposerCheck({},{}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",include_str!("dom.js"),json!(operation),args)).await
}
async fn input(operation: &str, text: Option<&str>) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",include_str!("../../../projection_check.js"),json!(operation),json!({"id":"channel-composer","text":text,"input":true}))).await
}
fn editor(run: &Run) -> Result<Arc<editor::Editor>, String> {
    root()
        .lock()
        .map_err(|_| "Root lock")?
        .host()
        .installation(&run.account, &run.fixture.scope().key)
        .map(|installed| installed.editor().clone())
        .ok_or("Original Editor missing".into())
}
fn actor(state: Signal<ShellState>, run: &Run) -> Result<Value, String> {
    let shell = state.peek();
    if !run.current(&shell) {
        return Err("Original configured owner changed".into());
    }
    let snapshot = super::super::hosted::snapshot(&shell, ComposerTarget::Channel).map(|s| {
        json!({
        "sources":s.selection.sources(),"ids":s.selection.ids(),"version":s.version,
        "saved":s.saved,"loaded":s.loaded,"sending":s.sending,"completed":s.completed,
        "canRetire":s.can_retire,"retryRetirement":s.retry_retirement,"retryLoad":s.retry_load,
        "retrySaved":s.retry_saved,"retryResult":s.retry_result})
    });
    Ok(json!({"ownerCurrent":true,
        "sourceCurrent":socket::capture(&shell,"T1").is_ok_and(|c|c.current(&shell)),
        "senderBusy":shell.draft_writes.sending.contains(&run.fixture.scope().key),
        "shellText":shell.core.composer.text,"hosted":snapshot,
        "editor":editor(run).ok().map(|e|json!({"serial":e.snapshot().serial,"local":e.snapshot().view.local()}))}))
}
async fn wait_actor(
    state: Signal<ShellState>,
    run: &Run,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..400 {
        last = actor(state, run)?;
        if predicate(&last) {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Actor did not settle: {last}"))
}
async fn wait_dom(predicate: impl Fn(&Value) -> bool) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..250 {
        if let Ok(value) = dom("read", json!({})).await {
            last = value;
            if predicate(&last) {
                return Ok(last);
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("DOM did not settle: {last}"))
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    run: &Run,
    stage: &str,
) -> Result<Value, String> {
    let checked = run.config.clone();
    tokio::task::spawn_blocking(move || isolation::validate(&checked))
        .await
        .map_err(|_| "Capture owner validation task lost")??;
    let observed = actor(state, run)?;
    let ui = dom("read", json!({})).await?;
    eprintln!(
        "protected-hosted-composer-stage: {}",
        json!({"stage":stage,"edge":"before","nonce":run.config.nonce})
    );
    let path = run.config.captures.join(format!("{stage}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    eprintln!(
        "protected-hosted-composer-stage: {}",
        json!({"stage":stage,"edge":"after","nonce":run.config.nonce})
    );
    Ok(
        json!({"stage":stage,"capture":path,"actor":observed,"dom":ui,
        "native":run.native.lock().map_err(|_|"Native evidence lock")?.clone(),"reads":run.fixture.reads(),"request":run.fixture.request()}),
    )
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    run: &Arc<Run>,
    _worker: &mut socket::Worker,
) -> Result<Value, String> {
    let mut ready = false;
    for _ in 0..250 {
        if input("ready", None).await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !ready {
        return Err("Native field never mounted".into());
    }
    input("show", None).await?;
    input("set", Some(run.fixture.caption())).await?;
    let mut stages = vec![capture(state, desktop, run, "initial").await?];
    for (index, id) in ["F00000001", "F00000002"].into_iter().enumerate() {
        dom("attach", json!({})).await?;
        dom("open", json!({})).await?;
        let picker =
            wait_dom(|v| v["picker"]["rows"].as_array().is_some_and(|a| a.len() == 2)).await?;
        if picker["picker"]["insertDisabled"] != true
            || picker["picker"]["query"] != ""
            || picker["picker"]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["selected"] != "false")
        {
            return Err("Picker selected a default or changed the fixed query".into());
        }
        stages.push(capture(state, desktop, run, &format!("picker-{index}")).await?);
        dom("choose", json!({"index":index})).await?;
        wait_dom(|v| {
            v["picker"]["rows"][index]["selected"] == "true"
                && v["picker"]["insertDisabled"] == false
        })
        .await?;
        stages.push(capture(state, desktop, run, &format!("selected-{index}")).await?);
        dom("insert", json!({})).await?;
        wait_actor(state, run, |v| {
            v["hosted"]["saved"] == true
                && v["hosted"]["ids"]
                    .as_array()
                    .is_some_and(|a| a.len() == index + 1 && a[index] == id)
        })
        .await?;
        wait_dom(|v| {
            v["picker"].is_null() && v["chips"].as_array().is_some_and(|a| a.len() == index + 1)
        })
        .await?;
        stages.push(capture(state, desktop, run, &format!("inserted-{index}")).await?);
    }
    dom("reorder", json!({})).await?;
    wait_actor(state, run, |v| {
        v["hosted"]["saved"] == true && v["hosted"]["ids"] == json!(["F00000002", "F00000001"])
    })
    .await?;
    stages.push(capture(state, desktop, run, "ordered").await?);
    dom("send", json!({})).await?;
    for _ in 0..400 {
        if run.response.armed() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !run.response.armed() {
        return Err("Actual Send never armed".into());
    }
    let unlocked = wait_dom(|v| v["field"]["readOnly"] == false).await?;
    if unlocked["field"]["text"] != run.fixture.caption() || run.fixture.request().is_some() {
        return Err("Original field or held request changed".into());
    }
    stages.push(capture(state, desktop, run, "armed-unlocked").await?);
    if run.config.scenario == "changed-current" {
        input("set", Some(NEWER)).await?;
        wait_actor(state, run, |v| v["shellText"] == NEWER).await?;
        editor(run)?
            .flush()
            .await
            .map_err(|e| format!("New caption persistence: {e:?}"))?;
        stages.push(capture(state, desktop, run, "newer-input").await?);
    }
    run.response
        .release()
        .map_err(|_| "Original held response disappeared")?;
    wait_actor(state, run, |v| {
        v["senderBusy"] == false && run.fixture.request().is_some()
    })
    .await?;
    if run.config.scenario == "storage" {
        wait_actor(state, run, |v| v["hosted"]["retryResult"] == true).await?;
        stages.push(capture(state, desktop, run, "returned-storage-failure").await?);
        let original = run.fixture.request().ok_or("No original request")?;
        dom("retry-result", json!({})).await?;
        wait_actor(state, run, |v| {
            v["hosted"]["completed"]
                .as_array()
                .is_some_and(|a| a.len() == 1)
                && v["hosted"]["canRetire"] == true
        })
        .await?;
        if run.fixture.request() != Some(original) {
            return Err("Storage retry changed the original request".into());
        }
        stages.push(capture(state, desktop, run, "returned-storage-saved").await?);
    }
    if matches!(run.config.scenario.as_str(), "storage" | "changed-current") {
        wait_actor(state, run, |v| {
            v["hosted"]["completed"]
                .as_array()
                .is_some_and(|a| a.len() == 1)
                && v["hosted"]["canRetire"] == true
        })
        .await?;
        dom("review-retirement", json!({})).await?;
        stages.push(capture(state, desktop, run, "review-local-removal").await?);
        dom("retire", json!({})).await?;
    }
    if run.config.scenario != "unknown" {
        wait_actor(state, run, |v| {
            v["hosted"]["sending"] == false
                && v["hosted"]["completed"] == json!([])
                && v["hosted"]["ids"] == json!([])
        })
        .await?;
    }
    let final_dom = wait_dom(|v| v["field"]["readOnly"] == false).await?;
    let expected_text = match run.config.scenario.as_str() {
        "unknown" | "storage" => run.fixture.caption(),
        "changed-current" => NEWER,
        _ => "",
    };
    if final_dom["field"]["text"] != expected_text {
        return Err("Native final caption mismatch".into());
    }
    stages.push(capture(state, desktop, run, "result").await?);
    // Only after the original send has settled: an inventory read can retire
    // dispatch activity and is deliberately forbidden while the arm is held.
    let route = run.account.owned.as_ref().ok_or("Owned route missing")?;
    let inventory = route
        .inventory()
        .await
        .map_err(|e| format!("Final inventory: {e:?}"))?;
    let hosted = inventory.hosted();
    if run.config.scenario == "unknown" {
        if hosted.claims().len() != 1 || !hosted.completed().is_empty() {
            return Err("Unknown delivery custody changed".into());
        }
    } else if !hosted.claims().is_empty()
        || !hosted.completed().is_empty()
        || !hosted.drafts().is_empty()
    {
        return Err("Accepted local retirement incomplete".into());
    }
    Ok(
        json!({"ok":true,"version":1,"nonce":run.config.nonce,"scenario":run.config.scenario,
        "account":run.account.id(),"scope":run.fixture.scope(),"stages":stages,
        "inventory":{"drafts":hosted.drafts(),"claims":hosted.claims(),"completed":hosted.completed()},
        "request":run.fixture.request(),"reads":run.fixture.reads(),
        "execution":"actual-configured-editor-source-native-send-closed-oracle","trustedInput":false}),
    )
}
