//! Two closed custom-catalog scenarios, retaining original native and disk custody.
use super::*;
use fixture::completion::plain_custom_emoji as catalog;

async fn catalog_wait(
    state: Signal<ShellState>,
    expected: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..400 {
        let v = catalog::snapshot(&state.peek());
        if expected(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Original catalog did not settle: {}",
        catalog::snapshot(&state.peek())
    ))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let seed_source = data::seed(state)?;
    checked(d, include_str!("../../../comments.js")).await?;
    checked(d, include_str!("../../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A \u{4e2d}\u{1f642}").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let disk = wire::Disk::capture(&state.peek())?;
    let mut p = json!({"source":seed_source,"replySeed":seed,"replyBlocks":remembered["blocks"],"opens":[],"inputs":[],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?});
    let mut rows = Vec::new();
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, catalog::TEXT).await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    if !catalog::retired() {
        p["failedSaveBefore"] = snapshot(state, &m);
        p["firstSave"] = button(d, "slack-list-comment-edit-save").await?;
        wait(state,&m,|v|v["capture"]["canSave"]==true && v["edit"]["active"]["notice"]=="The edit was not sent. Permissions, required metadata, or the original comment changed. Both drafts are retained.").await?;
        catalog_wait(state, |v| v["finished"] == 1 && v["active"] == 0).await?;
        p["failedSaveAfter"] = snapshot(state, &m);
        take(state, &m, d, "catalog-failed", &p, &mut rows, false).await?;
    }
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    p["enter"] = probe(d, "key", json!({"key":"Enter"})).await?;
    let serial = if catalog::retired() { 1 } else { 2 };
    catalog_wait(state, |v| {
        v["pending"] == json!([serial]) && v["active"] == 1
    })
    .await?;
    wait(state, &m, |v| {
        v["edit"]["active"]["busy"] == true && v["capture"]["canCancel"] == false
    })
    .await?;
    p["busyControls"] = probe(d, "blocked-controls", Value::Null).await?;
    take(state, &m, d, "catalog-held", &p, &mut rows, false).await?;
    if catalog::retired() {
        p["beforeReplacementDisk"] = disk.snapshot().await?;
        p["originalFixtureBefore"] = data::retained_snapshot(&m);
        let (media, epoch, account) = {
            let s = state.peek();
            (
                s.media.clone(),
                s.core.account_epoch,
                s.core.active_account.clone(),
            )
        };
        let mut replacement = ShellState::fixture(media);
        replacement.slack_lists = Default::default();
        replacement.overlay = None;
        replacement.core.active_account = Some("list-custom-emoji-successor".into());
        replacement.core.account_epoch = epoch + 1;
        crate::bootstrap::install_loaded(state, replacement);
        let detached=wait_dom(d,"({ok:!document.querySelector('.slack-list-dialog')&&!document.getElementById('slack-list-comment-edit-draft')&&!document.getElementById('slack-list-comment-draft')})").await?;
        let successor_before = compose::snapshot(&state.peek(), &m);
        p["replacement"] = json!({"originalAccount":account,"originalEpoch":epoch,"detached":detached,"syntheticAccountReplacement":true,"catalogRelease":catalog::release(serial)?,"successorBefore":successor_before});
        catalog_wait(state, |v| {
            v["active"] == 0 && v["finished"] == 1 && v["contextsCurrent"] == json!([false])
        })
        .await?;
        p["afterReplacementDisk"] = disk.snapshot().await?;
        p["originalFixtureAfter"] = data::retained_snapshot(&m);
        if p["afterReplacementDisk"] != p["beforeReplacementDisk"]
            || p["originalFixtureAfter"] != p["originalFixtureBefore"]
        {
            return Err("Retired catalog changed original edit/reply or update ledger".into());
        }
        let successor = compose::snapshot(&state.peek(), &m);
        if successor != p["replacement"]["successorBefore"] {
            return Err("Late catalog changed successor".into());
        }
        let original_catalog = catalog::snapshot(&state.peek());
        let ordered = wire::snapshot(&state.peek(), &m);
        let capture = harness::capture(d, "replaced").await?;
        if successor != compose::snapshot(&state.peek(), &m)
            || original_catalog != catalog::snapshot(&state.peek())
            || ordered != wire::snapshot(&state.peek(), &m)
        {
            return Err("Retired catalog capture changed".into());
        }
        rows.push(json!({"stage":"replaced","geometry":detached,"model":successor,"catalog":original_catalog,"ordered":ordered,"journal":p["afterReplacementDisk"]["reply"],"editInventory":p["afterReplacementDisk"]["edit"],"proof":p,"capture":capture}));
    } else {
        p["catalogRelease"] = catalog::release(serial)?;
        wait_fixture(state, |v| v["held"] == true).await?;
        p["doubleSave"] = command(d, "double-save", Value::Null).await?;
        take(state, &m, d, "saving", &p, &mut rows, false).await?;
        p["release"] = data::release(&state.peek())?;
        closed(state, &m, d).await?;
        take(state, &m, d, "saved", &p, &mut rows, false).await?;
        wire::publish(&state.peek())?;
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comments-refresh").await?);
        for _ in 0..400 {
            if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
                == wire::future_message()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
            != wire::future_message()
        {
            return Err("Original custom readback missing".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &m, d, "readback", &p, &mut rows, false).await?;
    }
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if catalog::retired(){"own-edit-plain-custom-retired"}else{"own-edit-plain-custom"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":if catalog::retired(){compose::snapshot(&state.peek(),&m)}else{snapshot(state,&m)},"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Actual full emoji.list raw response through the production parser; no picker/workspace publication.","Original synthetic DOM input and Save; custom image download/rendering parity is not claimed.","Retirement uses explicit synthetic offline account replacement; busy Cancel remains disabled."]}),
    )
}
