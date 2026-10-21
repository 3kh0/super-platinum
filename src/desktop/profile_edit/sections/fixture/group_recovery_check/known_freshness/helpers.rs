use super::*;

pub(super) async fn editor(
    desktop: &DesktopContext,
    text: &str,
    renamed: bool,
) -> Result<(), String> {
    let labels = json!([
        if renamed {
            known::RENAMED
        } else {
            recovery::LABEL
        },
        if renamed {
            known::RENAMED
        } else {
            recovery::LABEL
        },
        recovery::OTHER_LABEL
    ]);
    let blocks = json!(recovery::blocks(text));
    wait_dom(desktop,&format!(r#"(()=>{{
        const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e);
        const stable=x=>JSON.stringify(x,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);
        return{{ok:!!v&&e.isContentEditable&&stable(v.snapshot.blocks)===stable({blocks})
            &&JSON.stringify(v.tokens.map(t=>t.label))===JSON.stringify({labels})
            &&!document.getElementById('profile-section-rich-status')?.textContent&&!document.getElementById('status-toast')}};
    }})()"#)).await.map(|_|())
}
pub(super) async fn remember(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop, include_str!("remember.js")).await
}
pub(super) async fn custody(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop, include_str!("custody.js")).await
}
pub(super) async fn edit(desktop: &DesktopContext) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{
        const e=document.getElementById('profile-section-rich'),leaf=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)');
        if(!e?.isContentEditable||!leaf)return{{ok:false}};
        const before=window.superPlatinumRichEditor.resolved(e);
        leaf.textContent={};e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));
        const after=window.superPlatinumRichEditor.resolved(e);return{{ok:true,before,after}};
    }})()"#,json!(recovery::EDITED))).await
}
pub(super) fn source(state: Signal<ShellState>, renamed: bool) -> Result<Value, String> {
    let model = recovery::snapshot(&state.peek());
    let snapshot = &model["knownFreshness"]["source"];
    let rows = snapshot["rows"]
        .as_array()
        .ok_or("Missing actual canonical group snapshot")?;
    if snapshot["current"] != true
        || rows.len() != 2
        || recovery::ids().iter().any(|id| {
            rows.iter()
                .find(|r| r["source"]["id"] == *id)
                .is_none_or(|r| r["source"] != known::raw(id, renamed, false))
                || model["groups"][id]["source"] != known::raw(id, renamed, false)
        })
    {
        return Err(format!(
            "Known group original canonical source differs: {model}"
        ));
    }
    Ok(snapshot.clone())
}
pub(super) fn scheduler(state: Signal<ShellState>) -> Value {
    recovery::snapshot(&state.peek())["knownFreshness"]["scheduler"].clone()
}
pub(super) async fn settled(state: Signal<ShellState>, finished: u64) -> Result<Value, String> {
    for _ in 0..100 {
        let current = scheduler(state);
        if current["finished"] == finished
            && current["worker"] == false
            && current["inFlight"].is_null()
            && current["pending"] == json!([])
            && current["freshIds"] == json!(recovery::ids())
            && recovery::pending().is_empty()
        {
            return Ok(current);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Known-group scheduler settlement timed out: {}",
        recovery::snapshot(&state.peek())
    ))
}
pub(super) async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
    renamed: bool,
) -> Result<Value, String> {
    let requests = recovery::requests();
    let groups = recovery::snapshot(&state.peek())["groups"].clone();
    let source_before = source(state, renamed)?;
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        editor(desktop, text, renamed).await?;
        if recovery::requests() != requests
            || recovery::snapshot(&state.peek())["groups"] != groups
            || source(state, renamed)? != source_before
        {
            return Err("Known selector looped or rewrote its settled source".into());
        }
    }
    Ok(json!({"observedMs":500,"noAdditionalRead":true,"sourceRetained":true}))
}
pub(super) async fn reset_open(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
    renamed: bool,
) -> Result<(u64, Value), String> {
    click(desktop, "Cancel description edit", false).await?;
    form(desktop).await?;
    let reset = known::reconnect(&mut state.write())?;
    click(desktop, "Edit description", true).await?;
    editor(desktop, text, renamed).await?;
    let request = super::super::scenario::request(state, "directory", &recovery::ids()).await?;
    let opening = recovery::snapshot(&state.peek())["knownFreshness"]["opening"].clone();
    if opening["mounted"] != true || opening["reading"] != false || opening["lease"] != false {
        return Err("Known read withheld editor admission or created MissingOpening".into());
    }
    let quiet = quiet(state, desktop, text, renamed).await?;
    Ok((
        request,
        json!({"reset":reset,"request":request,"opening":opening,
        "immediateAdmission":true,"duplicateIdsCoalesced":true,"duplicateOpen":true,"quiet":quiet}),
    ))
}
pub(super) async fn reopen(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
    renamed: bool,
) -> Result<Value, String> {
    click(desktop, "Cancel description edit", false).await?;
    form(desktop).await?;
    let before = recovery::requests();
    click(desktop, "Edit description", false).await?;
    editor(desktop, text, renamed).await?;
    let quiet = quiet(state, desktop, text, renamed).await?;
    if recovery::requests() != before {
        return Err("Infinity-fresh known group reopening issued another read".into());
    }
    Ok(json!({"noRedundantRead":true,"quiet":quiet}))
}
