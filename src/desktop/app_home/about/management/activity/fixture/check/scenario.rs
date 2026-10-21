use super::*;
#[path = "emoji.rs"]
mod emoji;
const REFUSAL: &str =
    "Finish editing the current text before opening Activity. Your draft is unchanged.";
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    capture(state, desktop, name, label, proof, stages, captures).await
}
fn count(state: Signal<ShellState>) -> usize {
    snapshot(&state.peek())["ledger"]["requests"]
        .as_array()
        .unwrap()
        .len()
}
async fn settled(state: Signal<ShellState>, label: &str, after: usize) -> Result<Value, String> {
    wait(state, label, |v| {
        v["metadata"]["busy"] == false
            && v["ledger"]["requests"]
                .as_array()
                .is_some_and(|r| r.len() > after)
    })
    .await
}
async fn edit(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "accepted exact metadata draft", exact).await?;
    Ok(())
}
async fn refusal(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    composing: bool,
) -> Result<Value, String> {
    let before = snapshot(&state.peek());
    let revision = before["activity"]["documentRevision"]
        .as_u64()
        .ok_or("Missing document revision")?;
    let probe = if composing {
        "document.getElementById('workflow-management-title').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'未確定'}));".to_owned()
    } else {
        format!(
            "Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(document.getElementById('workflow-management-title'),{});",
            json!("Uncommitted native title 未確定")
        )
    };
    let expected = if composing {
        TITLE
    } else {
        "Uncommitted native title 未確定"
    };
    let probed=wait_dom(desktop,&format!(r#"(()=>{{const s=document.getElementById('workflow-management')?.__workflowManagement;if(!s)return{{ok:false}};{probe}const t=document.getElementById('workflow-management-title');return{{ok:t.value==={},value:t.value,refusals:s.activityRefusals??0,composing:s.composing}}}})()"#,json!(expected))).await?;
    let refusals = probed["refusals"]
        .as_u64()
        .ok_or("Missing native refusal counter")?;
    click(desktop, "workflow-management-activity").await?;
    let dom=wait_dom(desktop,&format!(r#"(()=>{{const t=document.getElementById('workflow-management-title'),s=document.getElementById('workflow-management')?.__workflowManagement,n=document.getElementById('workflow-management-navigation-notice');return{{ok:!!t&&t.value==={}&&!document.getElementById('workflow-activity')&&s?.activityRefusals==={}&&n?.textContent==={},value:t?.value,composing:s?.composing,refusals:s?.activityRefusals,notice:n?.textContent}}}})()"#,json!(expected),refusals+1,json!(REFUSAL))).await?;
    let after = snapshot(&state.peek());
    if after != before
        || !exact(&after)
        || after["metadata"]["notice"] != Value::Null
        || after["metadata"]["busy"] != false
        || after["metadata"]["writes"] != 0
    {
        return Err(format!(
            "Refused navigation mutated stored state: before={before}, after={after}"
        ));
    }
    Ok(
        json!({"uncommitted":!composing,"composition":composing,"probe":probed,"native":dom,"beforeRefusals":refusals,"afterRefusals":dom["refusals"],"beforeRevision":revision,"afterRevision":after["activity"]["documentRevision"],"sameRevision":true,"unchangedRustState":true,"noActivityRead":true,"draftRetained":true}),
    )
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let scenario = case();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-configuration');if(!e||document.getElementById('workflow-management'))return{ok:false};e.scrollIntoView({block:'nearest'});return{ok:true}})()").await?;
    stage(
        state,
        desktop,
        name,
        "about",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "Configuration editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    edit(state, desktop).await?;
    stage(
        state,
        desktop,
        name,
        "draft",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    if scenario == "retirement" {
        click(desktop, "workflow-management-save").await?;
        wait(state, "uncertain metadata outcome retained", |v| {
            exact(v)
                && v["metadata"]["pending"] == true
                && v["metadata"]["observed"] == true
                && v["metadata"]["busy"] == false
                && v["metadata"]["writes"] == 2
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "unconfirmed",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        let proof = refusal(state, desktop, false).await?;
        stage(
            state,
            desktop,
            name,
            "uncommitted-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        wait_dom(desktop,&format!("(()=>{{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(document.getElementById('workflow-management-title'),{});return{{ok:true}}}})()",json!(TITLE))).await?;
        let proof = refusal(state, desktop, true).await?;
        stage(
            state,
            desktop,
            name,
            "composition-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        wait_dom(desktop,"(()=>{document.getElementById('workflow-management-title').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'未確定'}));return{ok:true}})()").await?;
    }
    let before = count(state);
    click(desktop, "workflow-management-activity").await?;
    settled(state, "Activity initial page", before).await?;
    wait_dom(
        desktop,
        "({ok:!!document.getElementById('workflow-activity-run-E_REVIEW')})",
    )
    .await?;
    stage(
        state,
        desktop,
        name,
        "list",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    let mut replacement = Value::Null;
    if scenario == "retirement" {
        mode(&state.peek(), Mode::Held)?;
        click(desktop, "workflow-activity-apply").await?;
        wait(state, "held Activity read", |v| {
            v["ledger"]["held"].is_u64() && v["metadata"]["busy"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "held",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        replacement = replace_account(state)?;
        wait(
            state,
            "old request cancelled after account replacement",
            |v| v["ledger"]["cancelled"] == 1,
        )
        .await?;
        let release = release(&state.peek())?;
        if release["cancelledBeforeReply"] != true {
            return Err("Expected ownership cancellation before synthetic late reply".into());
        }
        let s = state.peek();
        let retained = s.app_home.about.management.documents.iter().any(|d| {
            d.pending.is_some() && d.title == TITLE && d.description == DESCRIPTION && d.dirty()
        });
        drop(s);
        if !retained
            || replacement["foreignDocumentHidden"] != true
            || replacement["retainedPending"] != 1
        {
            return Err("Uncertain metadata custody lost on account replacement".into());
        }
        replacement["retainedDraftExact"] = json!(retained);
        replacement["lateReply"] = release;
        wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-configuration');if(!e||document.getElementById('workflow-activity')||document.getElementById('workflow-management'))return{ok:false};e.scrollIntoView({block:'nearest'});return{ok:true}})()").await?;
        stage(
            state,
            desktop,
            name,
            "account-replaced",
            replacement.clone(),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        input(desktop, "workflow-activity-status", "SUCCESS").await?;
        input(desktop, "workflow-activity-limit", "20").await?;
        let before = count(state);
        date_range::select(state, desktop, "2026-06-15", "2026-06-16").await?;
        let accepted = wait(state, "accepted exact Activity filters", |v| {
            let f = &v["activity"]["filters"];
            f["status"] == "SUCCESS"
                && f["from"] == "2026-06-15"
                && f["to"] == "2026-06-16"
                && f["limit"] == "20"
        })
        .await?;
        wait_dom(desktop, &format!("({{ok:document.getElementById('workflow-activity-filters')?.dataset.revision===String({})}})", accepted["activity"]["filters"]["revision"])).await?;
        let dates=wait_dom(desktop,"(()=>{const from=new Date(2026,5,15),to=new Date(2026,5,16,23,59,59);return{ok:true,from:Math.floor(from.getTime()/1000),to:Math.floor(to.getTime()/1000)}})()").await?;
        let filtered = settled(state, "filtered Activity page", before).await?;
        let q = &filtered["activity"]["query"];
        if q["limit"] != 20
            || q["filters"]["workflow_instance_status"] != "SUCCESS"
            || q["filters"]["from_timestamp"] != dates["from"]
            || q["filters"]["to_timestamp"] != dates["to"]
        {
            return Err(format!(
                "Actual local dates/filters mismatch: {q} vs {dates}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            "filtered",
            dates,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-next").await?;
        settled(state, "cursor next", before).await?;
        stage(
            state,
            desktop,
            name,
            "next",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-previous").await?;
        settled(state, "cursor previous", before).await?;
        stage(
            state,
            desktop,
            name,
            "previous",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-run-E_REVIEW").await?;
        settled(state, "run detail", before).await?;
        wait_dom(
            desktop,
            "({ok:!!document.getElementById('workflow-activity-detail')})",
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "detail",
            json!({"emoji":emoji::inspect(desktop).await?}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-close-detail").await?;
        wait_dom(desktop,"({ok:!document.getElementById('workflow-activity-detail')&&!!document.getElementById('workflow-activity-children-E_REVIEW')})").await?;
        let before = count(state);
        click(desktop, "workflow-activity-children-E_REVIEW").await?;
        settled(state, "nested execution", before).await?;
        stage(
            state,
            desktop,
            name,
            "nested",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-all").await?;
        settled(state, "return to all runs", before).await?;
        mode(&state.peek(), Mode::Malformed)?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        settled(state, "malformed response retained error", before).await?;
        // Reveal the actual outside error feedback before capturing it.
        click(desktop, "workflow-activity-filter").await?;
        wait_dom(desktop,"({ok:document.getElementById('workflow-activity-filter')?.getAttribute('aria-expanded')==='false'&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
        stage(
            state,
            desktop,
            name,
            "error",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        settled(state, "explicit retry", before).await?;
        stage(
            state,
            desktop,
            name,
            "retried",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        mode(&state.peek(), Mode::Empty)?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        settled(state, "empty page", before).await?;
        stage(
            state,
            desktop,
            name,
            "empty",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-back").await?;
        wait(state, "metadata draft returned", |v| {
            exact(v) && v["activity"]["active"] == false && v["metadata"]["writes"] == 0
        })
        .await?;
        wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
        stage(
            state,
            desktop,
            name,
            "returned",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":scenario,"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"replacement":replacement,"final":snapshot(&state.peek())}),
    )
}
