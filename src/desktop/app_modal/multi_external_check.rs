//! Native synthetic-input evidence for normal modal multi-external controls.
//! Credential replacement and malformed protocol cases remain focused unit tests.
use super::{multi_external_fixture as fixture, *};
use crate::message_buttons::{
    external_fixture as responses,
    multi_external_fixture::{self as harness, checked, click, wait_dom},
};
use serde_json::json;
use std::time::Duration;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Modal multi-external check timed out"}),
        };
        eprintln!("multi-external-select-check: {receipt}");
    });
}

async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let ready = harness::wait_runner(state).await?;
    let mut captures = serde_json::Map::new();
    mounted(d).await?;
    expect(state, "choice", &["installed"])?;
    expect(state, "optional", &[])?;
    wait_dom(d,"({ok:document.querySelector('[data-modal-external=choice]')?.textContent.includes('Installed release')&&!document.querySelector('[data-modal-external=optional]')?.textContent.includes('Ignored initial')})").await?;
    captures.insert("initial".into(), harness::capture(d, "initial").await?);

    // Required empty stays editable and never reaches the synthetic transport.
    remove(d, "Installed release").await?;
    expect(state, "choice", &[])?;
    submit(d).await?;
    wait_dom(d,"({ok:document.querySelector('[data-block-id=choice] .app-modal-error')?.textContent.trim().length>0})").await?;
    attempts(state, 0)?;
    captures.insert(
        "required-empty".into(),
        harness::capture(d, "required-empty").await?,
    );
    click(d, "#app-modal-form footer button[type=button]").await?;
    wait_dom(d, "({ok:!document.getElementById('app-modal-form')})").await?;
    attempts(state, 0)?;
    fixture::remount(&mut state.write());
    mounted(d).await?;
    expect(state, "choice", &["installed"])?;
    expect(state, "optional", &[])?;
    captures.insert("remounted".into(), harness::capture(d, "remounted").await?);

    open(d).await?;
    query(d, "rel").await?;
    results(d).await?;
    let geometry = wait_dom(d, LAYOUT).await?;
    click(d, "[data-option-index='1']").await?;
    wait_selected(d, 2, true).await?;
    expect(state, "choice", &["installed", "beta"])?;
    click(d, "[data-option-index='0']").await?;
    wait_selected(d, 3, true).await?;
    expect(state, "choice", &["installed", "beta", "alpha,desktop"])?;
    submit(d).await?;
    wait_dom(d,"({ok:document.querySelector('[data-block-id=choice] .app-modal-error')?.textContent.trim().length>0})").await?;
    attempts(state, 0)?;
    captures.insert("maximum".into(), harness::capture(d, "maximum").await?);
    remove(d, "Installed release").await?;
    expect(state, "choice", &["beta", "alpha,desktop"])?;
    captures.insert("ordered".into(), harness::capture(d, "ordered").await?);
    query(d, "empty").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('No matching options')})").await?;
    expect(state, "choice", &["beta", "alpha,desktop"])?;
    captures.insert(
        "empty-query".into(),
        harness::capture(d, "empty-query").await?,
    );
    close(d).await?;
    checked(d,r#"(()=>{const e=document.querySelector('[data-block-id=note] input');e.value='Local sibling draft';e.dispatchEvent(new Event('input',{bubbles:true}));return {ok:true};})()"#).await?;
    submit(d).await?;
    wait_attempts(state, 1).await?;
    wire(state, 0)?;
    responses::complete(state, false);
    wait_dom(
        d,
        "({ok:document.body.textContent.includes('Choose a different release')})",
    )
    .await?;
    captures.insert(
        "validation".into(),
        harness::capture(d, "validation").await?,
    );

    // A changed pristine array resets inline state even when saved [] exists.
    // A dirty primary array survives the same replacement, with old proofs stale.
    open(d).await?;
    let queries_before_stale = queries(state);
    query(d, "stale").await?;
    wait_dom(d,"({ok:document.querySelector('[data-external-search]')?.textContent.includes('Loading options')})").await?;
    wait_queries(state, queries_before_stale + 1).await?;
    let old = external::owner(&state.peek(), &fixture::key("choice")).ok_or("Missing old owner")?;
    let old_revision = old
        .query(&state.peek())
        .ok_or("Missing old query")?
        .revision;
    let mut update = fixture::view();
    update["hash"] = json!("modal-multi-external-2");
    update["blocks"][0]["element"]["initial_options"] =
        json!([responses::option("Remote initial", "remote")]);
    update["blocks"][1]["element"]["initial_options"] =
        json!([responses::option("Pristine replacement", "replacement")]);
    super::apply_fixture_update(state, update.clone());
    external::choose(state, old.clone(), old_revision, 0);
    wait_dom(d,"({ok:!document.querySelector('[data-external-search]')&&document.querySelector('[data-modal-external=optional]')?.textContent.includes('Pristine replacement')})").await?;
    expect(state, "choice", &["beta", "alpha,desktop"])?;
    expect(state, "optional", &["replacement"])?;
    if old.query(&state.peek()).is_some() {
        return Err("Old view query retained authority".into());
    }
    captures.insert(
        "pristine-update".into(),
        harness::capture(d, "pristine-update").await?,
    );
    update["hash"] = json!("modal-multi-external-3");
    update["blocks"][1]["element"]["initial_options"] = Value::Null;
    super::apply_fixture_update(state, update.clone());
    expect(state, "optional", &["replacement"])?;
    update["hash"] = json!("modal-multi-external-4");
    update["blocks"][1]["element"]["initial_options"] = json!([]);
    super::apply_fixture_update(state, update);
    expect(state, "optional", &[])?;
    wait_dom(d,"({ok:!document.querySelector('[data-modal-external=optional]')?.textContent.includes('Pristine replacement')&&document.querySelector('[data-modal-external=choice]')?.textContent.includes('Release Beta')})").await?;
    if requests::prepare_submission(&state.peek()).is_ok() {
        return Err("Dirty replacement silently authorized submission".into());
    }
    attempts(state, 1)?;
    captures.insert(
        "dirty-update".into(),
        harness::capture(d, "dirty-update").await?,
    );

    open(d).await?;
    query(d, "error").await?;
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&document.querySelector('[data-external-search]')?.textContent.includes('not repeated')})").await?;
    let query_count = queries(state);
    close(d).await?;
    open(d).await?;
    query(d, "edited").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('[data-external-retry]')})",
    )
    .await?;
    if queries(state) != query_count {
        return Err("Reopen bypassed route cooldown".into());
    }
    captures.insert("cooldown".into(), harness::capture(d, "cooldown").await?);
    wait_dom(d,"({ok:!!document.querySelector('[data-external-retry]')&&!document.querySelector('[data-external-retry]').disabled})").await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    if queries(state) != query_count {
        return Err("Cooldown expiry replayed request".into());
    }
    click(d, "[data-external-retry]").await?;
    results(d).await?;
    if queries(state) != query_count + 1 {
        return Err("Explicit retry did not dispatch once".into());
    }
    // Reaccepted exact rows restore provenance; explicit edits clear server errors.
    click(d, "[data-option-index='0']").await?;
    wait_selected(d, 1, true).await?;
    expect(state, "choice", &["beta"])?;
    click(d, "[data-option-index='0']").await?;
    wait_selected(d, 2, true).await?;
    expect(state, "choice", &["beta", "alpha,desktop"])?;
    close(d).await?;
    submit(d).await?;
    wait_attempts(state, 2).await?;
    wire(state, 1)?;
    responses::complete(state, true);
    wait_dom(d,"({ok:document.body.textContent.includes('will not be repeated')&&document.querySelector('#app-modal-form button[type=submit]')?.disabled})").await?;
    captures.insert("uncertain".into(), harness::capture(d, "uncertain").await?);
    checked(d,"(()=>{document.querySelector('#app-modal-form button[type=submit]').click();return {ok:true};})()").await?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    attempts(state, 2)?;
    let shell = state.peek();
    let record = shell
        .message_buttons
        .external_fixture
        .as_ref()
        .ok_or("Missing recorder")?;
    if shell.core.transport.is_some() || shell.core.session.is_some() || !record.valid {
        return Err("Isolation/prepared request contract failed".into());
    }
    Ok(
        json!({"ok":true,"fixture":ready["fixture"],"nonce":ready["nonce"],"pid":ready["pid"],
        "noTransport":true,"noSession":true,"preparedRequests":2,"queries":record.queries,"attempts":record.attempts,
        "inlineNoDispatch":true,"requiredAndMaximum":true,"savedEmpty":true,"orderedRawWire":true,
        "retainedAcrossQueries":true,"cancelRemount":true,"pristineArrayReset":true,"dirtyRetained":true,
        "staleViewRefused":true,"uncertainNoReplay":true,"cooldownAcrossVisits":true,
        "cooldownExpiryWithoutReplay":true,"fullSiblingState":true,"syntheticInput":true,
        "noOverflow":geometry["noOverflow"],"eachVisible":geometry["eachVisible"],
        "hitTestedControls":geometry["hitTestedControls"],"compactSearch":geometry["compactSearch"],
        "closeBesideSearch":geometry["closeBesideSearch"],"viewport":geometry["viewport"],"captures":captures}),
    )
}

