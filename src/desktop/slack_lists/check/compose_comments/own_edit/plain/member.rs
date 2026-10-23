//! Original canonical member source refusal and same-owner Save retry.
use super::*;
use fixture::plain_members as members;
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
    replace(state, &m, d, &mut p, members::TEXT).await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    let (repair, event) = members::hold(state)?;
    p["memberEvent"] = event;
    for _ in 0..500 {
        if members::snapshot()["held"] == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if members::snapshot()["held"] != 2 {
        return Err("Original known member callback did not become held".into());
    }
    take(state, &m, d, "source-stale", &p, &mut rows, false).await?;
    p["failedSaveBefore"] = snapshot(state, &m);
    p["failedSave"] = probe(d, "key", json!({"key":"Enter"})).await?;
    wait(state, &m, |v| v["edit"]["active"]["notice"] == "The edit was not sent. Permissions, required metadata, or the original comment changed. Both drafts are retained."
        && v["capture"]["canSave"] == true && v["edit"]["attempts"] == json!([])).await?;
    p["failedSaveAfter"] = snapshot(state, &m);
    take(state, &m, d, "save-refused", &p, &mut rows, false).await?;
    p["memberCallback"] = members::release()?;
    for _ in 0..500 {
        let v = members::source(&state.peek());
        if v["current"] == true && members::snapshot()["repair"]["ok"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if members::source(&state.peek())["current"] != true {
        return Err("Original known response did not repair full member source".into());
    }
    if !repair.current(&state.peek()) || repair.original_current(&state.peek()) {
        return Err("Original member context changed or old source resurrected".into());
    }
    p["memberRelease"] = json!({"source":members::source(&state.peek()),"originalCurrent":repair.original_current(&state.peek()),"contextCurrent":repair.current(&state.peek())});
    take(state, &m, d, "source-ready", &p, &mut rows, false).await?;
    data::mode(&state.peek(), data::Mode::HeldSuccess)?;
    p["enter"] = probe(d, "key", json!({"key":"Enter"})).await?;
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
        return Err("Independent authored member readback missing".into());
    }
    wait_dom(
        d,
        "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
    )
    .await?;
    take(state, &m, d, "readback", &p, &mut rows, false).await?;
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":"own-edit-plain-member","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Synthetic DOM key/default input, not OS keyboard or full IME.","No-match is observed against accepted local canonical inventory, not server-wide absence.","Actual same-socket UserChanged native drift; explicit fixture-driven known-reader repair, not automatic Save recovery."]}),
    )
}
