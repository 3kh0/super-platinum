//! Real row/Close/Retry interactions; retained setup has no duplicate screenshots.
use super::*;
use crate::channel_workflows::details::fixture as server;
mod rate;
const A: &str = "channel-workflow-open-featured-0";
const B: &str = "channel-workflow-open-bookmarked-0";

async fn settled(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    status: &str,
    old: Option<u64>,
) -> Result<(), String> {
    let script = format!(
        r#"(()=>{{const e=document.getElementById('channel-workflow-details');let p;try{{p=JSON.parse(e?.dataset.fixtureReadback)}}catch{{return{{ok:false}}}}return{{ok:p.current===true&&p.status==={}&&({}===null||p.selection.id!=={}),product:p}}}})()"#,
        json!(status),
        json!(old),
        json!(old)
    );
    evidence::observed_wait(state, desktop, visit, "details original state", &script)
        .await
        .map(|_| ())
}
async fn open(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    button: &str,
    status: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let old = before["model"]["workflowDetails"]["selection"]["id"].as_u64();
    let tab = &before["native"]["workflows"];
    let opener = tab["sections"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["rows"].as_array().into_iter().flatten())
        .map(|r| &r["opener"])
        .find(|r| r["id"] == button)
        .ok_or("Original available opener missing")?;
    let expected = json!({"id":button,"nodeId":opener["nodeId"],"owner":tab["product"]["owner"],"serial":tab["product"]["serial"]});
    let script = include_str!("details/open.js")
        .replace("EXPOSURE", include_str!("details/exposure.js"))
        .replace("EXPECTED", &expected.to_string());
    let activation = evidence::observed_wait(
        state,
        desktop,
        visit,
        "exposed original workflow row",
        &script,
    )
    .await?;
    settled(state, desktop, visit, status, old).await?;
    if status == "loading" {
        until(
            state,
            visit,
            "actual original details held receiver",
            |_, s| {
                let ledger = &s["channelWorkflows"]["details"];
                let request = ledger["requests"].as_array().and_then(|v| v.last());
                request.is_some_and(|r| {
                    ledger["held"].as_array().is_some_and(|v| {
                        v.iter()
                            .any(|h| h["serial"] == r["serial"] && h["receiverRetired"] == false)
                    })
                })
            },
        )
        .await?;
    }
    let after = observe(state, desktop, visit).await?;
    Ok(json!({"action":button,"activation":activation,"before":before,"after":after}))
}
async fn retry(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    status: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let serial = before["model"]["workflowDetails"]["serial"]
        .as_u64()
        .ok_or("Original details serial missing")?;
    click(desktop, "channel-workflow-details-retry").await?;
    evidence::observed_wait(state,desktop,visit,"details Retry advanced original serial",&format!(r#"(()=>{{let p;try{{p=JSON.parse(document.getElementById('channel-workflow-details')?.dataset.fixtureReadback)}}catch{{return{{ok:false}}}}return{{ok:p.serial>{serial}&&p.status==={},product:p}}}})()"#,json!(status))).await?;
    let after = observe(state, desktop, visit).await?;
    Ok(json!({"action":"channel-workflow-details-retry","before":before,"after":after}))
}
async fn close(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let button = before["native"]["details"]["opener"]["id"]
        .as_str()
        .ok_or("Original details opener missing")?;
    click(desktop, "channel-workflow-details-close").await?;
    evidence::observed_wait(state,desktop,visit,"original details Close and focus",&format!(r#"({{ok:!document.getElementById('channel-workflow-details')&&document.activeElement===document.getElementById({})}})"#,json!(button))).await?;
    let ledger = server::capture(&state.peek()).map_err(|_| "Original details ledger missing")?;
    let held = ledger.snapshot()["held"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for entry in held {
        retired(
            &ledger,
            entry["serial"].as_u64().ok_or("Held serial missing")? as usize,
        )
        .await?;
    }
    let after = observe(state, desktop, visit).await?;
    Ok(json!({"action":"channel-workflow-details-close","before":before,"after":after}))
}
async fn retired(model: &server::Model, serial: usize) -> Result<(), String> {
    for _ in 0..100 {
        if model.snapshot()["held"].as_array().is_some_and(|v| {
            v.iter()
                .any(|r| r["serial"] == json!(serial) && r["receiverRetired"] == true)
        }) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original details receiver did not retire: {}",
        model.snapshot()
    ))
}
async fn late(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    model: &server::Model,
    serial: usize,
) -> Result<Value, String> {
    retired(model, serial).await?;
    let before = observe(state, desktop, visit).await?;
    let release = model.release(serial)?;
    if release["delivered"] != false || release["receiverRetired"] != true {
        return Err("Original retired details reply delivered".into());
    }
    let after = observe(state, desktop, visit).await?;
    for key in ["model", "firstPublish", "native"] {
        if before[key] != after[key] {
            return Err(format!("Late details changed successor {key}"));
        }
    }
    let q = quiet(state, desktop, visit).await?;
    Ok(json!({"before":before,"release":release,"after":after,"quiet":q}))
}
async fn private(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let activation=evidence::observed_wait(state,desktop,visit,"inert original private workflow",r#"(()=>{const e=document.querySelector('#channel-workflows-featured [data-trigger-id="FtDenied01"]'),r=window.__firstPublishRegistry;if(!e||e.querySelector('button,a,input,select'))return{ok:false};e.scrollIntoView({block:'center'});const b=e.getBoundingClientRect(),h=document.elementFromPoint(b.left+b.width/2,b.top+b.height/2);if(h!==e&&!e.contains(h))return{ok:false};e.click();return{ok:true,nodeId:r.ids.get(e),actions:0,trigger:e.dataset.triggerId}})()"#).await?;
    let q = quiet(state, desktop, visit).await?;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err("Inert private row changed original state".into());
    }
    Ok(json!({"before":before,"activation":activation,"after":after,"quiet":q}))
}
struct Scene<'a> {
    state: Signal<ShellState>,
    desktop: &'a DesktopContext,
    name: &'a str,
    visit: u64,
    captures: &'a mut serde_json::Map<String, Value>,
    stages: &'a mut Vec<Value>,
}
impl Scene<'_> {
    async fn stage(&mut self, label: &str, exercise: Value) -> Result<(), String> {
        stage(
            self.state,
            self.desktop,
            self.name,
            label,
            self.visit,
            exercise,
            self.captures,
            self.stages,
        )
        .await
    }
    async fn open(&mut self, label: &str, button: &str, status: &str) -> Result<(), String> {
        let e = open(self.state, self.desktop, self.visit, button, status).await?;
        self.stage(label, e).await
    }
    async fn close(&mut self, label: &str) -> Result<(), String> {
        let e = close(self.state, self.desktop, self.visit).await?;
        self.stage(label, e).await
    }
    async fn retry(&mut self, label: &str, status: &str) -> Result<(), String> {
        let e = retry(self.state, self.desktop, self.visit, status).await?;
        self.stage(label, e).await
    }
    async fn late(
        &mut self,
        label: &str,
        model: &server::Model,
        serial: usize,
    ) -> Result<(), String> {
        let e = late(self.state, self.desktop, self.visit, model, serial).await?;
        self.stage(label, e).await
    }
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    admission: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    workflows::settled(desktop, "ready").await?;
    let model = server::capture(&state.peek()).map_err(|_| "Original details ledger missing")?;
    let mut scene = Scene {
        state,
        desktop,
        name,
        visit,
        captures,
        stages,
    };
    let private = private(state, desktop, visit).await?;
    scene
        .stage("browse", json!({"admission":admission,"private":private}))
        .await?;
    match endpoint::case().as_str() {
        "first-details-success" => {
            scene.open("details-a", A, "ready").await?;
            scene.close("closed").await?;
            scene.open("details-b", B, "ready").await?;
            scene.close("closed-b").await?;
        }
        "first-details-errors" => {
            scene.open("denied", A, "failed").await?;
            scene.retry("not-found", "failed").await?;
            scene.retry("failed", "failed").await?;
            scene.retry("recovered", "ready").await?;
            scene.close("closed").await?;
        }
        "first-details-close-held" => {
            scene.open("info-held", A, "loading").await?;
            scene.close("closed-info").await?;
            scene.late("late-info", &model, 1).await?;
            scene.open("get-held", A, "loading").await?;
            scene.close("closed-get").await?;
            scene.late("late-get", &model, 3).await?;
        }
        "first-details-supersession" => {
            scene.open("a-held", A, "loading").await?;
            scene.open("b-ready", B, "ready").await?;
            scene.late("late-a", &model, 1).await?;
            scene.open("b-held", B, "loading").await?;
            let before = observe(state, desktop, visit).await?;
            workflows::sidebar(desktop, "private-review").await?;
            evidence::observed_wait(state,desktop,visit,"actual channel successor",r#"({ok:document.getElementById('channel-details-trigger')?.textContent==='#private-review'&&!document.getElementById('channel-workflow-details')&&!document.getElementById('channel-workflows-panel')&&!!document.getElementById('channel-composer')})"#).await?;
            retired(&model, 5).await?;
            let after = observe(state, desktop, visit).await?;
            scene
                .stage("channel-successor", json!({"before":before,"after":after}))
                .await?;
            scene.late("late-b", &model, 5).await?;
        }
        "first-details-account-retirement" => {
            scene.open("held", A, "loading").await?;
            let before = observe(state, desktop, visit).await?;
            let replacement = replace_account(state)?;
            evidence::observed_wait(state,desktop,visit,"actual account successor",r#"({ok:!document.getElementById('channel-workflow-details')&&!document.getElementById('channel-workflows-panel')&&!document.getElementById('workflow-finish-up')})"#).await?;
            retired(&model, 1).await?;
            let after = observe(state, desktop, visit).await?;
            scene
                .stage(
                    "successor",
                    json!({"before":before,"after":after,"replacement":replacement}),
                )
                .await?;
            scene.late("late-settled", &model, 1).await?;
            return Ok(replacement);
        }
        "first-details-rate-limit" => rate::run(&mut scene).await?,
        _ => return Err("Unrecognized closed details case".into()),
    }
    let exercise = workflows::return_document(state, desktop, visit).await?;
    scene.stage("returned", exercise).await?;
    Ok(Value::Null)
}
