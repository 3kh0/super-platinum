//! Actual original selectors and result buttons; no settings mutation from Rust.
use super::*;
pub(super) async fn select(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    value: &str,
    copy: bool,
) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled)return{{ok:false}};e.scrollIntoView({{block:'center',behavior:'instant'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}};}})()"#,json!(id),json!(value))).await?;
    until(state, visit, "original selected run/copy access", |d, _| {
        if copy {
            d["confirmation"]["selected"] == value
        } else {
            d["confirmation"]["runDraft"]["mode"] == value
        }
    })
    .await
}
async fn result(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
    kind: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let clicked=wait_dom(desktop,&format!(r#"(()=>{{const panel=document.getElementById('workflow-permissions'),rows=panel?[...panel.querySelectorAll('[data-run-result]')].filter(e=>e.dataset.runResult==={}&&e.dataset.kind==={}):[];if(rows.length!==1||rows[0].disabled)return{{ok:false}};const e=rows[0];e.scrollIntoView({{block:'center',behavior:'instant'}});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};const proof={{ok:true,target:e.dataset.runResult,kind:e.dataset.kind,label:e.textContent,connected:e.isConnected,insideOriginalPanel:panel.contains(e),hit:true}};e.click();return proof;}})()"#,json!(id),json!(kind))).await?;
    until(state, visit, "selected original result", |d, _| {
        d["confirmation"]["runDraft"]["ids"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|v| v == id))
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"] || !draft(&after) {
        return Err("Selecting original result dispatched or lost draft".into());
    }
    Ok(json!({"before":before,"clicked":clicked,"after":after}))
}
pub(super) async fn named(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let initial = observe(state, desktop, visit).await?;
    if initial["permissions"]["confirmation"]["runDraft"]["ids"] != json!(["UWORKFLOW"]) {
        return Err("Named mode did not retain original collaborator".into());
    }
    input(desktop, "workflow-permissions-target-query", "run").await?;
    until(state, visit, "actual internal query pages", |d, _| {
        let v = &d["confirmation"]["runDraft"];
        v["pending"] == false
            && v["query"] == "run"
            && ["URUNADA01", "CRUNTEAM1", "TRUNINNER"].iter().all(|id| {
                v["results"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == id))
            })
    })
    .await?;
    let queried = observe(state, desktop, visit).await?;
    let mut selections = Vec::new();
    for (id, kind) in [
        ("URUNADA01", "User"),
        ("CRUNTEAM1", "Channel"),
        ("TRUNINNER", "Team"),
    ] {
        selections.push(result(state, desktop, visit, id, kind).await?);
    }
    let before_external = observe(state, desktop, visit).await?;
    click(desktop, "workflow-permissions-external").await?;
    until(state, visit, "external choice", |d, _| {
        d["confirmation"]["runDraft"]["externalEnabled"] == true
    })
    .await?;
    let external = observe(state, desktop, visit).await?;
    input(desktop, "workflow-permissions-external-query", "run").await?;
    until(
        state,
        visit,
        "actual available external and query pages",
        |d, _| {
            let v = &d["confirmation"]["runDraft"];
            v["pending"] == false
                && v["externalQuery"] == "run"
                && v["externalResults"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "TRUNEXTER"))
        },
    )
    .await?;
    let external_queried = observe(state, desktop, visit).await?;
    let external_selected = result(state, desktop, visit, "TRUNEXTER", "Team").await?;
    select(
        state,
        desktop,
        visit,
        "workflow-permissions-copy",
        "everyone",
        true,
    )
    .await?;
    let after = observe(state, desktop, visit).await?;
    if after["permissions"]["confirmation"]["runDraft"]["ids"]
        != json!([
            "UWORKFLOW",
            "URUNADA01",
            "CRUNTEAM1",
            "TRUNINNER",
            "TRUNEXTER"
        ])
    {
        return Err("Named result category/order changed".into());
    }
    Ok(
        json!({"initial":initial,"queried":queried,"selections":selections,"beforeExternal":before_external,"external":external,"externalQueried":external_queried,"externalSelected":external_selected,"after":after}),
    )
}
