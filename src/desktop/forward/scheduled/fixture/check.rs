//! Real mounted controls with closed synthetic response/identity injection.
use super::*;
use crate::state::{MainView, Overlay};
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    let Some(book) = try_consume_context::<Signal<Registry>>() else {
        eprintln!(
            "scheduled-forward-check: {}",
            json!({"ok":false,"error":"Root registry missing"})
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(120),
            run(state, book, &desktop),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Scheduled forward native timeout"}),
        };
        eprintln!("scheduled-forward-check: {result}");
    });
}
async fn dom(operation: &str, args: Value) -> Result<Value, String> {
    let script = format!(
        "{};try{{dioxus.send(await scheduledForwardCheck({},{}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",
        include_str!("check.js"),
        json!(operation),
        args
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let _ = eval.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Scheduled forward DOM: {v}")),
        _ => Err("Scheduled forward DOM timeout".into()),
    }
}
async fn ready(operation: &str, args: Value) -> Result<Value, String> {
    let mut error = String::new();
    for _ in 0..100 {
        match dom(operation, args.clone()).await {
            Ok(v) => return Ok(v),
            Err(e) => error = e,
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(error)
}
async fn click(root: &str, label: &str, actions: &mut Vec<Value>) -> Result<(), String> {
    actions.push(dom("click", json!({"root":root,"label":label})).await?);
    Ok(())
}
async fn settled(
    book: Signal<Registry>,
    expected: Option<model::Phase>,
) -> Result<model::Record, String> {
    for _ in 0..250 {
        {
            let root = book.peek();
            if root.entries.len() == 1 {
                let entry = root.entries.values().next().unwrap();
                if !entry.busy {
                    if let Some(record) = entry.record.as_ref() {
                        if expected.is_none_or(|phase| record.phase() == phase) {
                            return Ok(record.clone());
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Retained record did not settle to {expected:?}"))
}
async fn action_settled(
    root: &str,
    label: &str,
    book: Signal<Registry>,
    expected: model::Phase,
    actions: &mut Vec<Value>,
) -> Result<model::Record, String> {
    let before = book
        .peek()
        .entries
        .values()
        .next()
        .and_then(|e| e.record.as_ref())
        .map(model::Record::revision);
    click(root, label, actions).await?;
    for _ in 0..250 {
        {
            let registry = book.peek();
            if registry.entries.len() == 1 {
                let entry = registry.entries.values().next().unwrap();
                if !entry.busy {
                    if let Some(record) = &entry.record {
                        if record.phase() == expected
                            && before.is_none_or(|revision| record.revision() > revision)
                        {
                            return Ok(record.clone());
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Action {label} did not advance exact retained revision to {expected:?}"
    ))
}
async fn continued(book: Signal<Registry>) -> Result<(), String> {
    for _ in 0..250 {
        {
            let root = book.peek();
            if root.entries.len() == 1 {
                let entry = root.entries.values().next().unwrap();
                if !entry.busy
                    && entry.discovery.as_ref().is_some_and(|scan| {
                        entry.record.as_ref().is_some_and(|record| {
                            record.phase() == model::Phase::Unknown && scan.matches(record)
                        })
                    })
                {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("No explicit continuation was retained".into())
}
async fn stage(
    state: Signal<ShellState>,
    book: Signal<Registry>,
    fixture: &State,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    root: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let layout = dom("inspect", json!({"root":root})).await?;
    let records = {
        let book = book.peek();
        book.entries.values().map(|e|json!({"record":e.record,"busy":e.busy,"error":e.error,"pendingEdit":e.pending_edit,"localPending":e.local.is_some(),"completionPending":e.completion.is_some(),"unsentPending":e.unsent.is_some(),"continuing":e.discovery.as_ref().is_some_and(|scan|e.record.as_ref().is_some_and(|r|scan.matches(r)))})).collect::<Vec<_>>()
    };
    let image = harness::capture(d, name).await?;
    stages.push(json!({"name":name,"layout":layout,"records":records,"server":fixture.evidence(),"image":image,"account":state.peek().core.active_account,"epoch":state.peek().core.account_epoch}));
    Ok(())
}
async fn inventory(mut state: Signal<ShellState>) -> Result<(), String> {
    // Synthetic navigation is explicit. All record operations use real controls.
    {
        let mut shell = state.write();
        shell.overlay = None;
        shell.forward_target = None;
        shell.main_view = MainView::Scheduled;
    }
    ready(
        "focus",
        json!({"root":"#scheduled-forwards","label":"Refresh records"}),
    )
    .await?;
    Ok(())
}
async fn open_review(actions: &mut Vec<Value>) -> Result<(), String> {
    click("#scheduled-forwards", "Review note and time", actions).await?;
    ready(
        "focus",
        json!({"root":"#scheduled-forward-edit","label":"Cancel edits"}),
    )
    .await?;
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    book: Signal<Registry>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let fixture = install(state).await?;
    let initial_server = fixture.evidence();
    let mut stages = vec![];
    let mut actions = vec![];
    let mut keyboard_events = vec![];
    if fixture.phase == "restart" {
        inventory(state).await?;
        let record = settled(book, None).await?;
        stage(
            state,
            book,
            &fixture,
            d,
            "restart-inventory",
            "#scheduled-forwards",
            &mut stages,
        )
        .await?;
        if record.phase() != model::Phase::Deleted {
            open_review(&mut actions).await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "restart-review",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
        }
        if fixture.evidence() != initial_server {
            return Err("Restart performed a request or mutation".into());
        }
    } else {
        ready("ready", json!({})).await?;
        stage(
            state,
            book,
            &fixture,
            d,
            "initial",
            "#forward-dialog",
            &mut stages,
        )
        .await?;
        actions.push(dom("compose", json!({})).await?);
        let person = fixture.scenario == "person-rich";
        actions.push(
            dom(
                "select",
                if person {
                    json!({"query":"@nativeforwardrecipient","id":"person:U00000991"})
                } else {
                    json!({"query":"#","id":"C2"})
                },
            )
            .await?,
        );
        click("#forward-dialog", "Schedule later", &mut actions).await?;
        let time = i64::try_from(super::super::now() / 60 * 60 + 3600)
            .map_err(|_| "Fixture time overflow")?;
        actions.push(dom("date", json!({"seconds":time})).await?);
        actions.push(dom("private", json!({})).await?);
        keyboard_events = keyboard::observe(&fixture).await?;
        stage(
            state,
            book,
            &fixture,
            d,
            "configured",
            "#forward-dialog",
            &mut stages,
        )
        .await?;
        let phase = if fixture.scenario == "lost-readback" {
            model::Phase::Unknown
        } else {
            model::Phase::Confirmed
        };
        action_settled(
            "#forward-dialog",
            "Schedule forward",
            book,
            phase,
            &mut actions,
        )
        .await?;
        // Focus recovery is an actual product property, not a checker refocus.
        stage(
            state,
            book,
            &fixture,
            d,
            "created",
            "#forward-dialog",
            &mut stages,
        )
        .await?;
        inventory(state).await?;
        if fixture.scenario == "lost-readback" {
            click(
                "#scheduled-forwards",
                "Check scheduled forward",
                &mut actions,
            )
            .await?;
            continued(book).await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "discovery-page",
                "#scheduled-forwards",
                &mut stages,
            )
            .await?;
            action_settled(
                "#scheduled-forwards",
                "Continue checking older drafts",
                book,
                model::Phase::Confirmed,
                &mut actions,
            )
            .await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "readback-confirmed",
                "#scheduled-forwards",
                &mut stages,
            )
            .await?;
        }
        open_review(&mut actions).await?;
        actions.push(
            dom(
                "append",
                json!({"root":"#scheduled-forward-edit","text":" Native edit 日本語"}),
            )
            .await?,
        );
        stage(
            state,
            book,
            &fixture,
            d,
            "edited",
            "#scheduled-forward-edit",
            &mut stages,
        )
        .await?;
        if fixture.scenario == "conflict-review" {
            fixture.inject_conflict().await?;
            action_settled(
                "#scheduled-forward-edit",
                "Save scheduled forward",
                book,
                model::Phase::Conflict,
                &mut actions,
            )
            .await?;
            actions.push(
                dom(
                    "summary",
                    json!({"root":"#scheduled-forward-edit","label":"Observed version from Slack"}),
                )
                .await?,
            );
            stage(
                state,
                book,
                &fixture,
                d,
                "conflict-review",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
            click(
                "#scheduled-forward-edit",
                "Keep my requested note and time…",
                &mut actions,
            )
            .await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "resolution-confirmation",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
            action_settled(
                "#scheduled-forward-edit",
                "Review and keep my requested version",
                book,
                model::Phase::Ready,
                &mut actions,
            )
            .await?;
            if fixture.server.lock().unwrap().writes != 1 {
                return Err("Local resolution dispatched a write".into());
            }
            stage(
                state,
                book,
                &fixture,
                d,
                "resolution-ready",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
            action_settled(
                "#scheduled-forward-edit",
                "Save scheduled forward",
                book,
                model::Phase::Confirmed,
                &mut actions,
            )
            .await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "resolution-saved",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
        } else if fixture.scenario == "owner-retirement" {
            fixture.arm_read()?;
            click(
                "#scheduled-forward-edit",
                "Save scheduled forward",
                &mut actions,
            )
            .await?;
            fixture.wait_read().await?;
            let original = {
                let mut shell = state.write();
                let account = shell.core.active_account.clone();
                shell.core.active_account = Some("00000000-0000-4000-8000-000000000192".into());
                shell.core.account_epoch += 1;
                account
            };
            fixture.release_read();
            settled(book, Some(model::Phase::Ready)).await?;
            if fixture.server.lock().unwrap().writes != 1 {
                return Err("Retired owner dispatched another write".into());
            }
            {
                let mut shell = state.write();
                shell.core.active_account = original;
                shell.core.account_epoch += 1;
            }
            inventory(state).await?;
            open_review(&mut actions).await?;
            stage(
                state,
                book,
                &fixture,
                d,
                "retained-after-owner",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
        } else {
            action_settled(
                "#scheduled-forward-edit",
                "Save scheduled forward",
                book,
                if fixture.scenario == "lost-edit" {
                    model::Phase::Unknown
                } else {
                    model::Phase::Confirmed
                },
                &mut actions,
            )
            .await?;
            if fixture.scenario == "lost-edit" {
                settled(book, Some(model::Phase::Unknown)).await?;
                stage(
                    state,
                    book,
                    &fixture,
                    d,
                    "edit-unknown",
                    "#scheduled-forward-edit",
                    &mut stages,
                )
                .await?;
                click("#scheduled-forward-edit", "Close review", &mut actions).await?;
                action_settled(
                    "#scheduled-forwards",
                    "Check scheduled forward",
                    book,
                    model::Phase::Confirmed,
                    &mut actions,
                )
                .await?;
                if fixture.server.lock().unwrap().writes != 2 {
                    return Err("Readback repeated a mutation".into());
                }
                open_review(&mut actions).await?;
            } else {
                settled(book, Some(model::Phase::Confirmed)).await?;
            }
            stage(
                state,
                book,
                &fixture,
                d,
                "edit-confirmed",
                "#scheduled-forward-edit",
                &mut stages,
            )
            .await?;
            if matches!(fixture.scenario.as_str(), "ordered-rich" | "person-rich") {
                let record = action_settled(
                    "#scheduled-forward-edit",
                    "Cancel schedule; keep draft",
                    book,
                    model::Phase::Confirmed,
                    &mut actions,
                )
                .await?;
                if record.intent().post_at() != 0 {
                    return Err("Unschedule lost draft intent".into());
                }
                stage(
                    state,
                    book,
                    &fixture,
                    d,
                    "unscheduled",
                    "#scheduled-forward-edit",
                    &mut stages,
                )
                .await?;
                click(
                    "#scheduled-forward-edit",
                    "Delete scheduled draft…",
                    &mut actions,
                )
                .await?;
                stage(
                    state,
                    book,
                    &fixture,
                    d,
                    "delete-confirmation",
                    "#scheduled-forward-edit",
                    &mut stages,
                )
                .await?;
                action_settled(
                    "#scheduled-forward-edit",
                    "Delete draft",
                    book,
                    model::Phase::Deleted,
                    &mut actions,
                )
                .await?;
                stage(
                    state,
                    book,
                    &fixture,
                    d,
                    "deleted",
                    "#scheduled-forward-edit",
                    &mut stages,
                )
                .await?;
            }
        }
    }
    let storage = storage::Storage::capture(&state.peek())
        .ok_or("Current synthetic journal owner missing")?;
    let records = storage.page(None).await?;
    if records.len() != 1 {
        return Err("Expected one durable record".into());
    }
    let before = fixture.evidence();
    tokio::time::sleep(Duration::from_millis(300)).await;
    if fixture.evidence() != before {
        return Err("Automatic replay after settling".into());
    }
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"nonce":fixture.nonce,"scenario":fixture.scenario,"phase":fixture.phase,"pid":std::process::id(),"initialServer":initial_server,"server":before,"durableRecords":records,"actions":actions,"stages":stages,"keyboard":keyboard_events,"actualPreparedRequests":true,"syntheticInput":true,"noRealSession":true,"disconnectedTransport":true,"privateJournal":true,"syntheticNavigation":true,"noReplay":true}),
    )
}
