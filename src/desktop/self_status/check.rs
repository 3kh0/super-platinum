//! Exact-gated signed native status workflow proof; all requests are prepared offline.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !super::fixture::enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let receipt = match run(state, &desktop).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("self-status-check: {receipt}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let mut stage = "initial status";
    let result:Result<Value,String>=async {
        wait_text(desktop,"Original status").await?;
        assert_snapshot(state,0,"Original status",":eyes:")?;
        stage="cancel discards unsaved draft";
        input(desktop,"self-status-text","Cancelled status","input").await?;
        click(desktop,"self-status-cancel").await?;
        closed(desktop).await?;
        assert_snapshot(state,0,"Original status",":eyes:")?;
        super::open(state);
        wait_text(desktop,"Original status").await?;

        stage="typing retained across expiry render";
        input(desktop,"self-status-text","Reviewing the release","input").await?;
        checked(desktop,r#"(()=>{const input=document.getElementById('self-status-text');input.focus();input.setSelectionRange(7,7);window.__statusInput=input;return {ok:true};})()"#).await?;
        input(desktop,"self-status-expiry","1_hour","change").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('self-status-text')===window.__statusInput&&window.__statusInput.value==='Reviewing the release'&&window.__statusInput.selectionStart===7&&document.getElementById('self-status-expiry').value==='1_hour'})"#).await?;

        stage="shared emoji picker retains draft";
        click(desktop,"self-status-emoji").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('emoji-picker-title')?.textContent==='Choose status emoji'&&document.getElementById('emoji-query')===document.activeElement})"#).await?;
        checked(desktop,r#"(()=>{const r=document.querySelector('.emoji-picker').getBoundingClientRect();return {ok:r.top>=0&&r.bottom<=innerHeight+1&&r.left>=0&&r.right<=innerWidth+1,pickerBounds:{top:r.top,bottom:r.bottom,height:innerHeight}};})()"#).await?;
        input(desktop,"emoji-query","rocket","input").await?;
        wait_dom(desktop,r#"({ok:!!document.querySelector('.emoji-options [role="option"][aria-label="rocket"]')})"#).await?;
        checked(desktop,r#"(()=>{document.querySelector('.emoji-options [role="option"][aria-label="rocket"]').click();return {ok:true};})()"#).await?;
        wait_text(desktop,"Reviewing the release").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('self-status-emoji')?.title.includes(':rocket:')})"#).await?;
        assert_snapshot(state,0,"Original status",":eyes:")?;
        checked(desktop,r#"(()=>{window.__statusInput=document.getElementById('self-status-text');return {ok:true};})()"#).await?;

        stage="custom date rejects past value";
        input(desktop,"self-status-expiry","custom","change").await?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('self-status-custom-date')})"#).await?;
        input(desktop,"self-status-custom-date","2000-01-01T00:00","input").await?;
        wait_dom(desktop,r#"({ok:document.getElementById('self-status-save').disabled&&document.getElementById('self-status-error')?.textContent.includes('future')})"#).await?;
        stage="valid custom date";
        let future=checked(desktop,r#"(()=>{const d=new Date();d.setDate(d.getDate()+2);d.setHours(12,0,0,0);const pad=v=>String(v).padStart(2,'0');const value=d.getFullYear()+'-'+pad(d.getMonth()+1)+'-'+pad(d.getDate())+'T12:00';const input=document.getElementById('self-status-custom-date');input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true,timestamp:Math.floor(d.getTime()/1000),value};})()"#).await?;
        let expiration=future["timestamp"].as_i64().ok_or("Missing custom timestamp")?;
        wait_dom(desktop,r#"({ok:!document.getElementById('self-status-save').disabled&&!document.getElementById('self-status-error')})"#).await?;

        stage="pending locks editing and duplicate saves";
        checked(desktop,r#"(()=>{const button=document.getElementById('self-status-save');button.click();button.click();return {ok:true};})()"#).await?;
        wait_dom(desktop,LOCKED).await?;
        let pending=assert_snapshot(state,1,"Original status",":eyes:")?;
        if pending["pending"]!=true || pending["attempts"][0]!=json!({"status_text":"Reviewing the release","status_emoji":":rocket:","status_expiration":expiration,"status_text_canonical":"","status_duration":"custom"}) {return Err("Unexpected prepared custom status payload or missing pending request".into());}
        stage="server rejection retains editable draft";
        super::fixture::complete_pending(&mut state.write());
        wait_dom(desktop,r#"({ok:!document.getElementById('self-status-text').disabled&&!!document.getElementById('self-status-error')&&document.getElementById('self-status-text')===window.__statusInput&&window.__statusInput.value==='Reviewing the release'&&document.getElementById('self-status-emoji').title.includes(':rocket:')})"#).await?;
        assert_snapshot(state,1,"Original status",":eyes:")?;
        wait_dom(desktop,r#"({ok:!document.getElementById('self-status-save').disabled})"#).await?;
        stage="retry success merges confirmed profile";
        click(desktop,"self-status-save").await?;
        wait_dom(desktop,LOCKED).await?;
        assert_snapshot(state,2,"Original status",":eyes:")?;
        super::fixture::complete_pending(&mut state.write());
        closed(desktop).await?;
        let saved=assert_snapshot(state,2,"Reviewing the release",":rocket:")?;
        if saved["status"]["expiration"]!=expiration {return Err("Confirmed expiration differs from custom local date".into());}

        stage="uncertain outcome locks retry";
        super::open(state);
        wait_text(desktop,"Reviewing the release").await?;
        input(desktop,"self-status-text","Waiting for confirmation","input").await?;
        input(desktop,"self-status-expiry","1_hour","change").await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('self-status-save').disabled&&!document.getElementById('self-status-custom-date')})"#).await?;
        state.write().self_status.fixture.as_mut().ok_or("Missing fixture")?.next_uncertain=true;
        click(desktop,"self-status-save").await?;
        wait_dom(desktop,LOCKED).await?;
        assert_snapshot(state,3,"Reviewing the release",":rocket:")?;
        super::fixture::complete_pending(&mut state.write());
        wait_dom(desktop,r#"({ok:!!document.getElementById('self-status-uncertain')&&document.getElementById('self-status-text').disabled&&document.getElementById('self-status-expiry').disabled&&document.getElementById('self-status-emoji').disabled&&!document.getElementById('self-status-save')&&!document.getElementById('self-status-check').disabled})"#).await?;
        assert_snapshot(state,3,"Reviewing the release",":rocket:")?;
        stage="explicit readback resolves without resend";
        click(desktop,"self-status-check").await?;
        closed(desktop).await?;
        let readback=assert_snapshot(state,3,"Waiting for confirmation",":rocket:")?;
        if readback["checks"]!=1 {return Err("Expected exactly one readback".into());}
        stage="Escape discards another edit";
        super::open(state);
        wait_text(desktop,"Waiting for confirmation").await?;
        input(desktop,"self-status-text","Discard with Escape","input").await?;
        checked(desktop,r#"(()=>{document.getElementById('self-status-text').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()"#).await?;
        closed(desktop).await?;
        assert_snapshot(state,3,"Waiting for confirmation",":rocket:")?;
        super::open(state);
        wait_text(desktop,"Waiting for confirmation").await?;
        stage="final layout";
        let mut receipt=checked(desktop,LAYOUT).await?;
        receipt["preparedRequests"]=json!(3);
        receipt["readbacks"]=json!(1);
        receipt["cancelAndEscapeDiscardedDraft"]=json!(true);
        receipt["typingAndEmojiRetained"]=json!(true);
        receipt["pastDateRejected"]=json!(true);
        receipt["duplicatePendingSaveRejected"]=json!(true);
        receipt["serverErrorRetainedDraft"]=json!(true);
        receipt["uncertainCheckedWithoutResend"]=json!(true);
        receipt["noTransport"]=json!(true);
        Ok(receipt)
    }.await;
    result.map_err(|error| format!("{stage}: {error}"))
}
fn assert_snapshot(
    state: Signal<ShellState>,
    attempts: usize,
    text: &str,
    emoji: &str,
) -> Result<Value, String> {
    let value = super::fixture::snapshot(&state.peek());
    if value["attempts"].as_array().map(Vec::len) != Some(attempts)
        || value["prepared_valid"] != true
        || value["noTransport"] != true
        || value["status"]["text"] != text
        || value["status"]["emoji"] != emoji
    {
        return Err(format!("Unexpected disconnected fixture state: {value}"));
    }
    Ok(value)
}
async fn input(
    desktop: &dioxus::desktop::DesktopContext,
    id: &str,
    value: &str,
    event: &str,
) -> Result<(), String> {
    let id = serde_json::to_string(id).unwrap();
    let value = serde_json::to_string(value).unwrap();
    let event = serde_json::to_string(event).unwrap();
    checked(desktop,&format!(r#"(()=>{{const input=document.getElementById({id});if(!input||input.disabled)return {{ok:false}};input.value={value};input.dispatchEvent(new Event({event},{{bubbles:true}}));return {{ok:true}};}})()"#)).await?;
    Ok(())
}
async fn click(desktop: &dioxus::desktop::DesktopContext, id: &str) -> Result<(), String> {
    let id = serde_json::to_string(id).unwrap();
    checked(desktop,&format!(r#"(()=>{{const button=document.getElementById({id});if(!button||button.disabled)return {{ok:false}};button.click();return {{ok:true}};}})()"#)).await?;
    Ok(())
}
async fn wait_text(desktop: &dioxus::desktop::DesktopContext, text: &str) -> Result<(), String> {
    let text = serde_json::to_string(text).unwrap();
    wait_dom(desktop,&format!(r#"({{ok:document.getElementById('self-status-text')?.value==={text}&&!document.getElementById('self-status-text').disabled}})"#)).await?;
    Ok(())
}
async fn closed(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('self-status-form')})"#,
    )
    .await?;
    Ok(())
}
const LOCKED: &str = r#"({ok:document.getElementById('self-status-save')?.disabled&&document.getElementById('self-status-save').getAttribute('aria-busy')==='true'&&['self-status-text','self-status-emoji','self-status-expiry','self-status-cancel'].every(id=>document.getElementById(id)?.disabled)&&(!document.getElementById('self-status-custom-date')||document.getElementById('self-status-custom-date').disabled)})"#;
const LAYOUT: &str = r#"(()=>{const dialog=document.querySelector('.self-status-editor'),body=dialog.querySelector('.self-status-body');const r=dialog.getBoundingClientRect();const footer=dialog.querySelector('footer').getBoundingClientRect();const date=document.getElementById('self-status-custom-date');if(date)date.scrollIntoView({block:'nearest'});const controls=[document.getElementById('self-status-save'),document.getElementById('self-status-cancel'),date].filter(Boolean);const fits=controls.every(e=>{const b=e.getBoundingClientRect();return b.left>=r.left&&b.right<=r.right+1&&b.top>=r.top&&b.bottom<=r.bottom+1;});return {ok:r.top>=0&&r.bottom<=innerHeight+1&&r.left>=0&&r.right<=innerWidth+1&&footer.bottom<=innerHeight+1&&body.scrollWidth<=body.clientWidth+1&&fits,width:innerWidth,height:innerHeight,dialogHeight:r.height,bodyOverflow:body.scrollWidth-body.clientWidth,footerBottom:footer.bottom};})()"#;
async fn wait_dom(
    desktop: &dioxus::desktop::DesktopContext,
    script: &str,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..60 {
        last = eval(desktop, script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Err(format!(
        "Native status did not reach expected state: {last}; expression: {script}"
    ))
}
async fn checked(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let receipt = eval(desktop, script).await?;
    if receipt["ok"] == true {
        Ok(receipt)
    } else {
        Err(format!(
            "Native status assertion failed: {receipt}; expression: {script}"
        ))
    }
}
async fn eval(desktop: &dioxus::desktop::DesktopContext, script: &str) -> Result<Value, String> {
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |value| {
            let _ = send.send(value);
        })
        .map_err(|_| "Native evaluation request failed")?;
    let raw = tokio::time::timeout(std::time::Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Native evaluation timed out")?
        .ok_or("Native callback closed")?;
    let value: Value = serde_json::from_str(&raw).map_err(|_| "Invalid native callback")?;
    match value {
        Value::String(value) => {
            serde_json::from_str(&value).map_err(|_| "Invalid encoded native callback".into())
        }
        value => Ok(value),
    }
}
