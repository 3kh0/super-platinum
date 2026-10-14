//! Drives real mounted attachment controls; only responses and identity changes are synthetic.
use super::*;
use super_platinum_core::scheduled::edit::EditPhase;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    let Some(operations) = try_consume_context::<Signal<Operations>>() else {
        eprintln!(
            "scheduled-attachments-check: {}",
            json!({"ok":false,"error":"App-root Operations context missing"})
        );
        return;
    };
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(Duration::from_secs(100), run(state, operations, &desktop))
                .await
            {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => json!({"ok":false,"error":e}),
                Err(_) => json!({"ok":false,"error":"Attachment native check timed out"}),
            };
        eprintln!("scheduled-attachments-check: {receipt}");
    });
}
async fn dom(operation: &str, args: Value) -> Result<Value, String> {
    let source = format!(
        "{};try{{dioxus.send(await scheduledAttachmentCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
        include_str!("check.js"),
        json!(operation),
        args
    );
    let mut eval = dioxus::document::eval(&source);
    let result = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let _ = eval.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Native attachment control: {v}")),
        _ => Err("Native attachment receipt unavailable".into()),
    }
}
async fn wait_dom(operation: &str) -> Result<Value, String> {
    for _ in 0..150 {
        if let Ok(v) = dom(operation, json!({})).await {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Native {operation} did not settle"))
}
fn lease(state: Signal<ShellState>, ops: Signal<Operations>, id: &str) -> Result<Lease, String> {
    ops.peek()
        .retained(&state.peek())
        .into_iter()
        .find(|l| l.draft_id() == id)
        .ok_or("Actual mounted attachment lease missing".into())
}
async fn phase(
    ops: Signal<Operations>,
    lease: &Lease,
    expected: EditPhase,
) -> Result<Snapshot, String> {
    for _ in 0..250 {
        if let Ok(s) = ops.peek().snapshot(lease)
            && s.phase == expected
        {
            return Ok(s);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Attachment phase did not become {expected:?}"))
}
async fn stage(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    captures: &mut serde_json::Map<String, Value>,
    layouts: &mut Vec<Value>,
) -> Result<(), String> {
    let layout = dom("layout", json!({})).await?;
    layouts.push(json!({"stage":name,"layout":layout}));
    captures.insert(name.into(), capture(d, name).await?);
    Ok(())
}
async fn open(
    state: Signal<ShellState>,
    ops: Signal<Operations>,
    id: &str,
) -> Result<Lease, String> {
    dom("open", json!({"id":id})).await?;
    for _ in 0..100 {
        if let Ok(lease) = lease(state, ops, id) {
            return Ok(lease);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Attachment editor did not open".into())
}
async fn action(label: &str) -> Result<(), String> {
    dom("action", json!({"label":label})).await.map(|_| ())
}
async fn remove(index: usize) -> Result<(), String> {
    dom("remove", json!({"index":index})).await.map(|_| ())
}
async fn no_replay(fixture: &State, count: usize) -> Result<(), String> {
    tokio::time::sleep(Duration::from_millis(300)).await;
    if fixture.count() != count {
        return Err("Unexpected automatic request or replay".into());
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    mut ops: Signal<Operations>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let fixture = install(state).await?;
    wait_runner(state).await?;
    let mut captures = serde_json::Map::new();
    let mut layouts = Vec::new();
    let mut facts = json!({});
    if fixture.scenario == "empty-body-cancel" {
        dom("tab", json!({"label":"Drafts"})).await?;
        let empty = open(state, ops, EMPTY).await?;
        remove(0).await?;
        action("Save changes").await?;
        let blocked = phase(ops, &empty, EditPhase::Editing).await?;
        if blocked.error.as_deref()
            != Some(
                "Keep an attachment or add a message in the message editor before removing the last file.",
            )
            || fixture.count() != 0
        {
            return Err("Empty-body last file was not blocked before any read/write".into());
        }
        stage(d, "empty-guard", &mut captures, &mut layouts).await?;
        dom("undo", json!({"index":0})).await?;
        stage(d, "empty-restored", &mut captures, &mut layouts).await?;
        action("Cancel").await?;
        wait_dom("absent").await?;
        dom("tab", json!({"label":"Scheduled"})).await?;
        let owned = open(state, ops, DRAFT).await?;
        remove(0).await?;
        fixture.arm_read()?;
        action("Save changes").await?;
        fixture.wait_read().await?;
        phase(ops, &owned, EditPhase::Prepared).await?;
        stage(d, "checking", &mut captures, &mut layouts).await?;
        action("Cancel").await?;
        wait_dom("absent").await?;
        fixture.release_read();
        for _ in 0..100 {
            if fixture
                .ledger
                .lock()
                .unwrap()
                .first()
                .is_some_and(|r| !r["result"].is_null())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        no_replay(&fixture, 1).await?;
        if ops.peek().snapshot(&owned).is_ok() {
            return Err("Canceled preflight lease was retained".into());
        }
        let reopened = open(state, ops, DRAFT).await?;
        let s = phase(ops, &reopened, EditPhase::Editing).await?;
        if s.changed || s.rows.iter().any(|r| r.removed) {
            return Err("Canceled staged file removal leaked into reopened editor".into());
        }
        stage(d, "canceled", &mut captures, &mut layouts).await?;
        facts = json!({"emptyBodyGuard":true,"cancelBeforeDispatch":true,"noMutation":true});
    } else {
        if fixture.scenario == "ordered-rich" {
            dom("legacy-open", json!({"id":DRAFT})).await?;
            let reservation_owner = owner(&state.peek(), DRAFT).ok_or("Synthetic owner missing")?;
            let draft = state.peek().core.scheduled.items[DRAFT].clone();
            if ops.write().open(reservation_owner, draft).is_ok() {
                return Err("Actual legacy editor failed to reserve the draft".into());
            }
            facts["actualLegacyEditorReserved"] = json!(true);
            dom("legacy-close", json!({})).await?;
            for _ in 0..100 {
                let owner = owner(&state.peek(), DRAFT).unwrap();
                if !ops.peek().blocked(&owner) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        let owned = open(state, ops, DRAFT).await?;
        stage(d, "initial", &mut captures, &mut layouts).await?;
        if fixture.scenario == "ordered-rich" {
            remove(1).await?;
            stage(d, "removed", &mut captures, &mut layouts).await?;
            dom("undo", json!({"index":1})).await?;
            if ops
                .peek()
                .snapshot(&owned)
                .map_err(|_| "Snapshot missing")?
                .changed
            {
                return Err("Undo changed original ordered files".into());
            }
            stage(d, "undo", &mut captures, &mut layouts).await?;
        }
        remove(0).await?;
        stage(d, "selected", &mut captures, &mut layouts).await?;
        action("Save changes").await?;
        match fixture.scenario.as_str() {
            "ordered-rich" => {
                phase(ops, &owned, EditPhase::Confirmed).await?;
                stage(d, "confirmed", &mut captures, &mut layouts).await?;
                action("Close").await?;
                wait_dom("absent").await?;
                let last = open(state, ops, DRAFT).await?;
                remove(0).await?;
                remove(1).await?;
                stage(d, "last-file", &mut captures, &mut layouts).await?;
                action("Save changes").await?;
                phase(ops, &last, EditPhase::Confirmed).await?;
                stage(d, "empty-confirmed", &mut captures, &mut layouts).await?;
                facts["orderedRemoveUndo"] = json!(true);
                facts["nonemptyBodyLastFile"] = json!(true);
            }
            "uncertain-owner" => {
                phase(ops, &owned, EditPhase::Uncertain).await?;
                stage(d, "uncertain", &mut captures, &mut layouts).await?;
                action("Close").await?;
                wait_dom("absent").await?;
                dom("blocked", json!({"id":DRAFT})).await?;
                if ops
                    .write()
                    .reserve_other(owner(&state.peek(), DRAFT).unwrap())
                    .is_ok()
                {
                    return Err(
                        "Uncertain attachment operation allowed competing reservation".into(),
                    );
                }
                fixture.rotate(state)?;
                {
                    let mut s = state.write();
                    s.core.active_account = Some("00000000-0000-4000-8000-000000000099".into());
                    s.core.account_epoch += 1;
                }
                if !ops.peek().retained(&state.peek()).is_empty() {
                    return Err("Foreign account saw original receipt".into());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                {
                    let mut s = state.write();
                    s.core.active_account = Some(ACCOUNT.into());
                    s.core.account_epoch += 1;
                    let epoch = s.core.account_epoch;
                    s.core.scheduled.ensure_scope(epoch, "T1");
                    for raw in fixture.server.lock().unwrap().values() {
                        s.core
                            .scheduled
                            .accept_remote(serde_json::from_value(raw.clone()).unwrap());
                    }
                }
                let retained = lease(state, ops, DRAFT)?;
                if retained != owned {
                    return Err("Uncertain owner rotation replaced retained lease".into());
                }
                dom("resume", json!({})).await?;
                phase(ops, &retained, EditPhase::Uncertain).await?;
                stage(d, "resumed", &mut captures, &mut layouts).await?;
                no_replay(&fixture, 2).await?;
                action("Review current draft").await?;
                phase(ops, &retained, EditPhase::Confirmed).await?;
                stage(d, "reviewed", &mut captures, &mut layouts).await?;
                no_replay(&fixture, 3).await?;
                facts = json!({"uncertainNoReplay":true,"stableOwnerRetained":true,"foreignAccountHidden":true,"competingReservationBlocked":true,"positiveReadbackOnly":true,"syntheticCredentialGenerationRotation":true});
            }
            _ => {
                let s = phase(ops, &owned, EditPhase::Editing).await?;
                if s.error.is_none() || !s.changed {
                    return Err("Recoverable failure lost staged files or explanation".into());
                }
                stage(d, "recoverable", &mut captures, &mut layouts).await?;
                no_replay(&fixture, 2).await?;
                fixture.rotate(state)?;
                action("Save changes").await?;
                phase(ops, &owned, EditPhase::Confirmed).await?;
                stage(d, "confirmed", &mut captures, &mut layouts).await?;
                no_replay(&fixture, 4).await?;
                facts = json!({"explicitRetryOnly":true,"stagedSelectionRetained":true,"syntheticCredentialGenerationRotation":true,"knownUnsent":fixture.scenario=="known-unsent","definiteRawRejection":fixture.scenario=="rejected"});
            }
        }
    }
    let shell = state.peek();
    if !fixture.admits(&shell) {
        return Err("Disconnected ownership changed at completion".into());
    }
    let ledger = fixture.ledger.lock().unwrap().clone();
    if ledger.iter().any(|r| r["result"].is_null()) {
        return Err("Unfinished fixture request".into());
    }
    let updates = ledger
        .iter()
        .filter(|r| {
            r["url"].as_str().is_some_and(|s| {
                s.split('?')
                    .next()
                    .is_some_and(|p| p.ends_with("/drafts.update"))
            })
        })
        .count();
    let expected = match fixture.scenario.as_str() {
        "empty-body-cancel" => 0,
        "uncertain-owner" => 1,
        _ => 2,
    };
    if updates != expected {
        return Err("Unexpected update/replay count".into());
    }
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"scenario":fixture.scenario,"nonce":fixture.nonce,"pid":std::process::id(),
        "noSession":true,"noTransport":true,"syntheticInput":true,"noFileDeletion":true,"noReplay":true,"actualPreparedRequests":true,
        "preparedRequests":ledger.len(),"updateRequests":updates,"attempts":ledger,"server":fixture.server(),"facts":facts,"layouts":layouts,
        "captures":captures,"viewport":layouts.last().ok_or("No geometry evidence")?["layout"]["viewport"]}),
    )
}
