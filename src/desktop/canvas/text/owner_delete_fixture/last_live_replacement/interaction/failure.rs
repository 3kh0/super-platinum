use super::*;
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    let variant = case();
    open(desktop, "neighbor", true).await?;
    if matches!(variant.as_str(), "missing" | "mutated") {
        let source = snapshot(&state.peek());
        let action = save(desktop).await?;
        wait(desktop,r#"({ok:document.querySelector('#canvas-text-editor > [role=alert]')?.textContent==='Fixture preflight changed.'&&!document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
        let wrote = !fixture(&mut state.write()).writes.is_empty();
        if !same_source(&state.peek(), &source) || wrote || recovery::stored().await?.is_some() {
            return Err("Retained endpoint preflight refusal changed custody".into());
        }
        record
            .shot(
                state,
                desktop,
                "preflight-refused",
                editor_layout(desktop).await?,
                action,
            )
            .await?;
        click(desktop, "#canvas-text-cancel").await?;
        wait(
            desktop,
            r#"({ok:!document.getElementById('canvas-text-editor')})"#,
        )
        .await?;
        let action = open(desktop, "neighbor", true).await?;
        record
            .shot(
                state,
                desktop,
                "retry-confirmation",
                editor_layout(desktop).await?,
                action,
            )
            .await?;
    }
    let action = save(desktop).await?;
    reserved(state, desktop).await?;
    record
        .shot(
            state,
            desktop,
            "reserved",
            editor_layout(desktop).await?,
            action,
        )
        .await?;
    let replacement = pair::release(state).await?;
    saving(desktop).await?;
    record
        .shot(
            state,
            desktop,
            "saving",
            editor_layout(desktop).await?,
            json!({"replacement":replacement}),
        )
        .await?;
    let receipt = recovery::stored().await?.ok_or("Missing saved pair")?;
    if variant == "retired" {
        let target = state.peek().canvas.clone().ok_or("No Canvas target")?;
        let old_visit = checklist::presentation(&state.peek()).unwrap().visit;
        state.write().dismiss_overlay();
        wait(
            desktop,
            r#"({ok:!document.querySelector('.canvas-dialog')})"#,
        )
        .await?;
        crate::canvas::open_target(state, target);
        // Reopening keeps the fixture source, but its fresh Reader starts with
        // export-only content until the explicit canonical reload below.
        wait(desktop,r#"({ok:!!document.querySelector('.canvas-dialog')&&!document.getElementById('canvas-text-editor')})"#).await?;
        reload(state).await;
        wait(desktop,r#"({ok:!!document.querySelector('[data-canvas-text-section="neighbor"]')&&!document.getElementById('canvas-text-editor')})"#).await?;
        let current = checklist::presentation(&state.peek()).ok_or("No replacement visit")?;
        if current.visit == old_visit {
            return Err("Closing/reopening did not retire visit".into());
        }
        locked(desktop).await?;
        let before = model(state).await?;
        record
            .shot(
                state,
                desktop,
                "retired",
                check::layout(desktop).await?,
                json!({"oldVisit":old_visit,"newVisit":current.visit}),
            )
            .await?;
        pair::complete(state, "retired").await?;
        let after = model(state).await?;
        if before["canonical"] != after["canonical"]
            || recovery::stored().await?.as_ref() != Some(&receipt)
            || before["pairDispatches"] != after["pairDispatches"]
            || before["view"]["visit"] != after["view"]["visit"]
        {
            return Err("Late pair response attached to replacement visit".into());
        }
        record
            .shot(
                state,
                desktop,
                "late-response-inert",
                check::layout(desktop).await?,
                json!({"before":before,"noCurrentPublication":true}),
            )
            .await?;
        return Ok(json!({"noReplay":true,"lateResponseUnattached":true}));
    }
    let attempt = pair::complete(state, &variant).await?;
    pair::read_and_settle(state, true).await?;
    if recovery::stored().await?.as_ref() != Some(&receipt)
        || checklist::text_ops::settled(&state.peek(), &attempt)
    {
        return Err("Unconfirmed or damaged readback unlocked pair".into());
    }
    wait(desktop,r#"({ok:document.getElementById('canvas-text-save')?.disabled&&document.getElementById('canvas-text-cancel')?.disabled})"#).await?;
    record
        .shot(
            state,
            desktop,
            if variant == "unknown" {
                "unknown-locked"
            } else if variant == "partial" {
                "partial-locked"
            } else {
                "readback-refused"
            },
            editor_layout(desktop).await?,
            json!({"exactReceiptRetained":true}),
        )
        .await?;
    Ok(
        json!({"noReplay":true,"exactReceiptRetained":true,"pairedDispatches":1,"awaitFreshProcess":variant=="unknown"}),
    )
}
async fn locked(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-paragraph-actions button')];return{ok:b.length>0&&b.every(b=>b.disabled)}})()"#).await?;
    Ok(())
}
pub(in crate::canvas::text::owner_delete_fixture::last_live_replacement) async fn restart(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    record: &mut Record,
) -> Result<Value, String> {
    let receipt = recovery::stored()
        .await?
        .ok_or("Fresh-process restart lost pair")?;
    if !receipt.checkpoint().is_replacement() {
        return Err("Restart receipt is not paired".into());
    }
    if !confirmed::requested() {
        reload(state).await;
    }
    locked(desktop).await?;
    checked(
        desktop,
        r#"({ok:document.querySelector('.canvas-dialog')?.textContent.includes('unconfirmed')})"#,
    )
    .await?;
    let source = snapshot(&state.peek());
    record
        .shot(
            state,
            desktop,
            "restored-lock",
            check::layout(desktop).await?,
            json!({"freshProcess":true}),
        )
        .await?;
    checked(desktop,r#"(()=>{for(const b of document.querySelectorAll('.canvas-paragraph-actions button'))b.click();return{ok:!document.getElementById('canvas-text-editor')}})()"#).await?;
    checked(desktop,r#"(()=>{const b=[...document.querySelectorAll('.canvas-toolbar button')].find(b=>b.textContent==='Review saved change');if(!b||b.disabled)return{ok:false};b.id='last-live-review-open';return{ok:true}})()"#).await?;
    let action = click(desktop, "#last-live-review-open").await?;
    wait(desktop,r#"(()=>{const p=document.querySelector('.canvas-local-review'),i=p?.querySelector('input');return{ok:!!i&&!i.disabled&&!i.checked&&p.textContent.includes('Delete a paragraph and create an empty replacement')&&p.textContent.includes('Either part, both parts, or neither part')}})()"#).await?;
    if confirmed::requested() {
        checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review');if(!e||e.querySelector('.canvas-review-preview'))return{ok:false};e.scrollIntoView({block:'center'});return{ok:true,emptySavedPreview:true}})()"#).await?;
    } else {
        checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review > .canvas-review-preview');if(!e||e.textContent!=='Another paragraph')return{ok:false};e.scrollIntoView({block:'center'});return{ok:true}})()"#).await?;
    }
    record
        .shot(
            state,
            desktop,
            "review-unchecked",
            capture::review_geometry(desktop, false).await?,
            action,
        )
        .await?;
    checked(desktop,r#"(()=>{const e=document.querySelector('.canvas-local-review .canvas-text-actions');if(!e)return{ok:false};e.scrollIntoView({block:'end'});const b=[...e.querySelectorAll('button')],c=b.find(b=>b.textContent==='Cancel'),r=b.find(b=>b.textContent==='Remove local edit lock');if(c)c.id='last-live-review-cancel';return{ok:!!c&&!c.disabled&&!!r&&r.disabled}})()"#).await?;
    record
        .shot(
            state,
            desktop,
            "review-controls",
            capture::review_geometry(desktop, true).await?,
            json!({"acknowledged":false,"removeDisabled":true}),
        )
        .await?;
    click(desktop, "#last-live-review-cancel").await?;
    wait(
        desktop,
        r#"({ok:!document.querySelector('.canvas-local-review')})"#,
    )
    .await?;
    locked(desktop).await?;
    let wrote = !fixture(&mut state.write()).writes.is_empty();
    if !same_source(&state.peek(), &source)
        || recovery::stored().await?.as_ref() != Some(&receipt)
        || wrote
        || recovery::observe().await?["archiveCount"] != 0
    {
        return Err("Inert fresh-process review changed source, journal or dispatched".into());
    }
    record
        .shot(
            state,
            desktop,
            "review-cancelled",
            check::layout(desktop).await?,
            json!({"noReplay":true}),
        )
        .await?;
    Ok(json!({"freshProcess":true,"noReplay":true,"exactReceiptRetained":true,"archiveCount":0}))
}
