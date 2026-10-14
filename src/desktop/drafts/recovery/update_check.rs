//! Native explicit Update check, interrupted-read restart, and preserved typing.
use super::{update_fixture as fixture, *};
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};
pub(crate) fn start(s: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(120), run(s, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Update recovery check timeout"}),
        };
        eprintln!("draft-update-recovery-check: {receipt}");
    });
}
fn label(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "thread"
    } else {
        "channel"
    }
}
fn ensure(value: bool, reason: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(reason.into()) }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut e = document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), e.recv::<Value>()).await;
    let _ = e.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Update DOM: {v}")),
        _ => Err("Update DOM timeout".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("../projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(target)),"text":text,"input":input}))).await
}
async fn home(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<Value, String> {
    // Real component unmount/remount restores stored text. Never sets DOM text.
    {
        let mut s = state.write();
        s.main_view = MainView::Unreads;
        s.thread_root = None;
    }
    let mut removed = false;
    for _ in 0..100 {
        if native(target, "absent", None, false).await.is_ok() {
            removed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !removed {
        return Err("Composer did not unmount".into());
    }
    {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        s.sync_composer_drafts();
        if s.thread_root.is_some() {
            s.refresh_thread_from_core();
        }
    }
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    for _ in 0..150 {
        if let Ok(v) = native(target, "read", None, false).await
            && v["scope"] == scope
            && v["visit"].as_str().is_some_and(|s| !s.is_empty())
        {
            use super_platinum_core::domain::PendingScrollTarget;
            {
                let mut s = state.write();
                let id = s.core.active_channel.clone().ok_or("Channel missing")?;
                s.core.pending_scroll_to = Some((id, PendingScrollTarget::Latest));
                s.stick_to_bottom = true;
                s.reset_timeline_window();
            }
            if !crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await {
                return Err("Timeline anchor unavailable".into());
            }
            return native(target, "show", None, false).await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Restored composer mount unavailable".into())
}
async fn drafts(mut s: Signal<ShellState>, op: &Operation) -> Result<Value, String> {
    {
        let mut shell = s.write();
        // Match the UI close flow: checkpoint both composers before removing
        // the thread overlay, keeping its draft available for home(Thread).
        shell.close_thread();
        shell.main_view = MainView::Scheduled;
        shell.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
    }
    let mut closed = false;
    for _ in 0..150 {
        if eval(
            "dioxus.send({ok:!document.querySelector('.thread-pane')});await dioxus.recv();".into(),
        )
        .await
        .is_ok()
        {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(closed, "Thread pane did not close before Drafts review")?;
    for _ in 0..150 {
        if let Ok(v) = row(op, false).await {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Update recovery row unavailable".into())
}
async fn row(op: &Operation, click: bool) -> Result<Value, String> {
    eval(format!(r#"
const host=[...document.querySelectorAll('[data-draft-operation]')].find(e=>e.dataset.draftOperation==={});
const article=host?.closest('article'),button=host?.querySelector('button'),body=article?.closest('.scheduled-body');
if(article)article.scrollIntoView({{block:'center',behavior:'instant'}});
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const rect=e=>{{const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}};
const a=article?rect(article):null,b=button?rect(button):null,p=body?rect(body):null;
const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1&&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
// Sample the full row and action, not only a button centre that may remain
// exposed while a thread overlay hides the surrounding explanation.
const unobscured=(e,r)=>!!e&&!!r&&[0.05,0.5,0.95].every(x=>[0.05,0.5,0.95].every(y=>e.contains(document.elementFromPoint(r.x+r.width*x,r.y+r.height*y))));
const threadPaneAbsent=!document.querySelector('.thread-pane');
const rowUnobscured=unobscured(article,a),buttonUnobscured=unobscured(button,b);
const hit=!!button&&button.contains(document.elementFromPoint(b.x+b.width/2,b.y+b.height/2));
const ok=!!host&&host.dataset.draftOperationKind==='update'&&threadPaneAbsent&&inside(a,p)&&inside(b,p)&&rowUnobscured&&buttonUnobscured&&hit;
const result={{ok,kind:host?.dataset.draftOperationKind,operation:host?.dataset.draftOperation,text:article?.innerText,buttonText:button?.textContent,disabled:button?.disabled,hit,threadPaneAbsent,rowUnobscured,buttonUnobscured,row:a,button:b,panel:p,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}};
if({}&&ok&&!button.disabled){{button.focus({{preventScroll:true}});button.click();}}
dioxus.send(result);await dioxus.recv();
"#,json!(op.id),json!(click))).await
}
async fn flush(editor: &Arc<checkpoint::Editor>) -> Result<(), String> {
    let _ = editor.flush().await;
    for _ in 0..250 {
        if !editor.snapshot().running {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Update editor did not settle".into())
}
async fn type_newer(b: &fixture::FixtureBinding, target: ComposerTarget) -> Result<Value, String> {
    let routed = b.routed.load(Ordering::Relaxed);
    native(target, "set", Some(fixture::NEWER), true).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Relaxed) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Relaxed) > routed,
        "Native input never reached owned writer",
    )?;
    flush(&b.editors[&fixture::key(target)]).await?;
    let saved = fixture::durable(b).await?;
    ensure(
        saved[label(target)]["canonical"]["composer"]["text"] == fixture::NEWER,
        "Newer input was not durably paired",
    )?;
    native(target, "read", None, false).await
}
fn content(s: &ShellState) -> Value {
    json!({"channel":s.core.workspaces["T1"].messages.get("C2").map(|b|&b.messages),"thread":s.core.threads.get(&("T1".into(),"C2".into(),fixture::ROOT.into())).map(|b|&b.messages)})
}
async fn capture(
    d: &dioxus::desktop::DesktopContext,
    b: &fixture::FixtureBinding,
    stage: &str,
    measurement: Value,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    let mut measurement = measurement;
    measurement["stage"] = json!(stage);
    geometry.push(measurement);
    let directory = std::env::var_os("SUPER_PLATINUM_DRAFT_UPDATE_RECOVERY_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let path = std::path::PathBuf::from(directory)
        .join(format!("{alias}-{}-{}-{stage}.png", b.scenario, b.phase));
    eprintln!(
        "draft-update-recovery-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "draft-update-recovery-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
fn native_valid(value: &Value, text: &str) -> bool {
    value["text"] == text
        && value["display"] == text
        && value["codePoints"] == json!(text.chars().map(|c| c as u32).collect::<Vec<_>>())
        && value["utf16Length"] == json!(text.encode_utf16().count())
}
async fn completed(b: &fixture::FixtureBinding, op: &Operation) -> Result<Value, String> {
    for _ in 0..250 {
        let view = b
            .owner(&op.scope.key)
            .load()
            .await
            .map_err(|e| format!("Load completion: {e:?}"))?;
        if view
            .stored
            .as_ref()
            .is_some_and(|s| s.state.pending_check(op).is_none())
        {
            return fixture::durable(b).await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Update receipt did not persist".into())
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(s).await?;
    fixture::wait_runner(&b).await?;
    let target = b.target();
    let key = fixture::key(target);
    let which = label(target);
    let other = if which == "thread" {
        "channel"
    } else {
        "thread"
    };
    let initial = fixture::durable(&b).await?;
    let messages = content(&s.peek());
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mut details = serde_json::Map::new();
    let owner = b.owner(&key);
    let view = owner
        .load()
        .await
        .map_err(|e| format!("Load operation: {e:?}"))?;
    let op = operation(&view);
    if b.phase == "restart" {
        if matches!(
            b.scenario.as_str(),
            "pending-check-channel" | "cooldown-channel"
        ) {
            let op = op.as_ref().ok_or("Restart lost uncertain operation")?;
            drafts(s, op).await?;
            let clicked = row(op, true).await?;
            ensure(
                !clicked["disabled"].as_bool().unwrap_or(true),
                "Restart check unavailable",
            )?;
            for _ in 0..150 {
                let v = row(op, false).await?;
                if v["disabled"] == false
                    && v["text"].as_str().is_some_and(|t| {
                        t.contains(if b.scenario == "pending-check-channel" {
                            "unresolved"
                        } else {
                            "cooldown"
                        })
                    })
                {
                    details.insert("restartRefusal".into(), v);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            ensure(
                details.contains_key("restartRefusal"),
                "Restart did not visibly refuse duplicate check",
            )?;
            capture(
                d,
                &b,
                "restart",
                row(op, false).await?,
                &mut captures,
                &mut geometry,
            )
            .await?;
        } else {
            let v = home(s, target).await?;
            ensure(
                native_valid(
                    &v,
                    if b.scenario.starts_with("typing-") {
                        fixture::NEWER
                    } else {
                        fixture::REQUESTED
                    },
                ),
                "Restart native text mismatch",
            )?;
            details.insert("native".into(), v.clone());
            capture(d, &b, "restart", v, &mut captures, &mut geometry).await?;
        }
        ensure(
            b.requests.lock().unwrap().is_empty(),
            "Restart dispatched read",
        )?;
    } else {
        let op = op.as_ref().ok_or("Seed must be an uncertain Update")?;
        ensure(
            op.kind == OperationKind::Update && op.base.is_some(),
            "Wrong frozen operation",
        )?;
        capture(
            d,
            &b,
            "initial",
            drafts(s, op).await?,
            &mut captures,
            &mut geometry,
        )
        .await?;
        if b.scenario == "typing-before-channel" {
            home(s, target).await?;
            let v = type_newer(&b, target).await?;
            ensure(
                native_valid(&v, fixture::NEWER),
                "Newer input was not routed",
            )?;
            details.insert("input".into(), v.clone());
            capture(d, &b, "input", v, &mut captures, &mut geometry).await?;
            drafts(s, op).await?;
        }
        b.arm(if b.scenario == "typing-during-commit-thread" {
            "commit"
        } else {
            "read"
        })?;
        let clicked = row(op, true).await?;
        ensure(clicked["disabled"] == false, "Check was initially disabled")?;
        b.wait_reached().await?;
        let held = fixture::durable(&b).await?;
        ensure(
            held[which]["stored"]["state"]["check_ticket"]["operation"] == json!(op),
            "Read preceded exact durable ticket",
        )?;
        ensure(
            b.requests.lock().unwrap().len() == 1,
            "Expected one actual prepared read",
        )?;
        details.insert("held".into(), held);
        capture(
            d,
            &b,
            "checking",
            row(op, false).await?,
            &mut captures,
            &mut geometry,
        )
        .await?;
        if b.scenario == "typing-during-commit-thread" {
            home(s, target).await?;
            let v = type_newer(&b, target).await?;
            ensure(
                native_valid(&v, fixture::NEWER),
                "During-commit input was not routed",
            )?;
            details.insert("input".into(), v.clone());
            capture(d, &b, "input", v, &mut captures, &mut geometry).await?;
        }
        if b.scenario == "owner-rotation-channel" {
            home(s, target).await?;
            details.insert(
                "managedBeforeRotation".into(),
                json!(
                    s.peek()
                        .managed_drafts
                        .entry(fixture::ACCOUNT, &key)
                        .and_then(|v| v.stored.as_ref())
                        .map(|v| v.revision)
                ),
            );
            let before_epoch = s.peek().core.account_epoch;
            s.write().core.account_epoch += 1;
            details.insert("ownerRotation".into(),json!({"beforeEpoch":before_epoch,"afterEpoch":s.peek().core.account_epoch,"managedBefore":details["managedBeforeRotation"]}));
        }
        if b.scenario != "pending-check-channel" {
            b.release();
            let persisted = completed(&b, op).await?;
            b.wait_ui().await?;
            details.insert("afterReceipt".into(), persisted.clone());
            if b.scenario == "cooldown-channel" {
                ensure(
                    persisted[which]["stored"]["state"]["workflow"]["Uncertain"] == json!(op),
                    "429 released frozen operation",
                )?;
                for _ in 0..150 {
                    if row(op, false).await.is_ok_and(|v| v["disabled"] == false) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                row(op, true).await?;
                frames().await?;
                ensure(
                    b.requests.lock().unwrap().len() == 1,
                    "Cooldown replayed read",
                )?;
                capture(
                    d,
                    &b,
                    "result",
                    row(op, false).await?,
                    &mut captures,
                    &mut geometry,
                )
                .await?;
            } else {
                ensure(
                    persisted[which]["stored"]["state"]["workflow"] == "Idle",
                    "Positive Update not confirmed",
                )?;
                let v = if b.scenario == "typing-during-commit-thread" {
                    frames().await?;
                    native(target, "read", None, false).await?
                } else {
                    home(s, target).await?
                };
                ensure(
                    native_valid(
                        &v,
                        if b.scenario.starts_with("typing-") {
                            fixture::NEWER
                        } else {
                            fixture::REQUESTED
                        },
                    ),
                    "Receipt replaced current text",
                )?;
                if b.scenario == "typing-during-commit-thread" {
                    let before = &details["input"];
                    for field in ["visit", "scope", "start", "end", "direction"] {
                        ensure(
                            v[field] == before[field],
                            "Receipt replaced native visit or selection",
                        )?;
                    }
                }
                if b.scenario == "owner-rotation-channel" {
                    ensure(
                        json!(
                            s.peek()
                                .managed_drafts
                                .entry(fixture::ACCOUNT, &key)
                                .and_then(|v| v.stored.as_ref())
                                .map(|v| v.revision)
                        ) == details["managedBeforeRotation"],
                        "Old owner projected receipt into new epoch",
                    )?;
                    details.get_mut("ownerRotation").unwrap()["managedAfter"] = json!(
                        s.peek()
                            .managed_drafts
                            .entry(fixture::ACCOUNT, &key)
                            .and_then(|v| v.stored.as_ref())
                            .map(|v| v.revision)
                    );
                }
                details.insert("native".into(), v.clone());
                capture(d, &b, "result", v, &mut captures, &mut geometry).await?;
            }
        } else {
            capture(
                d,
                &b,
                "result",
                row(op, false).await?,
                &mut captures,
                &mut geometry,
            )
            .await?;
        }
    }
    let durable = fixture::durable(&b).await?;
    ensure(
        durable[other] == initial[other],
        "Unrelated destination changed",
    )?;
    if b.phase == "restart" {
        ensure(
            durable == initial,
            "Restart changed complete durable inventory",
        )?;
    }
    let shell = s.peek();
    ensure(
        shell.core.session.is_none()
            && shell.core.transport.is_none()
            && content(&shell) == messages,
        "Isolation lost or canonical messages changed",
    )?;
    let expected = if b.scenario.starts_with("typing-") {
        fixture::NEWER
    } else {
        fixture::REQUESTED
    };
    ensure(
        durable[which]["canonical"]["composer"]["text"] == expected,
        "Durable current text mismatch",
    )?;
    let attempts = b.requests.lock().unwrap().clone();
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"nonce":b.nonce,"pid":std::process::id(),"scenario":b.scenario,"phase":b.phase,"isolatedDatabase":b.path,"target":which,"noSession":true,"noTransport":true,"noProductionRoute":matches!(&b.account.backend,Backend::Isolated{..}),"syntheticInput":true,"actualPreparedRequests":true,"canonicalMessagesUnchanged":true,"preparedRequests":attempts.len(),"attempts":attempts,"routedInputs":b.routed.load(Ordering::Relaxed),"operation":op,"initial":initial,"durable":durable,"captures":captures,"viewport":geometry[0]["viewport"],"geometry":geometry,"details":details,"limitations":["No live Slack, autosave, or realtime ingress; one isolated drafts.info read request only.","Owner rotation rejects UI projection; captured-account receipt persistence remains authorized.","Wrong identity/version/content and failed storage are separate unit-test evidence.","Pending-check intentionally exits with unresolved durable ticket and held response."]}),
    )
}
