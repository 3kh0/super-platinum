//! Real DOM controls and actual configured owner. Frame waits are rendering
//! waits only; storage outcomes come from the retained actual worker.
use super::*;
use std::time::Duration;
const NEWER: &str = "Newer native *input* 日本語 🚀 e\u{301}\nretained after review";
fn label(target: ComposerTarget) -> &'static str {
    if target == ComposerTarget::Thread {
        "thread"
    } else {
        "channel"
    }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Native predicate: {value}")),
        _ => Err("Native evaluation timed out".into()),
    }
}
async fn review(run: &Run, operation: &str, mut args: Value) -> Result<Value, String> {
    args["target"] = json!(label(run.target));
    eval(format!("{};try{{dioxus.send(await draftRemoteReviewCheck({},{}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",include_str!("dom.js"),json!(operation),args)).await
}
pub(super) async fn native(
    run: &Run,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",include_str!("../../../projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(run.target)),"text":text,"input":input}))).await
}
async fn wait(run: &Run, operation: &str) -> Result<Value, String> {
    let mut last = String::new();
    for _ in 0..300 {
        match review(run, operation, json!({})).await {
            Ok(value) => return Ok(value),
            Err(error) => last = error,
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("{operation} did not settle: {last}"))
}
fn shell_local(shell: &ShellState, target: ComposerTarget) -> LocalDraft {
    match target {
        ComposerTarget::Channel => LocalDraft {
            composer: shell.core.composer.clone(),
            attachments: shell.core.composer_attachments.clone(),
        },
        ComposerTarget::Thread => LocalDraft {
            composer: shell.core.thread_composer.clone(),
            attachments: shell.core.thread_composer_attachments.clone(),
        },
    }
}
async fn snapshot(state: Signal<ShellState>, run: &Run) -> Result<Value, String> {
    let editor = {
        let root = root().lock().map_err(|_| "Root lock")?;
        root.host()
            .installation(&run.account, &run.fixture.scope().key)
            .map(|v| v.editor().clone())
    };
    if let Some(editor) = &editor {
        // Flush can truthfully refuse stale/changed input; exact observed state
        // is serialized below rather than converting refusal to success.
        let _ = editor.flush().await;
    }
    let owner = run
        .account
        .owner(run.fixture.scope().key.clone())
        .map_err(|e| format!("Owner: {e:?}"))?;
    let actual = owner
        .review()
        .await
        .map_err(|e| format!("Actual inventory: {e:?}"))?;
    let reads=run.fixture.reads().into_iter().map(|read| {
        let value=read.read.map(|v| match v.response {
            durable::BaselineResponse::Positive{raw,user} => json!({"kind":"positive","raw":raw,"user":user,"knownUntil":v.known_until}),
            durable::BaselineResponse::Nonpositive(reason) => json!({"kind":"nonpositive","reason":format!("{reason:?}"),"knownUntil":v.known_until}),
        });
        json!({"scope":read.scope,"draftId":read.draft_id,"result":value})
    }).collect::<Vec<_>>();
    let operations=run.account.retained_operations().map_err(|_|"Retained operations")?.into_iter().map(|v| {
        let status=v.record().evidence.lock().map(|s|format!("{:?}",s.status)).unwrap_or_else(|_|"Poisoned".into());
        json!({"id":v.id(),"status":status,"baselinePending":v.baseline_pending(),"projectionObserved":v.baseline_projection_pending()})
    }).collect::<Vec<_>>();
    let action_root = dioxus::core::try_consume_context::<
        std::rc::Rc<crate::view::composer::protected_operations::Root>,
    >()
    .ok_or("Missing actual ActionRoot")?;
    let root_operations = action_root
        .fixture_retained_operations()
        .map_err(|_| "ActionRoot observation")?;
    let checkpoint=run.checkpoint.lock().map_err(|_|"Checkpoint observation")?.as_ref().map(|(v,barrier)| {
        let status=v.record().evidence.lock().map(|s|format!("{:?}",s.status)).unwrap_or_else(|_|"Poisoned".into());
        json!({"operation":v.id(),"barrier":barrier,"status":status,"syntheticComposition":true})
    });
    let projection = editor
        .as_ref()
        .and_then(|e| e.snapshot().projection.as_ref().map(|v| v.id().to_owned()));
    // These domain evidence types are intentionally not Serialize; emit every
    // public retained field without extending core serialization authority.
    let blocked = actual
        .blocked()
        .map(|v| json!({"token":v.token,"draft":v.draft}));
    let alternates = actual
        .alternates()
        .iter()
        .map(|v| json!({"writer_id":v.writer_id,"revision":v.revision,"draft":v.draft}))
        .collect::<Vec<_>>();
    let shell = state.peek();
    let installed = editor.as_ref().is_some_and(|e| {
        registry(&shell)
            .is_ok_and(|r| r.current(run.account.id(), run.epoch, &run.fixture.scope().key, e))
    });
    Ok(
        json!({"currentOwner":run.current(&shell),"sourceCurrent":socket::capture(&shell,"T1").is_ok_and(|c|c.current(&shell)),
        "configuredAccount":account(run.account.id()).is_ok_and(|a|Arc::ptr_eq(&a,&run.account)),"managedInstalled":installed,
        "local":actual.local(),"blocked":blocked,"alternates":alternates,"uncommitted":actual.uncommitted(),
        "rawState":actual.row().state(),"revision":actual.row().revision(),"shell":shell_local(&shell,run.target),
        "operations":operations,"rootOperations":root_operations,"checkpoint":checkpoint,"projection":projection,
        "reads":reads,"native":run.native.lock().map_err(|_|"Native evidence lock")?.clone()}),
    )
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    run: &Run,
    stage: &str,
    extra: Value,
) -> Result<Value, String> {
    isolation::validate(&run.config)?;
    let storage = snapshot(state, run).await?;
    let input = native(run, "read", None, false).await?;
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await?;
    eprintln!(
        "protected-owned-baseline-stage: {}",
        json!({"stage":stage,"edge":"before","nonce":run.config.nonce})
    );
    let path = run.config.captures.join(format!("{stage}.png"));
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    eprintln!(
        "protected-owned-baseline-stage: {}",
        json!({"stage":stage,"edge":"after","nonce":run.config.nonce})
    );
    Ok(json!({"stage":stage,"capture":path,"storage":storage,"input":input,"ui":extra}))
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    run: &Arc<Run>,
    worker: &mut socket::Worker,
) -> Result<Value, String> {
    for _ in 0..250 {
        if native(run, "ready", None, false).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    native(run, "show", None, false).await?;
    let mut stages = vec![capture(state, desktop, run, "initial", json!({})).await?];
    review(run, "open", json!({})).await?;
    if run.config.scenario == "cooldown-channel" {
        let first = wait(run, "feedback").await?;
        stages.push(capture(state, desktop, run, "rate-limited", first).await?);
        review(run, "open", json!({})).await?;
        let refused = wait(run, "feedback").await?;
        stages.push(capture(state, desktop, run, "reserve-cooldown", refused).await?);
        let deadline = run
            .fixture
            .reads()
            .first()
            .and_then(|v| v.read.as_ref())
            .map(|v| v.known_until)
            .ok_or("No actual cooldown deadline")?;
        // A bounded wait for the actual clock; no fabricated deadline or typed
        // claim Deferred. The next explicit click captures its own current time.
        let now = crate::drafts::send::now_ms();
        if deadline.saturating_sub(now) > 60_000 {
            return Err("Unexpected cooldown".into());
        }
        tokio::time::sleep(Duration::from_millis(deadline.saturating_sub(now) + 10)).await;
        review(run, "open", json!({})).await?;
    }
    let panel = wait(run, "read").await?;
    if panel["selected"] != 0 || panel["applyDisabled"] != true {
        return Err("Review selected a default".into());
    }
    stages.push(capture(state, desktop, run, "review", panel.clone()).await?);
    let rows = panel["rows"].as_array().ok_or("Missing rows")?;
    if rows.len() != 2 {
        return Err(format!("Unexpected exact fixture rows: {}", rows.len()));
    }
    for index in 0..rows.len() {
        let row = review(run, "row", json!({"index":index})).await?;
        stages.push(capture(state, desktop, run, &format!("row-{index}"), row).await?);
    }
    if run.config.scenario == "attachment-channel" {
        let refused = review(run, "disabled-current", json!({})).await?;
        stages.push(capture(state, desktop, run, "local-refused", refused).await?);
    }
    let local = run.config.scenario.starts_with("local-");
    let selected = review(
        run,
        "choose",
        json!({"kind":if local{"current"}else{"remote"}}),
    )
    .await?;
    if selected["selected"] != 1 || selected["applyDisabled"] != false {
        return Err("Explicit choice failed".into());
    }
    stages.push(capture(state, desktop, run, "selected", selected).await?);
    if run.config.scenario == "retire-channel" {
        worker.retire(&mut state.write());
        let mut refused = None;
        for _ in 0..100 {
            if let Ok(value) = review(run, "disabled-apply", json!({})).await {
                refused = Some(value);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        stages.push(
            capture(
                state,
                desktop,
                run,
                "retired-refused",
                refused.ok_or("Retired Source did not disable apply")?,
            )
            .await?,
        );
    } else {
        if run.config.scenario == "edit-channel" {
            let changed = native(run, "set", Some(NEWER), false).await?;
            stages.push(capture(state, desktop, run, "native-changed", changed).await?);
        }
        review(run, "apply", json!({})).await?;
        let feedback = wait(run, "feedback").await?;
        if run.config.scenario == "finish-channel" {
            let retained = snapshot(state, run).await?;
            let checkpoint = retained["checkpoint"].clone();
            if checkpoint["status"] != "Failed(ReviewRequired)"
                || retained["projection"] != checkpoint["barrier"]
                || retained["operations"]
                    .as_array()
                    .is_none_or(|v| v.len() != 1)
                || retained["rootOperations"] != json!([checkpoint["operation"]])
            {
                return Err("Original failed baseline operation was not retained".into());
            }
            stages.push(capture(state, desktop, run, "projection-deferred", feedback).await?);
            let ended = native(run, "composition-end", None, false).await?;
            if ended["composing"] != false {
                return Err("Synthetic composition did not end".into());
            }
            review(run, "finish", json!({})).await?;
            let feedback = wait(run, "feedback").await?;
            let final_state = snapshot(state, run).await?;
            if final_state["checkpoint"] != checkpoint
                || final_state["operations"] != json!([])
                || final_state["rootOperations"] != json!([])
                || !final_state["projection"].is_null()
            {
                return Err("Same-operation Finish cleanup failed".into());
            }
            stages.push(capture(state, desktop, run, "result", feedback).await?);
        } else {
            stages.push(capture(state, desktop, run, "result", feedback).await?);
        }
    }
    Ok(
        json!({"ok":true,"version":1,"nonce":run.config.nonce,"scenario":run.config.scenario,
        "target":label(run.target),"account":run.account.id(),"stages":stages,
        "execution":"actual-configured-root-managed-source-disconnected-read","trustedInput":false}),
    )
}
