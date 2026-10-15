//! Disconnected native Send checks through the actual composer control and run.
//! Preparation only: copy beside send_fixture.rs after root releases integration.
use super::fixture;
use crate::drafts::{checkpoint, storage::Backend};
use crate::state::{ComposerTarget, MainView, PendingDeliveryStatus, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use super_platinum_core::{
    drafts::sync::{ConsumeIntent, OperationKind, Workflow},
    slack::models::Message,
};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Managed Send check timed out"}),
        };
        eprintln!("managed-draft-send-check: {receipt}");
    });
}

fn label(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Channel => "channel",
        ComposerTarget::Thread => "thread",
    }
}
fn ensure(value: bool, reason: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(reason.into()) }
}
async fn eval(script: String) -> Result<Value, String> {
    let mut evaluation = document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), evaluation.recv::<Value>()).await;
    let _ = evaluation.send(());
    match result {
        Ok(Ok(value)) if value["ok"] == true => Ok(value),
        Ok(Ok(value)) => Err(format!("Managed Send DOM: {value}")),
        _ => Err("Managed Send DOM timeout".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_| ())
}
async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
        include_str!("projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(target)),"text":text,"input":input}))).await
}
fn native_valid(value: &Value, text: &str) -> bool {
    value["text"] == text
        && value["display"] == text
        && value["codePoints"] == json!(text.chars().map(|c| c as u32).collect::<Vec<_>>())
        && value["utf16Length"] == json!(text.encode_utf16().count())
}
fn same_native(before: &Value, after: &Value) -> bool {
    [
        "text",
        "display",
        "codePoints",
        "utf16Length",
        "scope",
        "visit",
        "start",
        "end",
        "direction",
    ]
    .iter()
    .all(|key| before[*key] == after[*key])
}
async fn home(mut state: Signal<ShellState>, target: ComposerTarget) -> Result<Value, String> {
    {
        let mut shell = state.write();
        shell.main_view = MainView::Unreads;
        shell.thread_root = None;
    }
    let mut absent = false;
    for _ in 0..100 {
        if native(target, "absent", None, false).await.is_ok() {
            absent = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(absent, "Composer did not unmount")?;
    {
        let mut shell = state.write();
        shell.main_view = MainView::Home;
        shell.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        shell.sync_composer_drafts();
        if shell.thread_root.is_some() {
            shell.refresh_thread_from_core();
        }
    }
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    for _ in 0..150 {
        if let Ok(value) = native(target, "read", None, false).await
            && value["scope"] == scope
            && value["visit"].as_str().is_some_and(|v| !v.is_empty())
        {
            use super_platinum_core::domain::PendingScrollTarget;
            {
                let mut shell = state.write();
                let channel = shell.core.active_channel.clone().ok_or("Channel missing")?;
                shell.core.pending_scroll_to = Some((channel, PendingScrollTarget::Latest));
                shell.stick_to_bottom = true;
                shell.reset_timeline_window();
            }
            ensure(
                crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await,
                "Timeline anchor unavailable",
            )?;
            return native(target, "read", None, false).await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Composer mount unavailable".into())
}

/// Measure the complete field/footer, then activate the real Dioxus Send event.
/// This never calls send::run, consumes a draft, or constructs a fake Outcome.
async fn controls(target: ComposerTarget, click: bool) -> Result<Value, String> {
    eval(format!(r#"
const field=document.getElementById({}),footer=field?.closest('footer');
const button=footer?.querySelector('.composer-row > .composer-send-btn');
const pane=footer?.closest('.thread-pane,.conversation'),header=pane?.querySelector(':scope > header');
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const rect=e=>{{if(!e)return null;const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}};}};
const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1&&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
const sample=(e,r,rounded=false)=>{{
  const proof={{ok:false,method:rounded?'rounded-arc':'rect-grid',points:[]}};
  if(!e||!r||r.width<=0||r.height<=0)return proof;
  let points;
  if(rounded){{
    const style=getComputedStyle(e),names=['borderTopLeftRadius','borderTopRightRadius','borderBottomRightRadius','borderBottomLeftRadius'];
    proof.computedRadii=names.map(name=>style[name]);proof.inset=1;
    const radii=proof.computedRadii.map(value=>{{const parts=value.trim().split(/\s+/);return parts.length<=2&&parts.every(part=>/^\d+(?:\.\d+)?px$/.test(part))&&parts.every(part=>parseFloat(part)===parseFloat(parts[0]))?parseFloat(parts[0]):NaN;}});
    const radius=radii[0];
    if(!Number.isFinite(radius)||!radii.every(value=>value===radius)||radius>Math.min(r.width,r.height)/2||Math.min(r.width,r.height)<=2){{proof.error='Unsupported rounded Send hit shape';return proof;}}
    proof.radius=radius;
    // Square-corner samples can lie outside a rounded button. Keep all four
    // edge midpoints and sample each actual 45-degree arc one CSS px inward.
    const d=radius>1?radius-(radius-1)/Math.SQRT2:1;
    points=[['center',r.width/2,r.height/2],['top',r.width/2,1],['right',r.width-1,r.height/2],['bottom',r.width/2,r.height-1],['left',1,r.height/2],['top-left',d,d],['top-right',r.width-d,d],['bottom-right',r.width-d,r.height-d],['bottom-left',d,r.height-d]];
  }}else{{points=[.05,.5,.95].flatMap((x,i)=>[.05,.5,.95].map((y,j)=>[`${{i}}:${{j}}`,r.width*x,r.height*y]));}}
  proof.points=points.map(([name,dx,dy])=>{{const x=r.x+dx,y=r.y+dy,hit=document.elementFromPoint(x,y);return {{name,x,y,owned:!!hit&&e.contains(hit),element:hit?{{tag:hit.tagName,id:hit.id||'',classes:typeof hit.className==='string'?hit.className:hit.className?.baseVal||''}}:null}};}});
  proof.ok=proof.points.every(point=>point.owned);return proof;
}};
const f=rect(field),c=rect(footer),b=rect(button),p=rect(pane),h=rect(header);
const hitSamples={{field:sample(field,f),composer:sample(footer,c),button:sample(button,b,true)}};
const fieldUnobscured=hitSamples.field.ok,composerUnobscured=hitSamples.composer.ok,buttonUnobscured=hitSamples.button.ok;
const targetMatches=!!pane&&({}==='thread'?pane.classList.contains('thread-pane'):pane.classList.contains('conversation'));
const visible=targetMatches&&inside(c,p)&&inside(f,c)&&inside(b,c)&&inside(h,p)&&c.y>=h.y+h.height-1&&fieldUnobscured&&composerUnobscured&&buttonUnobscured;
const ok=!!field&&!!button&&!button.disabled&&visible;
const result={{ok,kind:'composer',target:{},clickRequested:{},clicked:false,field:f,composer:c,button:b,pane:p,header:h,hitSamples,fieldUnobscured,composerUnobscured,buttonUnobscured,targetMatches,disabled:button?.disabled,scope:field?.dataset.draftScope,visit:field?.dataset.projectionVisit,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}};
if({}&&ok){{button.focus({{preventScroll:true}});button.click();result.clicked=true;}}
result.fieldFocused=document.activeElement===field;result.buttonFocused=document.activeElement===button;
dioxus.send(result);await dioxus.recv();
"#, json!(format!("{}-composer",label(target))),json!(label(target)),json!(label(target)),json!(click),json!(click))).await
}
async fn measured(target: ComposerTarget) -> Result<Value, String> {
    let mut value = controls(target, false).await?;
    value["native"] = native(target, "read", None, false).await?;
    Ok(value)
}
async fn focus_state(target: ComposerTarget) -> Result<Value, String> {
    eval(format!("const field=document.getElementById({}),button=field?.closest('footer')?.querySelector('.composer-send-btn');dioxus.send({{ok:!!field&&!!button,fieldFocused:document.activeElement===field,buttonFocused:document.activeElement===button,activeId:document.activeElement?.id||''}});await dioxus.recv();",json!(format!("{}-composer",label(target))))).await
}
async fn blocked_feedback(state: Signal<ShellState>) -> Result<Value, String> {
    const TEXT: &str =
        "A saved message needs review in Drafts before sending another from this composer.";
    for _ in 0..150 {
        let model_ready = state
            .peek()
            .toast
            .as_ref()
            .is_some_and(|toast| toast.text == TEXT);
        if model_ready {
            if let Ok(mut rendered) = eval(format!(r#"
const toast=document.getElementById('status-toast'),r=toast?.getBoundingClientRect();
const visible=!!r&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&[.05,.5,.95].every(x=>[.05,.5,.95].every(y=>toast.contains(document.elementFromPoint(r.left+r.width*x,r.top+r.height*y))));
dioxus.send({{ok:visible&&toast.textContent==={},renderedText:toast?.textContent,rendered:visible,rect:r?{{x:r.x,y:r.y,width:r.width,height:r.height}}:null,viewport:[innerWidth,innerHeight]}});await dioxus.recv();
"#, json!(TEXT))).await {
                rendered["modelText"] = json!(TEXT);
                return Ok(rendered);
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Blocked Send did not produce visible shared admission feedback".into())
}
async fn flush(editor: &Arc<checkpoint::Editor>) -> Result<(), String> {
    let _ = editor.flush().await;
    for _ in 0..250 {
        if !editor.snapshot().running {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Managed Send editor did not settle".into())
}
async fn type_text(
    binding: &fixture::FixtureBinding,
    target: ComposerTarget,
    text: &str,
) -> Result<Value, String> {
    let routed = binding.routed.load(Ordering::Relaxed);
    native(target, "selection", None, false).await?;
    native(target, "set", Some(text), true).await?;
    for _ in 0..150 {
        if binding.routed.load(Ordering::Relaxed) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        binding.routed.load(Ordering::Relaxed) > routed,
        "Native input did not reach the isolated writer",
    )?;
    flush(&binding.editors[&fixture::key(target)]).await?;
    let durable = fixture::durable(binding).await?;
    ensure(
        durable[label(target)]["canonical"]["composer"]["text"] == text,
        "Native input did not persist exact text",
    )?;
    let value = native(target, "read", None, false).await?;
    ensure(
        native_valid(&value, text),
        "Native Unicode input differs from canonical text",
    )?;
    ensure(
        focus_state(target).await?["fieldFocused"] == true,
        "Typing field did not retain focus",
    )?;
    Ok(value)
}
async fn cleared(target: ComposerTarget) -> Result<Value, String> {
    for _ in 0..150 {
        if let Ok(value) = native(target, "read", None, false).await
            && native_valid(&value, "")
        {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Durably consumed native field did not clear".into())
}
fn messages(shell: &ShellState) -> Value {
    let channel = shell.core.workspaces["T1"].messages.get("C2");
    let thread = shell
        .core
        .threads
        .get(&("T1".into(), "C2".into(), fixture::ROOT.into()));
    let delivery: serde_json::Map<String, Value> = shell
        .pending_delivery
        .iter()
        .map(|(ts, status)| {
            (
                ts.clone(),
                json!(match status {
                    PendingDeliveryStatus::Failed => "Failed",
                    PendingDeliveryStatus::Unconfirmed => "Unconfirmed",
                }),
            )
        })
        .collect();
    json!({"channel":channel.map(|bag|&bag.messages),"thread":thread.map(|bag|&bag.messages),
        "pending":{"channel":channel.map(|bag|bag.pending.clone()).unwrap_or_default(),"thread":thread.map(|bag|bag.pending.clone()).unwrap_or_default()},
        "pending_delivery":delivery})
}
fn matching_messages<'a>(content: &'a Value, target: &str, client: &str) -> Vec<&'a Value> {
    content[target]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["client_msg_id"] == client)
        .collect()
}
fn verify_messages(
    initial: &Value,
    actual: &Value,
    target: ComposerTarget,
    intent: &ConsumeIntent,
    optimistic: &str,
    outcome: &str,
) -> Result<(), String> {
    ensure(
        matches!(outcome, "held" | "positive" | "lost"),
        "Unknown fixture message outcome",
    )?;
    ensure(
        optimistic != fixture::ACK_TS,
        "Optimistic timestamp was already a server acknowledgement",
    )?;
    ensure(
        optimistic
            .split_once('.')
            .is_some_and(|(seconds, fraction)| {
                seconds.parse::<u64>().is_ok_and(|value| value > 0)
                    && fraction.len() == 6
                    && seconds
                        .bytes()
                        .chain(fraction.bytes())
                        .all(|b| b.is_ascii_digit())
            }),
        "Optimistic timestamp is not an actual local message timestamp",
    )?;
    let positive = outcome == "positive";
    let expected = json!(Message {
        kind: Some("message".into()),
        user: Some("U00000001".into()),
        channel: Some("C2".into()),
        thread_ts: (target == ComposerTarget::Thread).then(|| fixture::ROOT.into()),
        subtype: (target == ComposerTarget::Thread).then(|| "thread_broadcast".into()),
        text: Some(fixture::REQUESTED.into()),
        blocks: intent.snapshot.blocks.clone(),
        client_msg_id: Some(intent.client_msg_id.clone()),
        ts: Some(
            if positive {
                fixture::ACK_TS
            } else {
                optimistic
            }
            .into()
        ),
        ..Default::default()
    });
    for destination in ["channel", "thread"] {
        ensure(
            matching_messages(initial, destination, &intent.client_msg_id).is_empty(),
            "Fixture already contained consumed client identity",
        )?;
        let expected_copy = destination == label(target)
            || (positive && target == ComposerTarget::Thread && destination == "channel");
        if expected_copy {
            let copies = matching_messages(actual, destination, &intent.client_msg_id);
            ensure(
                copies.len() == 1 && *copies[0] == expected,
                "New message differs from exact frozen routing/body/default fields",
            )?;
            let unrelated = actual[destination]
                .as_array()
                .ok_or("Message bag missing")?
                .iter()
                .filter(|message| message["client_msg_id"] != intent.client_msg_id)
                .cloned()
                .collect::<Vec<_>>();
            ensure(
                unrelated == initial[destination].as_array().cloned().unwrap_or_default(),
                "Send changed an unrelated message",
            )?;
        } else {
            ensure(
                actual[destination] == initial[destination],
                "Send appeared in an unrelated conversation or premature broadcast",
            )?;
        }
        if positive {
            ensure(
                !actual[destination]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|message| message["ts"] == optimistic),
                "Confirmed Send retained its optimistic message",
            )?;
        }
    }
    let mut pending = initial["pending"].clone();
    if !positive {
        pending[label(target)]
            .as_array_mut()
            .ok_or("Initial pending list missing")?
            .push(json!(optimistic));
    }
    ensure(
        actual["pending"] == pending,
        "Pending message timestamps differ from exact outcome",
    )?;
    let mut delivery = initial["pending_delivery"].clone();
    ensure(
        delivery
            .as_object()
            .is_some_and(|map| !map.contains_key(optimistic)),
        "Fixture already contained delivery status for optimistic timestamp",
    )?;
    if outcome == "lost" {
        delivery
            .as_object_mut()
            .unwrap()
            .insert(optimistic.into(), json!("Unconfirmed"));
    }
    ensure(
        actual["pending_delivery"] == delivery,
        "Send delivery status differs from exact outcome",
    )
}
async fn capture(
    desktop: &dioxus::desktop::DesktopContext,
    binding: &fixture::FixtureBinding,
    stage: &str,
    target: ComposerTarget,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    let mut measurement = measured(target).await?;
    measurement["stage"] = json!(stage);
    geometry.push(measurement);
    let directory = std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_SEND_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture alias")?;
    let path = std::path::PathBuf::from(directory).join(format!(
        "{alias}-{}-{}-{stage}.png",
        binding.scenario, binding.phase
    ));
    eprintln!(
        "managed-draft-send-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "managed-draft-send-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}

async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let binding = fixture::install(state).await?;
    fixture::wait_runner(&binding).await?;
    let target = binding.target();
    let key = fixture::key(target);
    let which = label(target);
    let other = if which == "thread" {
        "channel"
    } else {
        "thread"
    };
    let initial = fixture::durable(&binding).await?;
    ensure(
        state.peek().toast.is_none(),
        "Fixture started with stale admission feedback",
    )?;
    let initial_messages = messages(&state.peek());
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mut details = serde_json::Map::new();
    let mounted = home(state, target).await?;
    details.insert("nativeInitial".into(), mounted.clone());
    let positive = binding.scenario.starts_with("ack-newer-");
    let held = binding.scenario.starts_with("held-response-");
    let mut operation = Value::Null;
    let mut consumption = Value::Null;
    if binding.phase == "restart" {
        ensure(
            native_valid(&mounted, fixture::NEWER),
            "Restart did not restore newer canonical text",
        )?;
        ensure(
            initial[which]["canonical"]["composer"]["reply_broadcast"] == false,
            "Restart broadcast changed",
        )?;
        if positive {
            ensure(
                initial[which]["stored"].is_null(),
                "Confirmed managed row was not released",
            )?;
        } else {
            let view = binding
                .owner(&key)
                .load()
                .await
                .map_err(|e| format!("Restart load: {e:?}"))?;
            let stored = view.stored.ok_or("Unresolved Send lost managed state")?;
            ensure(
                matches!(stored.state.workflow(), Workflow::Uncertain(_)),
                "Restart lost uncertain Send",
            )?;
            operation = json!(stored.state.pending().ok_or("Restart operation missing")?);
            consumption = json!(
                stored
                    .state
                    .consumption()
                    .ok_or("Restart consumption missing")?
            );
            details.insert("beforeBlocked".into(), initial.clone());
            details.insert(
                "nativeBeforeBlocked".into(),
                native(target, "read", None, false).await?,
            );
            details.insert("blockedClick".into(), controls(target, true).await?);
            details.insert("blockedFeedback".into(), blocked_feedback(state).await?);
            let after = fixture::durable(&binding).await?;
            let after_native = native(target, "read", None, false).await?;
            ensure(
                after == initial,
                "Blocked restart Send changed durable inventory",
            )?;
            ensure(
                same_native(&details["nativeBeforeBlocked"], &after_native),
                "Blocked restart Send replaced native input",
            )?;
            ensure(
                binding.requests.lock().unwrap().is_empty(),
                "Restart replayed a request",
            )?;
            let focus = focus_state(target).await?;
            ensure(
                focus["buttonFocused"] == true && focus["fieldFocused"] == false,
                "Blocked Send changed clicked button focus",
            )?;
            details.insert("blockedFocus".into(), focus);
            let after_messages = messages(&state.peek());
            ensure(
                after_messages == initial_messages,
                "Blocked Send projected a message or delivery status",
            )?;
            details.insert("messagesAfterBlocked".into(), after_messages);
            details.insert("afterBlocked".into(), after);
            details.insert("nativeAfterBlocked".into(), after_native);
        }
        capture(
            desktop,
            &binding,
            "restart",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
    } else {
        ensure(
            native_valid(&mounted, fixture::BASE),
            "Fresh native composer differs from acknowledged base",
        )?;
        let view = binding
            .owner(&key)
            .load()
            .await
            .map_err(|e| format!("Initial load: {e:?}"))?;
        let stored = view.stored.ok_or("Acknowledged seed missing")?;
        ensure(
            matches!(stored.state.workflow(), Workflow::Idle)
                && stored.state.consumption().is_none()
                && stored.state.acknowledged().is_some(),
            "Seed must be acknowledged idle",
        )?;
        capture(
            desktop,
            &binding,
            "initial",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
        details.insert(
            "nativeInput".into(),
            type_text(&binding, target, fixture::REQUESTED).await?,
        );
        details.insert("beforeSend".into(), fixture::durable(&binding).await?);
        capture(
            desktop,
            &binding,
            "input",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
        binding.arm("response")?;
        details.insert("actualClick".into(), controls(target, true).await?);
        binding.wait_reached().await?;
        ensure(
            binding.requests.lock().unwrap().len() == 1,
            "Send did not prepare exactly one request",
        )?;
        let view = binding
            .owner(&key)
            .load()
            .await
            .map_err(|e| format!("Held load: {e:?}"))?;
        let stored = view.stored.ok_or("Consumption was not durable")?;
        let intent = stored
            .state
            .consumption()
            .ok_or("Send consumption missing")?
            .clone();
        let op = stored
            .state
            .pending()
            .ok_or("Prepared Send operation missing")?
            .clone();
        ensure(
            matches!(stored.state.workflow(), Workflow::InFlight(_))
                && op.kind == OperationKind::Send,
            "Expected actual InFlight Send",
        )?;
        ensure(
            op.snapshot.as_ref() == Some(&intent.snapshot)
                && op.base == intent.base
                && op.client_msg_id == intent.client_msg_id
                && op.consume_id.as_deref() == Some(intent.id.as_str()),
            "Send did not freeze exact consumed snapshot",
        )?;
        ensure(
            op.base
                .as_ref()
                .is_some_and(|base| base.snapshot != intent.snapshot),
            "Send did not replace acknowledged body with newer text",
        )?;
        let expected_snapshot = crate::drafts::codec::encode_composer(
            &key,
            &fixture::local(fixture::REQUESTED, target).composer,
        )
        .map_err(|_| "Requested fixture text could not be encoded")?;
        ensure(
            intent.snapshot == expected_snapshot,
            "Actual consumed snapshot differs from the native requested text",
        )?;
        operation = json!(op);
        consumption = json!(intent);
        let empty = cleared(target).await?;
        details.insert("nativeConsumed".into(), empty);
        let after_consume = fixture::durable(&binding).await?;
        ensure(
            after_consume[which]["canonical"]["composer"]["text"] == "",
            "Native field cleared before the exact durable local clear",
        )?;
        details.insert("heldAfterConsume".into(), after_consume);
        let held_messages = messages(&state.peek());
        let pending = matching_messages(&held_messages, which, &intent.client_msg_id);
        ensure(
            pending.len() == 1,
            "Actual Send did not create exactly one optimistic message",
        )?;
        let optimistic = pending[0]["ts"]
            .as_str()
            .ok_or("Optimistic message timestamp missing")?
            .to_owned();
        verify_messages(
            &initial_messages,
            &held_messages,
            target,
            &intent,
            &optimistic,
            "held",
        )?;
        details.insert("optimisticTs".into(), json!(optimistic));
        details.insert("messagesHeld".into(), held_messages);
        capture(
            desktop,
            &binding,
            "pending",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
        let newer = type_text(&binding, target, fixture::NEWER).await?;
        details.insert("nativeNewer".into(), newer.clone());
        let after_newer = fixture::durable(&binding).await?;
        details.insert("heldAfterTyping".into(), after_newer.clone());
        capture(
            desktop,
            &binding,
            "newer",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
        details.insert("duplicateClick".into(), controls(target, true).await?);
        frames().await?;
        ensure(
            binding.requests.lock().unwrap().len() == 1,
            "Duplicate Send prepared another request",
        )?;
        ensure(
            fixture::durable(&binding).await? == after_newer,
            "Duplicate held Send changed frozen inventory",
        )?;
        let before_release = native(target, "selection", None, false).await?;
        ensure(
            same_native(&newer, &before_release),
            "Restoring field focus changed newer typing or selection",
        )?;
        details.insert("nativeBeforeRelease".into(), before_release);
        let before_focus = focus_state(target).await?;
        ensure(
            before_focus["fieldFocused"] == true && before_focus["buttonFocused"] == false,
            "Before-response field focus unavailable",
        )?;
        details.insert("beforeReleaseFocus".into(), before_focus);
        if !held {
            binding.release();
            binding.wait_ui().await?;
            frames().await?;
        }
        let final_native = native(target, "read", None, false).await?;
        let final_focus = focus_state(target).await?;
        ensure(
            final_focus["fieldFocused"] == true && final_focus["buttonFocused"] == false,
            "Send response stole focus from newer typing",
        )?;
        details.insert("finalFocus".into(), final_focus);
        ensure(
            same_native(&newer, &final_native),
            "Send response replaced newer native typing or selection",
        )?;
        details.insert("nativeFinal".into(), final_native);
        let after = fixture::durable(&binding).await?;
        if positive {
            ensure(
                after[which]["stored"].is_null(),
                "Positive Send did not release managed row",
            )?;
        } else {
            let workflow = &after[which]["stored"]["state"]["workflow"];
            ensure(
                workflow[if held { "InFlight" } else { "Uncertain" }] == operation,
                "Unresolved Send changed exact frozen operation",
            )?;
            ensure(
                after[which]["stored"]["state"]["consumption"] == consumption,
                "Unresolved Send changed exact consumption",
            )?;
        }
        let final_messages = messages(&state.peek());
        verify_messages(
            &initial_messages,
            &final_messages,
            target,
            &intent,
            &optimistic,
            if positive {
                "positive"
            } else if held {
                "held"
            } else {
                "lost"
            },
        )?;
        details.insert("messagesFinal".into(), final_messages);
        capture(
            desktop,
            &binding,
            "result",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
    }
    let durable = fixture::durable(&binding).await?;
    ensure(
        durable[other] == initial[other],
        "Send changed unrelated destination inventory",
    )?;
    ensure(
        durable[which]["canonical"]["composer"]["text"] == fixture::NEWER,
        "Send discarded newer local draft",
    )?;
    ensure(
        durable[which]["canonical"]["composer"]["reply_broadcast"] == false,
        "Newer input inherited the consumed thread broadcast flag",
    )?;
    if binding.phase == "restart" {
        ensure(
            durable == initial,
            "Restart changed durable draft inventory",
        )?;
    }
    let attempts = binding.requests.lock().unwrap().clone();
    ensure(
        binding.refused.load(Ordering::Relaxed) == 0,
        "Isolated input routing left refused changes",
    )?;
    ensure(
        attempts.len() == usize::from(binding.phase == "exercise"),
        "Wrong actual request count",
    )?;
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"nonce":binding.nonce,"pid":std::process::id(),"scenario":binding.scenario,"phase":binding.phase,"isolatedDatabase":binding.path,"target":which,
        "noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noCache":state.peek().core.cache.is_none(),"noProductionRoute":matches!(&binding.account.backend,Backend::Isolated{..}),
        "syntheticInput":true,"actualSendControl":true,"actualPreparedRequests":true,"preparedRequests":attempts.len(),"attempts":attempts,"routedInputs":binding.routed.load(Ordering::Relaxed),"refusedInputs":binding.refused.load(Ordering::Relaxed),
        "operation":operation,"consumption":consumption,"initial":initial,"durable":durable,"messagesInitial":initial_messages,"captures":captures,"viewport":geometry[0]["viewport"],"geometry":geometry,"details":details,
        "limitations":["Synthetic DOM input/clicks and injected HTTP response; no trusted input or live Slack.","Held-response exits while actual Send response is pending; fresh restart normalizes only the retained operation uncertainty.","Message delivery is a fixture response; draft durability and request construction use the shared implementation."]}),
    )
}
