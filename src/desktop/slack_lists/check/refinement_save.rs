//! Actual local Apply, metadata Save and independent ACK-child fixture workflow.
use super::*;
use crate::slack_lists::rename::{self, fixture as rename_fixture};
use rename_fixture::refinement as adapter;
mod harness;
mod model;
mod oracle;
mod workflow;
use model::Proof;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !adapter::check_enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => {
                json!({"ok":false,"error":error,"failedState":adapter::state(&state.peek()).ok(),"failedLedger":adapter::snapshot(&state.peek()).ok(),"failedProjection":model::projection(state).ok()})
            }
        };
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(
                    format!("slack-list-board-refinement-save-check: {receipt}\n").as_bytes(),
                )
                .expect("Refinement terminal output failed");
        })
        .await
        .expect("Refinement terminal writer failed");
    });
}
async fn wait_state(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&Value, &Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..150 {
        let raw = adapter::state(&state.peek())?;
        let ledger = adapter::snapshot(&state.peek())?;
        if test(&raw, &ledger) {
            return Ok(json!({"state":raw,"ledger":ledger}));
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err(format!("Refinement did not reach {label}"))
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "window.__listRefinementSave.click({})",
            json!(format!("#{id}"))
        ),
    )
    .await
}
fn serial(state: Signal<ShellState>) -> Result<u64, String> {
    rename::presentation(&state.peek())
        .map(|p| p.serial)
        .ok_or("Missing refinement editor".into())
}
async fn bind(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    kind: &str,
) -> Result<Value, String> {
    wait_dom(
        d,
        &format!(
            "window.__listRefinementSave.bindPanel({})",
            json!({"kind":kind,"serial":serial(state)?})
        ),
    )
    .await
}
async fn accept(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    attempt: u64,
) -> Result<Value, String> {
    let raw = adapter::state(&state.peek())?;
    if raw["selected"] != "V_REFINED_CREATED" {
        return Err("No actual new selection".into());
    }
    wait_dom(d,&format!("window.__listRefinementSave.acceptSelection({})",json!({"serial":attempt,"visit":raw["visit"],"selection":raw["selection"],"id":raw["selected"]}))).await
}
async fn readiness(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut previous = None;
    for _ in 0..150 {
        if let Ok(now) = model::projection(state) {
            if previous.as_ref() == Some(&now) {
                let dom = wait_dom(d, "window.__listRefinementSave.ready()").await?;
                if model::projection(state)? == now {
                    return Ok(json!({"projection":now,"dom":dom,"stableSamples":2}));
                }
            }
            previous = Some(now);
        } else {
            previous = None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    Err("Refinement setup never stabilized".into())
}
async fn drag(d: &dioxus::desktop::DesktopContext, owner: &Value) -> Result<Value, String> {
    let started = checked(
        d,
        &include_str!("refinement_save/drag_bridge.js").replace("__OWNER__", &owner.to_string()),
    )
    .await?;
    if started["owner"] != *owner || started["started"] != true {
        return Err("Drag bridge start owner mismatch".into());
    }
    let settled = wait_dom(d, "window.__listRefinementDragSettlement.read()").await?;
    if settled["owner"] != *owner
        || settled["state"] != "complete"
        || settled["result"]["ok"] != true
    {
        return Err(format!("Async drag did not settle successfully: {settled}"));
    }
    Ok(json!({"started":started,"settled":settled}))
}
async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let owner = harness::wait_runner(state).await?;
    checked(d, include_str!("refinement_save.js")).await?;
    let ready = readiness(state, d).await?;
    let setup = adapter::initial_readiness(&state.peek())?;
    let mode = adapter::case().ok_or("No refinement fixture mode")?;
    if setup["state"]["find"] != json!({"input":"","applied":"","revision":0}) {
        return Err("Initial Find was not empty".into());
    }
    let query = checked(d, "window.__listRefinementSave.query()").await?;
    wait_state(state, "Find debounce", |s, _| {
        s["find"]["input"] == "Launch item 0"
            && s["find"]["applied"] == "Launch item 0"
            && s["find"]["revision"].as_u64().is_some_and(|n| n > 0)
    })
    .await?;
    let first_find = checked(d, "window.__listRefinementSave.bindFind()").await?;
    let raw = adapter::state(&state.peek())?;
    let mut proof = Proof {
        mode,
        original: raw["document"].clone(),
        population: raw["population"].clone(),
        find: raw["find"].clone(),
        expanded: model::projection(state)?["expanded"].clone(),
        stages: vec![],
        captures: Default::default(),
    };
    proof.stage(state, d, "baseline").await?;
    let drag = drag(d, &owner).await?;
    wait_state(state, "local provider", |s, l| {
        s["provider"] == json!([{"select":[]},{"select":["OPT_DONE"]},{"select":["OPT_PROGRESS"]}])
            && s["localEmpty"] == true
            && l["requests"] == json!([])
    })
    .await?;
    proof.stage(state, d, "local").await?;
    let fields = workflow::fields(&mut proof, state, d).await?;
    let save_open = click(
        d,
        if mode.created() {
            "slack-list-refinement-save-new"
        } else {
            "slack-list-refinement-save"
        },
    )
    .await?;
    let attempt = serial(state)?;
    let binding = bind(state, d, if mode.created() { "name" } else { "save" }).await?;
    let name = if mode.created() {
        let input = checked(d, "window.__listRefinementSave.name('Focused delivery')").await?;
        wait_state(state, "name editing", |s, _| {
            s["stage"] == "Editing" && s["name"] == "Focused delivery"
        })
        .await?;
        proof.stage(state, d, "name").await?;
        let confirm = click(d, "slack-list-rename-confirm").await?;
        json!({"input":input,"confirm":confirm})
    } else {
        Value::Null
    };
    wait_state(state, "held metadata Save", |s, l| {
        s["stage"] == "Saving" && l["writePending"] == true
    })
    .await?;
    proof.stage(state, d, "saving").await?;
    let settlement = workflow::settle(&mut proof, state, d, attempt).await?;
    let final_ledger = adapter::snapshot(&state.peek())?;
    let final_state = adapter::state(&state.peek())?;
    if final_state["stage"] != Value::Null {
        return Err("Final review retained authority".into());
    }
    let history = checked(d, "window.__listRefinementSave.history()").await?;
    proof.stable(state, "board")?;
    if adapter::snapshot(&state.peek())? != final_ledger
        || adapter::state(&state.peek())? != final_state
    {
        return Err("Final history changed state/ledger".into());
    }
    Ok(
        json!({"ok":true,"schema":1,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"mode":oracle::mode(mode),"owner":owner,"noSession":true,"noTransport":true,"original":proof.original,"population":proof.population,"find":proof.find,"ready":ready,"setup":setup,"query":query,"firstFind":first_find,"drag":drag,"fields":fields,"save":{"open":save_open,"binding":binding,"attempt":attempt,"name":name},"settlement":settlement,"history":history,"stages":proof.stages,"captures":proof.captures,"ledger":final_ledger,"finalState":final_state}),
    )
}
