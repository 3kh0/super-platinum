//! Actual Add selector, source-backed preflight and original response retirement.
use super::*;
use endpoint::confirmation as server;
mod feature;
pub(super) mod share;
pub(super) fn destination(s: &ShellState) -> Value {
    let b = &s.core.bookmarks;
    json!({"account":s.core.active_account,"accountEpoch":s.core.account_epoch,"team":s.core.active_team,
        "channel":s.core.active_channel,"channelGeneration":s.channel_generation,"tab":format!("{:?}",s.channel_tab),
        "bookmarks":{"scope":b.scope,"generation":b.generation,"loaded":b.loaded,"loading":b.loading,"items":b.items.iter().map(|v|json!({"id":v.id,"channel":v.channel_id,"type":v.kind,"shortcutId":v.shortcut_id,"title":v.title,"link":v.link})).collect::<Vec<_>>()},
        "documents":s.app_home.about.management.documents.iter().map(|d|json!({"id":d.id,"revision":d.revision,"title":d.title,"description":d.description,"pending":d.pending,"editor":d.editor.raw_workflow(),"trigger":d.editor.trigger()})).collect::<Vec<_>>()})
}
fn add(p: &Value) -> &Value {
    &last(p)["confirmationAdd"]
}
pub(super) async fn ready(state: Signal<ShellState>, visit: u64) -> Result<(), String> {
    until(state, visit, "actual Add choices settled", |p, _| {
        add(p)["status"] == "ready" && add(p)["selected"] == ""
    })
    .await
}
async fn choose(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    value: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let epoch = add(&before["firstPublish"])["epoch"]
        .as_u64()
        .ok_or("Original Add epoch absent")?;
    evidence::observed_wait(state,desktop,visit,"original Add selection control",&format!(r#"(()=>{{const e=document.getElementById('workflow-confirmation-channel');if(!e||e.disabled||![...e.options].some(o=>o.value==={value}))return{{ok:false}};e.scrollIntoView({{block:'center',behavior:'instant'}});e.value={value};e.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}};}})()"#,value=json!(value))).await?;
    until(state, visit, "original channel choice event", |p, _| {
        add(p)["epoch"].as_u64().is_some_and(|v| v > epoch)
            && add(p)["selected"] == value
            && add(p)["status"] != "loading"
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    Ok(json!({"before":before,"after":after,"value":value}))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let case = endpoint::case();
    ready(state, visit).await?;
    if share::requested() {
        return share::run(state, desktop, name, visit, captures, stages).await;
    }
    let mut replacement = Value::Null;
    if case == "first-add-no-selection" {
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-finish-done").await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('workflow-finish-up')&&!!document.getElementById('workflow-management-title')})"#).await?;
        let after = observe(state, desktop, visit).await?;
        if before["server"] != after["server"] {
            return Err("Unselected Done dispatched".into());
        }
        stage(
            state,
            desktop,
            name,
            "done",
            visit,
            json!({"before":before,"after":after}),
            captures,
            stages,
        )
        .await?;
        return Ok(replacement);
    }
    let mut exercise = choose(state, desktop, visit, server::raw::DEST).await?;
    if feature::requested() {
        exercise = feature::selected(state, desktop, visit, exercise).await?;
    }
    if case == "first-add-unavailable" {
        if add(&exercise["after"]["firstPublish"])["notice"]
            .as_str()
            .is_none_or(str::is_empty)
        {
            return Err("Actual channel failure missing notice".into());
        }
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "unavailable",
            visit,
            json!({"selection":exercise,"quiet":q}),
            captures,
            stages,
        )
        .await?;
        let selection = choose(state, desktop, visit, "").await?;
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-finish-done").await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('workflow-finish-up')&&!!document.getElementById('workflow-management-title')})"#).await?;
        let after = observe(state, desktop, visit).await?;
        if before["server"] != after["server"] {
            return Err("Unavailable Done dispatched".into());
        }
        stage(
            state,
            desktop,
            name,
            "done",
            visit,
            json!({"selection":selection,"before":before,"after":after}),
            captures,
            stages,
        )
        .await?;
        return Ok(replacement);
    }
    stage(
        state, desktop, name, "selected", visit, exercise, captures, stages,
    )
    .await?;
    let original = server::capture(&state.peek())?;
    let before = observe(state, desktop, visit).await?;
    click(desktop, "workflow-finish-done").await?;
    if case == "first-add-retirement" {
        until(state, visit, "actual original Add held", |p, s| {
            add(p)["operation"]["outcome"] == "in-flight"
                && s["confirmationAdd"]["held"]
                    .as_array()
                    .is_some_and(|v| v.len() == 1)
        })
        .await?;
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "add-held",
            visit,
            json!({"admission":before,"quiet":q}),
            captures,
            stages,
        )
        .await?;
        let held = observe(state, desktop, visit).await?;
        let serial = held["server"]["confirmationAdd"]["held"][0]
            .as_u64()
            .ok_or("Original Add held serial absent")? as usize;
        replacement = replace_account(state)?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-finish-up')})"#).await?;
        let successor = observe(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "successor",
            visit,
            json!({"before":held,"after":successor,"replacement":replacement}),
            captures,
            stages,
        )
        .await?;
        let release = original.release(serial)?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err("Dispatched original Add receiver changed".into());
        }
        until(state, visit, "retired original Add settled", |p, _| {
            add(p)["operation"]["finished"] == true
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if successor["model"] != after["model"]
            || successor["native"] != after["native"]
            || successor["server"]["current"] != after["server"]["current"]
        {
            return Err("Late Add changed successor".into());
        }
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "late-settled",
            visit,
            json!({"before":successor,"release":release,"after":after,"quiet":q}),
            captures,
            stages,
        )
        .await?;
    } else {
        until(state, visit, "original Add finally closed", |p, _| {
            add(p)["operation"]["finished"] == true && last(p)["closed"] == true
        })
        .await?;
        if crate::channel_workflows::fixture::requested() {
            if crate::channel_workflows::details::fixture::requested() {
                return super::details::run(state, desktop, name, visit, before, captures, stages)
                    .await;
            }
            return super::workflows::run(state, desktop, name, visit, before, captures, stages)
                .await;
        }
        if case == "first-add-success" {
            evidence::observed_wait(state,desktop,visit,"post-finally accepted Add destination",r#"(()=>{const p=document.getElementById('channel-workflows-panel'),t=[...document.querySelectorAll('.channel-tabs button')].find(e=>e.textContent.trim()==='Workflows');return{ok:!document.getElementById('workflow-finish-up')&&document.getElementById('channel-details-trigger')?.textContent==='#review-destination'&&!!p&&!['idle','loading'].includes(p.dataset.status)&&t?.getAttribute('aria-pressed')==='true'}})()"#).await?;
        } else {
            evidence::observed_wait(state,desktop,visit,"post-finally retained Add editor",r#"({ok:!document.getElementById('workflow-finish-up')&&!!document.getElementById('workflow-management-title')})"#).await?;
        }
        let featured = if feature::requested() {
            feature::added(state, desktop, visit).await?
        } else {
            Value::Null
        };
        let after = observe(state, desktop, visit).await?;
        let label = match case.as_str() {
            "first-add-success" => "added",
            "first-add-duplicate" => "duplicate",
            "first-add-rejected" => "rejected",
            _ => return Err("Unknown Add case".into()),
        };
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            label,
            visit,
            json!({"before":before,"after":after,"quiet":q,"feature":featured}),
            captures,
            stages,
        )
        .await?;
    }
    Ok(replacement)
}
