//! Disconnected native routing and geometry evidence; input events are synthetic.
use super::{
    external_fixture as fixture,
    user_check::{checked, click, wait_dom},
};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::{Value, json};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() || super::multi_external_fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(error) => json!({"ok":false,"error":error}),
        };
        eprintln!("external-select-check: {result}");
    });
}
async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let modal = fixture::modal();
    let mut captures = serde_json::Map::new();
    wait_dom(d,if modal{"({ok:!!document.querySelector('[data-modal-external]')&&document.body.textContent.includes('Installed release')})"}else{"({ok:document.querySelector('.block-external-select')?.textContent.includes('Installed release')&&!document.querySelector('.block-external-select')?.disabled})"}).await?;
    if !modal {
        checked(d,r#"(()=>{const e=document.querySelector('.block-external-select'),r=e.getBoundingClientRect(),a=e.querySelector('span').getBoundingClientRect(),b=e.querySelector('svg').getBoundingClientRect();return {ok:Math.abs(r.height-32)<=1&&Math.abs((a.top+a.bottom)-(b.top+b.bottom))<=2&&a.right<=b.left+1};})()"#).await?;
    }
    captures.insert("initial".into(), capture(d, "initial").await?);
    open(d, modal).await?;
    query(d, "ab").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('at least 3')})").await?;
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    if queries(state) != 0 {
        return Err("Below-minimum query dispatched".into());
    }
    captures.insert("short-query".into(), capture(d, "short-query").await?);
    if modal {
        checked(d, "(()=>{const e=document.querySelector('[data-external-search] input');e.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));return {ok:true};})()").await?;
        modal_focus_returned(d).await?;
        open(d, true).await?;
    }
    query(d, "rel").await?;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    if queries(state) != 0 {
        return Err("Suggestion request dispatched before debounce".into());
    }
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('Loading options')})").await?;
    captures.insert("loading".into(), capture(d, "loading").await?);
    results(d).await?;
    let layout = wait_dom(d, LAYOUT).await?;
    captures.insert("groups".into(), capture(d, "groups").await?);
    attempts(state, 0)?;
    click(d, "[data-option-index='0']").await?;
    if !modal {
        wait_dom(
            d,
            "({ok:!!document.getElementById('button-confirmation-form')})",
        )
        .await?;
        captures.insert("confirmation".into(), capture(d, "confirmation").await?);
        attempts(state, 0)?;
        click(d, "#button-confirmation-cancel").await?;
        wait_dom(d,"({ok:document.querySelector('.block-external-select')?.textContent.includes('Installed release')})").await?;
        open(d, false).await?;
        query(d, "rel").await?;
        results(d).await?;
        click(d, "[data-option-index='0']").await?;
        wait_dom(
            d,
            "({ok:!!document.getElementById('button-confirmation-form')})",
        )
        .await?;
        checked(d,"(()=>{const b=document.querySelector('#button-confirmation-form button[type=submit]');b.click();b.click();return {ok:true};})()").await?;
        wait_attempts(state, 1).await?;
        wire(state, 0, Some("alpha,desktop"), false)?;
        fixture::complete(state, false);
    }
    wait_dom(d,if modal{"({ok:document.querySelector('[data-modal-external]')?.textContent.includes('Release Alpha')&&!document.querySelector('[data-external-search]')})"}else{"({ok:document.querySelector('.block-external-select')?.textContent.includes('Release Alpha')&&!document.querySelector('.block-external-select')?.disabled})"}).await?;
    if modal {
        wait_dom(
            d,
            "({ok:document.activeElement?.matches('[data-external-open]')})",
        )
        .await?;
    }
    captures.insert("selected".into(), capture(d, "selected").await?);
    open(d, modal).await?;
    query(d, "empty").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('No matching options')})").await?;
    close(d, modal).await?;
    wait_dom(d,"({ok:document.body.textContent.includes('Release Alpha')&&!document.querySelector('[data-external-search]')})").await?;
    if modal {
        attempts(state, 0)?;
        click(d, "[aria-label='Clear selected option']").await?;
        wait_dom(
            d,
            r#"({ok:!document.querySelector('[aria-label="Clear selected option"]')})"#,
        )
        .await?;
        captures.insert("cleared".into(), capture(d, "cleared").await?);
        submit(d).await?;
        wait_attempts(state, 1).await?;
        wire(state, 0, None, true)?;
        fixture::complete(state, false);
        wait_dom(
            d,
            "({ok:document.body.textContent.includes('Choose a different release')})",
        )
        .await?;
        captures.insert("validation".into(), capture(d, "validation").await?);
    }
    open(d, modal).await?;
    query(d, "error").await?;
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&document.querySelector('[data-external-search]')?.textContent.includes('not repeated')})").await?;
    let query_count = queries(state);
    query(d, "edited").await?;
    close(d, modal).await?;
    open(d, modal).await?;
    query(d, "edited").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('[data-external-retry]')})",
    )
    .await?;
    if queries(state) != query_count {
        return Err("Close/reopen bypassed Retry-After".into());
    }
    captures.insert("error".into(), capture(d, "error").await?);
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&!document.querySelector('[data-external-retry]').disabled})").await?;
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    if queries(state) != query_count {
        return Err("Rate-limited request replayed automatically".into());
    }
    click(d, "[data-external-retry]").await?;
    results(d).await?;
    if queries(state) != query_count + 1 {
        return Err("Explicit Retry did not issue exactly one request".into());
    }
    close(d, modal).await?;
    if modal {
        open(d, true).await?;
        query(d, "rel").await?;
        results(d).await?;
        click(d, "[data-option-index='1']").await?;
        wait_dom(d,"({ok:document.querySelector('[data-modal-external]')?.textContent.includes('Release Beta')&&!document.querySelector('[data-external-search]')})").await?;
        submit(d).await?;
        wait_attempts(state, 2).await?;
        wire(state, 1, Some("beta"), true)?;
    } else {
        click(d, ".block-button").await?;
        wait_attempts(state, 2).await?;
        wire(state, 1, Some("alpha,desktop"), false)?;
    }
    fixture::complete(state, true);
    wait_dom(d,if modal{"({ok:document.body.textContent.includes('will not be repeated')&&document.querySelector('#app-modal-form button[type=submit]')?.disabled})"}else{"({ok:[...document.querySelectorAll('.block-select,.block-button')].every(e=>e.disabled)})"}).await?;
    captures.insert("uncertain".into(), capture(d, "uncertain").await?);
    attempts(state, 2)?;
    let shell = state.read();
    let record = shell.message_buttons.external_fixture.as_ref().unwrap();
    if shell.core.transport.is_some() || !record.valid {
        return Err("Prepared request or isolation contract failed".into());
    }
    Ok(
        json!({"ok":true,"noTransport":true,"preparedRequests":record.attempts.len(),"queries":record.queries,
        "exactRawWire":true,"minimumAndDebounce":true,"groupOrder":true,"initialOutsideResults":true,"selectionSurvivedEmptyResults":true,"confirmationCancelNoCommit":!modal,"inlineNoDispatch":modal,"optionalClear":modal,"focusRestoredAfterDoneAndEscape":modal,"closeBesideSearch":layout["closeBesideSearch"],"uncertainNoReplay":true,"cooldownExpiryWithoutReplay":true,"cooldownAcrossVisits":true,"fullSiblingState":true,
        "compactSearch":layout["compactSearch"],"searchHeight":layout["searchHeight"],"eachVisible":layout["eachVisible"],"hitTestedControls":layout["hitTestedControls"],"noOverflow":layout["noOverflow"],"viewport":layout["viewport"],"captures":captures}),
    )
}
async fn open(d: &dioxus::desktop::DesktopContext, modal: bool) -> Result<(), String> {
    click(
        d,
        if modal {
            "[data-external-open]"
        } else {
            ".block-external-select"
        },
    )
    .await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('[data-external-search] input')})",
    )
    .await?;
    Ok(())
}
async fn close(d: &dioxus::desktop::DesktopContext, modal: bool) -> Result<(), String> {
    click(
        d,
        if modal {
            "[data-external-done]"
        } else {
            "#external-cancel"
        },
    )
    .await?;
    wait_dom(
        d,
        "({ok:!document.querySelector('[data-external-search]')})",
    )
    .await?;
    if modal {
        modal_focus_returned(d).await?;
    }
    Ok(())
}
async fn modal_focus_returned(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d, "({ok:!!document.getElementById('app-modal-form')&&!document.querySelector('[data-external-search]')&&document.activeElement?.matches('[data-external-open]')})").await?;
    Ok(())
}
async fn query(d: &dioxus::desktop::DesktopContext, value: &str) -> Result<(), String> {
    checked(d,&format!("(()=>{{const e=document.querySelector('[data-external-search] input');e.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true}};}})()",serde_json::to_string(value).unwrap())).await?;
    Ok(())
}
async fn results(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:document.querySelectorAll('[data-option-index]').length===2&&[...document.querySelectorAll('.message-multi-group')].map(e=>e.textContent).join('|')==='Stable|Preview'})").await?;
    Ok(())
}
async fn submit(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{const b=document.querySelector('#app-modal-form button[type=submit]');b.click();b.click();return {ok:true};})()").await?;
    Ok(())
}
fn queries(state: Signal<ShellState>) -> usize {
    state
        .read()
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .queries
        .len()
}
fn attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    let actual = state
        .read()
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .attempts
        .len();
    if actual == count {
        Ok(())
    } else {
        Err(format!("Expected {count} actions, found {actual}"))
    }
}
async fn wait_attempts(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    for _ in 0..100 {
        if attempts(state, count).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    attempts(state, count)
}
fn wire(
    state: Signal<ShellState>,
    index: usize,
    value: Option<&str>,
    modal: bool,
) -> Result<(), String> {
    let shell = state.read();
    let attempt = &shell
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .attempts[index];
    let values = if modal {
        &attempt["state"]["values"]
    } else {
        &attempt["state"]
    };
    let selected = &values["choice"]["pick"]["selected_option"];
    if selected.get("value").and_then(Value::as_str) != value
        || (value.is_none() && !selected.is_null())
    {
        return Err(format!("Wrong selected option: {selected}"));
    }
    if let Some(value) = value {
        let expected = fixture::option(
            if value == "beta" {
                "Release Beta"
            } else {
                "Release Alpha"
            },
            value,
        );
        if *selected != expected {
            return Err("Raw option was reconstructed".into());
        }
    }
    if modal {
        if values["note"]["value"]["value"] != "Keep this sibling" {
            return Err("Modal sibling lost".into());
        }
    } else if values["review"]["environment"]["selected_option"]["value"] != "prod" {
        return Err("Message sibling lost".into());
    }
    Ok(())
}
async fn capture(d: &dioxus::desktop::DesktopContext, stage: &str) -> Result<Value, String> {
    let Some(dir) = std::env::var_os("SUPER_PLATINUM_EXTERNAL_CAPTURE_DIR") else {
        return Ok(Value::Null);
    };
    checked(d,"(()=>{window.__externalPaint=false;requestAnimationFrame(()=>requestAnimationFrame(()=>window.__externalPaint=true));return {ok:true};})()").await?;
    wait_dom(d, "({ok:window.__externalPaint===true})").await?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    let path = std::path::PathBuf::from(dir).join(format!("{name}-{stage}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    Ok(json!(path.to_string_lossy()))
}
const LAYOUT: &str = r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.querySelector('[data-external-search] input'),c=document.querySelector('#external-cancel,[data-external-done]');const r=d.getBoundingClientRect();const sr=s.getBoundingClientRect(),cr=c.getBoundingClientRect();const closeBesideSearch=!c.matches('[data-external-done]')||(Math.abs((sr.top+sr.bottom)-(cr.top+cr.bottom))<=2&&sr.right<=cr.left);const searchHeight=sr.height;const compactSearch=Math.abs(searchHeight-32)<=1;const es=[s,c];const eachVisible=es.every(e=>{const x=e.getBoundingClientRect();return x.width>0&&x.height>0&&x.left>=0&&x.right<=innerWidth&&x.top>=0&&x.bottom<=innerHeight});const hitTestedControls=es.every(e=>{const x=e.getBoundingClientRect(),h=document.elementFromPoint(x.left+x.width/2,x.top+x.height/2);return h===e||e.contains(h)});const noOverflow=r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;return {ok:closeBesideSearch&&compactSearch&&eachVisible&&hitTestedControls&&noOverflow,closeBesideSearch,compactSearch,searchHeight,eachVisible,hitTestedControls,noOverflow,viewport:[innerWidth,innerHeight]};})()"#;