fn expect(state: Signal<ShellState>, block: &str, expected: &[&str]) -> Result<(), String> {
    let shell = state.peek();
    let field = &shell
        .app_modal
        .as_ref()
        .ok_or("Missing modal")?
        .current_form()
        .ok_or("Missing form")?
        .fields[&fixture::key(block)];
    let InputValue::ExternalOptions(options) = &field.value else {
        return Err("Wrong multi value kind".into());
    };
    let actual: Vec<_> = options.iter().map(|o| o.value()).collect();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{block} selected values mismatch: {actual:?}"))
    }
}
fn queries(state: Signal<ShellState>) -> usize {
    state
        .peek()
        .message_buttons
        .external_fixture
        .as_ref()
        .unwrap()
        .queries
        .len()
}
fn attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    let actual = state
        .peek()
        .message_buttons
        .external_fixture
        .as_ref()
        .ok_or("Missing recorder")?
        .attempts
        .len();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("Expected {expected} attempts, found {actual}"))
    }
}
async fn wait_attempts(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..100 {
        if attempts(state, expected).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    attempts(state, expected)
}
async fn wait_queries(state: Signal<ShellState>, expected: usize) -> Result<(), String> {
    for _ in 0..100 {
        if queries(state) == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Expected suggestion request was not issued".into())
}
fn wire(state: Signal<ShellState>, index: usize) -> Result<(), String> {
    let shell = state.peek();
    let values = &shell
        .message_buttons
        .external_fixture
        .as_ref()
        .ok_or("Missing recorder")?
        .attempts[index]["state"]["values"];
    if values["choice"]["pick"]["selected_options"]
        != json!([
            responses::option("Release Beta", "beta"),
            responses::option("Release Alpha", "alpha,desktop")
        ])
        || values["optional"]["pick"]["selected_options"] != json!([])
        || values["note"]["value"]["value"] != "Local sibling draft"
    {
        return Err("Exact raw ordered/full sibling submission mismatch".into());
    }
    Ok(())
}
async fn mounted(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:!!document.querySelector('[data-modal-external=choice] [data-external-open]')&&!!document.querySelector('[data-modal-external=optional]')})").await?;
    Ok(())
}
async fn open(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(d, "[data-modal-external=choice] [data-external-open]").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('[data-external-search] input')})",
    )
    .await?;
    Ok(())
}
async fn close(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    click(d, "[data-external-done]").await?;
    wait_dom(d,"({ok:!document.querySelector('[data-external-search]')&&document.activeElement?.matches('[data-external-open]')})").await?;
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
async fn remove(d: &dioxus::desktop::DesktopContext, label: &str) -> Result<(), String> {
    let before = checked(d,"(()=>{const count=document.querySelectorAll('[data-modal-external=choice] .message-multi-selected-option').length;return {ok:count>0,count};})()").await?;
    click(
        d,
        &format!(
            "[data-modal-external=choice] button[aria-label={}]",
            serde_json::to_string(&format!("Remove {label}")).unwrap()
        ),
    )
    .await?;
    wait_selected(
        d,
        before["count"].as_u64().ok_or("Missing selected count")? as usize - 1,
        true,
    )
    .await
}
async fn submit(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{const b=document.querySelector('#app-modal-form button[type=submit]');b.click();b.click();return {ok:true};})()").await?;
    Ok(())
}
async fn wait_selected(
    d: &dioxus::desktop::DesktopContext,
    count: usize,
    equal: bool,
) -> Result<(), String> {
    wait_dom(d,&format!("(()=>{{const n=document.querySelectorAll('[data-modal-external=choice] .message-multi-selected-option').length;return {{ok:n{}{},n}};}})()",if equal{"==="}else{"<"},count)).await?;
    Ok(())
}
const LAYOUT: &str = r#"(()=>{const d=document.getElementById('shell-overlay-dialog'),s=document.querySelector('[data-external-search] input'),c=document.querySelector('[data-external-done]');s.scrollIntoView({block:'nearest'});const r=d.getBoundingClientRect(),sr=s.getBoundingClientRect(),cr=c.getBoundingClientRect();const closeBesideSearch=Math.abs((sr.top+sr.bottom)-(cr.top+cr.bottom))<=2&&sr.right<=cr.left;const compactSearch=Math.abs(sr.height-32)<=1;const es=[s,c];const eachVisible=es.every(e=>{const x=e.getBoundingClientRect();return x.width>0&&x.height>0&&x.left>=0&&x.right<=innerWidth&&x.top>=0&&x.bottom<=innerHeight});const hitTestedControls=es.every(e=>{const x=e.getBoundingClientRect(),h=document.elementFromPoint(x.left+x.width/2,x.top+x.height/2);return h===e||e.contains(h)});const noOverflow=r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&d.scrollWidth<=d.clientWidth+1;return {ok:closeBesideSearch&&compactSearch&&eachVisible&&hitTestedControls&&noOverflow,closeBesideSearch,compactSearch,eachVisible,hitTestedControls,noOverflow,viewport:[innerWidth,innerHeight]};})()"#;
