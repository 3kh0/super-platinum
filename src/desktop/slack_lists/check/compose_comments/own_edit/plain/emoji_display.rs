//! Actual original display emoji, original media completion, and guarded Save.
use super::*;
fn script(action: &str, arg: Value) -> String {
    format!(
        "({})({},{})",
        include_str!("emoji_display.js"),
        json!(action),
        arg
    )
}
async fn event(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(d, &script(action, arg)).await
}
fn initial_ops(tone: u8, appended: bool) -> Value {
    let mut ops = vec![
        json!({"insert":"Initial "}),
        json!({"insert":{"slackemoji":{"text":format!(":thumbsup::skin-tone-{tone}:")}}}),
        json!({"insert":" Custom "}),
        json!({"insert":{"slackemoji":{"text":":parrot_alias:"}}}),
    ];
    if appended {
        ops.extend([
            json!({"insert":" Typed "}),
            json!({"insert":{"slackemoji":{"text":":smile:"}}}),
            json!({"insert":" "}),
            json!({"insert":{"slackemoji":{"text":":smile:"}}}),
        ]);
    }
    ops.push(json!({"insert":"\n"}));
    json!(ops)
}
async fn settle(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    ops: &Value,
) -> Result<Value, String> {
    wait(state, m, |v| {
        v["capture"]["canSave"] == true
            && v["edit"]["active"]["plainDelta"]["ops"] == *ops
            && v["edit"]["active"]["nativePending"] == false
    })
    .await?;
    let native = wait_dom(d, &script("state", json!({"ops":ops}))).await?;
    Ok(json!({"native":native,"inventory":durable(state,ops).await?}))
}
async fn shot(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    p["displaySource"] = source::emoji_display::snapshot(&state.peek());
    let saved = matches!(name, "saved" | "readback");
    let glyphs = if saved {
        json!([
            "\u{1f44d}\u{1f3fe}",
            ":parrot_alias:",
            "\u{1f604}",
            "\u{1f604}"
        ])
    } else {
        json!(["\u{1f44d}\u{1f3fd}", ":parrot_alias:"])
    };
    let probe = script(
        "transcript",
        json!({"ts":data::RECEIVED_TS,"glyphs":glyphs}),
    );
    let before = wait_dom(d, &probe).await?;
    take(state, m, d, name, p, rows, false).await?;
    let after = checked(d, &probe).await?;
    for key in ["pane", "row", "body", "bodyText", "renderedText", "images"] {
        if before[key] != after[key] {
            return Err(format!(
                "Original transcript changed across {name} capture: {key}"
            ));
        }
    }
    for (old, new) in before["emojis"]
        .as_array()
        .ok_or("Missing original emoji nodes")?
        .iter()
        .zip(
            after["emojis"]
                .as_array()
                .ok_or("Missing captured emoji nodes")?,
        )
    {
        for key in [
            "node",
            "text",
            "rendered",
            "title",
            "codepoints",
            "connected",
            "painted",
        ] {
            if old[key] != new[key] {
                return Err(format!(
                    "Original emoji node changed across {name} capture: {key}"
                ));
            }
        }
    }
    rows.last_mut().ok_or("Original stage missing")?["transcript"] =
        json!({"before":before,"after":after});
    Ok(())
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
    shot(state, &m, d, "draft", &mut p, &mut rows).await?;

    if source::emoji_display::recovery() {
        p["failedOpen"] = command(d, "open", json!({"ts":data::RECEIVED_TS})).await?;
        wait(state, &m, |v| {
            v["edit"]["active"].is_null()
                && v["edit"]["opening"] == false
                && v["comments"]["comments"]["error"]
                    .as_str()
                    .is_some_and(|s| s.contains("could not be opened"))
        })
        .await?;
        shot(state, &m, d, "unavailable", &mut p, &mut rows).await?;
        open_plain(state, &m, d, &mut p).await?;
        p["initial"] = settle(state, &m, d, &initial_ops(4, false)).await?;
        shot(state, &m, d, "editing", &mut p, &mut rows).await?;
        p["escape"] = probe(d, "key", json!({"key":"Escape"})).await?;
        closed(state, &m, d).await?;
        shot(state, &m, d, "cancelled", &mut p, &mut rows).await?;
    } else {
        open_plain(state, &m, d, &mut p).await?;
        p["initial"] = settle(state, &m, d, &initial_ops(4, false)).await?;
        p["mediaBefore"] = event(d, "media-hold", Value::Null).await?;
        shot(state, &m, d, "display-pending", &mut p, &mut rows).await?;
        p["mediaDelivery"] = source::emoji_display::deliver_media(&state.peek())?;
        p["mediaAfter"] = wait_dom(d, &script("media-ready", Value::Null)).await?;
        shot(state, &m, d, "display-ready", &mut p, &mut rows).await?;
        p["append"] = event(d, "append", json!({"text":" Typed :smile: \u{1f604}"})).await?;
        p["typed"] = settle(state, &m, d, &initial_ops(4, true)).await?;
        shot(state, &m, d, "typed", &mut p, &mut rows).await?;
        let started = std::time::Instant::now();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        p["historyPauseMillis"] = json!(started.elapsed().as_millis());
        p["tone"] = event(d, "tone", json!({"label":":thumbsup::skin-tone-4:"})).await?;
        p["toneSettled"] = settle(state, &m, d, &initial_ops(5, true)).await?;
        p["undo"] = event(d, "undo", Value::Null).await?;
        p["undoSettled"] = settle(state, &m, d, &initial_ops(4, true)).await?;
        p["redo"] = event(d, "redo", Value::Null).await?;
        p["redoSettled"] = settle(state, &m, d, &initial_ops(5, true)).await?;
        shot(state, &m, d, "tone", &mut p, &mut rows).await?;
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        p["enter"] = probe(d, "key", json!({"key":"Enter"})).await?;
        wait_fixture(state, |v| v["held"] == true).await?;
        p["doubleSave"] = command(d, "double-save", Value::Null).await?;
        shot(state, &m, d, "saving", &mut p, &mut rows).await?;
        p["release"] = data::release(&state.peek())?;
        closed(state, &m, d).await?;
        shot(state, &m, d, "saved", &mut p, &mut rows).await?;
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
            return Err("Independent emoji display readback missing".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        shot(state, &m, d, "readback", &mut p, &mut rows).await?;
    }
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if source::emoji_display::recovery(){"own-edit-plain-emoji-display-recovery"}else{"own-edit-plain-emoji-display"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Synthetic DOM input/click/keys, not OS keyboard or full IME.","Actual fixture registry PNG completion, not network image downloading.","Recovery is unavailable initial catalog then a new actual Edit, not mounted Save Retry."]}),
    )
}
