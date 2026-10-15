//! Disconnected native Schedule creation/rejection checks through actual UI events.
//! Only Account storage and HTTP execution are isolated; no synthetic domain outcome.
use crate::drafts::{checkpoint, schedule::fixture};
use crate::state::{ComposerTarget, MainView, PendingDeliveryStatus, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use super_platinum_core::{
    drafts::sync::{OperationKind, Workflow},
    scheduled::DraftFilter,
};
#[path = "managed_schedule_check/helpers.rs"]
mod helpers;
use helpers::*;
#[path = "managed_schedule_check/retry.rs"]
mod retry;

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Managed Schedule check timed out"}),
        };
        eprintln!("managed-draft-schedule-check: {receipt}");
    });
}
async fn dom(target: ComposerTarget, op: &str, extra: Value) -> Result<Value, String> {
    let mut input = extra;
    input["target"] = json!(label(target));
    input["op"] = json!(op);
    eval(format!("{};try{{dioxus.send(await managedScheduleCheck({}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("managed_schedule_check/dom.js"),input)).await
}
async fn wait_dom(target: ComposerTarget, op: &str) -> Result<Value, String> {
    wait_dom_with(target, op, json!({})).await
}
async fn wait_dom_with(target: ComposerTarget, op: &str, extra: Value) -> Result<Value, String> {
    let mut last = String::new();
    for _ in 0..150 {
        match dom(target, op, extra.clone()).await {
            Ok(value) => return Ok(value),
            Err(error) => last = error,
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Schedule native state did not settle: {op}; last: {last}"
    ))
}
async fn panel(mut state: Signal<ShellState>, scheduled: bool) -> Result<(), String> {
    {
        let mut shell = state.write();
        shell.close_thread();
        shell.main_view = MainView::Scheduled;
        shell.core.scheduled.filter = if scheduled {
            DraftFilter::Scheduled
        } else {
            DraftFilter::Drafts
        };
    }
    wait_dom(ComposerTarget::Channel, "panel-ready").await?;
    Ok(())
}
async fn capture(
    desktop: &dioxus::desktop::DesktopContext,
    binding: &fixture::Binding,
    stage: &str,
    target: ComposerTarget,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    if matches!(
        stage,
        "review" | "pending" | "retry-review" | "retry-pending"
    ) {
        capture_one(
            desktop,
            binding,
            stage,
            target,
            captures,
            geometry,
            Some("editor"),
        )
        .await?;
        capture_one(
            desktop,
            binding,
            &format!("{stage}-date"),
            target,
            captures,
            geometry,
            Some("date"),
        )
        .await
    } else {
        capture_one(desktop, binding, stage, target, captures, geometry, None).await
    }
}
async fn capture_one(
    desktop: &dioxus::desktop::DesktopContext,
    binding: &fixture::Binding,
    stage: &str,
    target: ComposerTarget,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
    body_view: Option<&str>,
) -> Result<(), String> {
    frames().await?;
    let body_scroll = if let Some(view) = body_view {
        let mut scroll = dom(target, "reveal", json!({"view":view})).await?;
        scroll.as_object_mut().unwrap().remove("ok");
        scroll
    } else {
        Value::Null
    };
    let mut measurement = wait_dom_with(target, "measure", json!({"view":body_view})).await?;
    measurement["bodyScroll"] = body_scroll;
    measurement["stage"] = json!(stage);
    geometry.push(measurement);
    let directory = std::env::var_os("SUPER_PLATINUM_MANAGED_DRAFT_SCHEDULE_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture alias")?;
    let path = std::path::PathBuf::from(directory).join(format!(
        "{alias}-{}-{}-{stage}.png",
        binding.scenario, binding.phase
    ));
    eprintln!(
        "managed-draft-schedule-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "managed-draft-schedule-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
fn contains_type(value: &Value, kind: &str) -> bool {
    value.get("type").and_then(Value::as_str) == Some(kind)
        || match value {
            Value::Array(rows) => rows.iter().any(|v| contains_type(v, kind)),
            Value::Object(rows) => rows.values().any(|v| contains_type(v, kind)),
            _ => false,
        }
}
fn changed_blocks(target: ComposerTarget) -> Value {
    let mut expected = json!(fixture::rich_snapshot(target).blocks);
    expected[0]["elements"][0]["elements"][0]["text"] = json!("Changed rich schedule ");
    expected
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let binding = fixture::install(state).await?;
    fixture::wait_runner(&binding).await?;
    let target = binding.target();
    let key = fixture::key(target);
    let which = label(target);
    let other = if which == "channel" {
        "thread"
    } else {
        "channel"
    };
    let rejected = matches!(
        binding.scenario.as_str(),
        "retry-channel" | "expired-change-thread"
    );
    let change = binding.scenario == "expired-change-thread";
    let server_reject = binding.scenario == "rejected-retry-channel";
    let stale = binding.scenario.starts_with("stale-");
    let newer = binding.scenario.starts_with("newer-")
        || binding.scenario.starts_with("lost-ack-")
        || binding.scenario.starts_with("held-response-");
    let held = binding.scenario.starts_with("held-response-");
    let lost = binding.scenario.starts_with("lost-ack-");
    let initial = fixture::durable(&binding).await?;
    let initial_messages = messages(&state.peek());
    let mut details = serde_json::Map::new();
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mounted = home(state, target).await?;
    ensure(
        native_valid(
            &mounted,
            initial[which]["canonical"]["composer"]["text"]
                .as_str()
                .unwrap_or(""),
        ),
        "Initial canonical/native text mismatch",
    )?;
    details.insert("nativeInitial".into(), mounted);
    {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        shell.core.scheduled.ensure_scope(epoch, "T1");
    }
    let initial_queue = queue(&state.peek());
    let mut operation = Value::Null;
    let mut consumption = Value::Null;
    let mut frozen = Value::Null;
    if binding.phase == "restart" {
        // A restart only reloads the actual paired store. It never issues a write.
        let view = binding
            .owner(&key)
            .load()
            .await
            .map_err(|e| format!("Restart load: {e:?}"))?;
        let stored = view
            .stored
            .ok_or("Managed successor unexpectedly released")?;
        if held || lost {
            ensure(
                matches!(stored.state.workflow(), Workflow::Uncertain(_)),
                "Restart lost uncertain Schedule",
            )?;
            operation = json!(stored.state.pending().ok_or("Uncertain operation absent")?);
            consumption = json!(
                stored
                    .state
                    .consumption()
                    .ok_or("Uncertain intent absent")?
            );
            ensure(
                crate::drafts::schedule::Request::create(&state.peek(), target).is_none(),
                "Uncertain Schedule admitted a second creation",
            )?;
            let blocked = dom(target, "schedule-control", json!({})).await?;
            ensure(
                blocked["control"]["disabled"] == true && blocked["clicked"] == false,
                "Restart replay control became active",
            )?;
            details.insert("blockedReplay".into(), blocked);
            ensure(
                same_native(
                    &details["nativeInitial"],
                    &native(target, "read", None, false).await?,
                ),
                "Blocked replay changed native text/selection",
            )?;
        } else if !stale {
            ensure(
                matches!(stored.state.workflow(), Workflow::Idle)
                    && stored.state.consumption().is_none(),
                "Confirmed Schedule did not retain clean managed successor",
            )?;
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
        if rejected {
            panel(state, false).await?;
        }
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
            "open".into(),
            dom(target, "open", json!({"rejected":rejected})).await?,
        );
        let review = wait_dom(target, "read").await?;
        details.insert("reviewInitial".into(), review.clone());
        if rejected {
            ensure(
                review["blocks"] == json!(fixture::rich_snapshot(target).blocks),
                "Review changed frozen rich blocks",
            )?;
        }
        if change {
            let blocked = dom(target, "click", json!({"label":"Retry same schedule"})).await?;
            ensure(
                blocked["control"]["disabled"] == true && blocked["clicked"] == false,
                "Expired Retry remained actionable",
            )?;
            details.insert("expiredRetry".into(), blocked);
            details.insert(
                "change".into(),
                dom(target, "click", json!({"label":"Change schedule"})).await?,
            );
            frames().await?;
            dom(target, "edit", json!({"change":true})).await?;
            dom(target, "time", json!({})).await?;
        } else if !rejected {
            dom(target, "edit", json!({"change":false})).await?;
            dom(target, "time", json!({})).await?;
        }
        frames().await?;
        frozen = wait_dom(target, "read").await?;
        if change {
            ensure(
                frozen["blocks"] == changed_blocks(target),
                "Change did not preserve exact opaque/mention/list metadata",
            )?;
        } else if rejected {
            ensure(
                frozen["blocks"] == review["blocks"] && frozen["postAt"] == review["postAt"],
                "Retry changed frozen content or time",
            )?;
        } else {
            ensure(
                contains_type(&frozen["blocks"], "rich_text_quote")
                    && contains_type(&frozen["blocks"], "rich_text_list"),
                "Native rich editor collapsed structure",
            )?;
        }
        capture(
            desktop,
            &binding,
            "review",
            target,
            &mut captures,
            &mut geometry,
        )
        .await?;
        let primary = if change {
            "Save new schedule"
        } else if rejected {
            "Retry same schedule"
        } else {
            "Schedule message"
        };
        if stale {
            let before = native(target, "read", None, false).await?;
            ensure(
                native_valid(&before, fixture::BASE),
                "Pre-retirement composer changed",
            )?;
            details.insert("staleNativeBefore".into(), before.clone());
            details.insert(
                "staleRetained".into(),
                dom(target, "retain-review", json!({"label":primary})).await?,
            );
            details.insert("contextBefore".into(), context(&state.peek()));
            if binding.scenario == "stale-owner-channel" {
                state.write().core.account_epoch += 1;
            } else {
                state.write().draft_writes.view_id += 1;
            }
            details.insert("contextAfter".into(), context(&state.peek()));
            // The keyed composer owns this creation dialog. Retirement proves
            // actual disconnection; a detached click would not prove that the
            // delegated Dioxus callback ran, so none is manufactured here.
            details.insert(
                "staleRetired".into(),
                dom(target, "retire-review", json!({})).await?,
            );
            let after = wait_native(target, fixture::BASE).await?;
            ensure(
                before["scope"] != after["scope"]
                    && before["visit"] != after["visit"]
                    && after["scope"]
                        == json!(crate::view::composer::field_scope(&state.peek(), target)),
                "Retired composer identity was reused",
            )?;
            details.insert("staleNativeAfter".into(), after);
            ensure(
                binding
                    .requests
                    .lock()
                    .map_err(|_| "Request ledger unavailable")?
                    .is_empty(),
                "Retired review prepared a request",
            )?;
            ensure(
                fixture::durable(&binding).await? == initial,
                "Stale review changed paired storage",
            )?;
            capture(
                desktop,
                &binding,
                "stale",
                target,
                &mut captures,
                &mut geometry,
            )
            .await?;
        } else {
            binding.arm("response")?;
            details.insert(
                "submit".into(),
                dom(target, "click", json!({"label":primary})).await?,
            );
            binding.wait_reached().await?;
            let view = binding
                .owner(&key)
                .load()
                .await
                .map_err(|e| format!("Pending load: {e:?}"))?;
            let stored = view.stored.ok_or("Pending state missing")?;
            ensure(
                matches!(stored.state.workflow(), Workflow::InFlight(_)),
                "Actual dispatch not durably InFlight",
            )?;
            let op = stored.state.pending().ok_or("Pending operation absent")?;
            let intent = stored
                .state
                .consumption()
                .ok_or("Pending consumption absent")?;
            let OperationKind::Schedule { post_at } = op.kind else {
                return Err("Non-Schedule operation reached fixture".into());
            };
            ensure(
                op.snapshot.as_ref() == Some(&intent.snapshot)
                    && json!(intent.snapshot.blocks) == frozen["blocks"]
                    && json!(post_at) == frozen["postAt"],
                "Actual dispatch differs from native reviewed payload/time",
            )?;
            ensure(
                op.known_schedule_check(),
                "Unknown-ID/unsupported Schedule reached fixture",
            )?;
            operation = json!(op);
            consumption = json!(intent);
            details.insert("pending".into(), fixture::durable(&binding).await?);
            if newer {
                // Synthetic input into the actual retained composer while HTTP is held;
                // this is not a claim of trusted typing through a modal scrim.
                let typed = type_text(&binding, target, fixture::NEWER).await?;
                details.insert("newerNative".into(), typed);
                details.insert("afterNewer".into(), fixture::durable(&binding).await?);
            }
            capture(
                desktop,
                &binding,
                "pending",
                target,
                &mut captures,
                &mut geometry,
            )
            .await?;
            if !held {
                binding.release();
                wait_dom(target, "closed").await?;
                if server_reject {
                    let retried = retry::after_first_rejection(
                        state,
                        desktop,
                        &binding,
                        &frozen,
                        &operation,
                        &consumption,
                        &initial_queue,
                        &mut details,
                        &mut captures,
                        &mut geometry,
                    )
                    .await?;
                    operation = retried.0;
                    consumption = retried.1;
                }
                let completed = binding
                    .owner(&key)
                    .load()
                    .await
                    .map_err(|e| format!("Complete load: {e:?}"))?
                    .stored
                    .ok_or("Schedule automatically released managed row")?;
                ensure(
                    if lost {
                        matches!(completed.state.workflow(), Workflow::Uncertain(_))
                            && completed.state.pending() == Some(op)
                    } else {
                        matches!(completed.state.workflow(), Workflow::Idle)
                            && completed.state.consumption().is_none()
                    },
                    "Completion workflow differs from actual decoder result",
                )?;
                if !rejected {
                    if server_reject {
                        details.insert("retryHomeMount".into(), home(state, target).await?);
                    }
                    let after = wait_native(
                        target,
                        if newer || server_reject {
                            fixture::NEWER
                        } else {
                            ""
                        },
                    )
                    .await?;
                    if newer {
                        ensure(
                            same_native(&details["newerNative"], &after),
                            "Completion rewrote newer native text or selection",
                        )?;
                    }
                    details.insert("nativeAfter".into(), after);
                }
                if lost {
                    ensure(
                        crate::drafts::schedule::Request::create(&state.peek(), target).is_none(),
                        "Uncertain operation admitted replay",
                    )?;
                    let blocked = dom(target, "schedule-control", json!({})).await?;
                    ensure(
                        blocked["control"]["disabled"] == true && blocked["clicked"] == false,
                        "Uncertain replay control became active",
                    )?;
                    details.insert("blockedReplay".into(), blocked);
                }
                panel(state, !lost).await?;
                details.insert(
                    "completionToast".into(),
                    dom(target, "dismiss-notice", json!({"expected":if lost {
                        "Schedule is unconfirmed. Check its status in Drafts before trying again."
                    } else {
                        "Message scheduled."
                    }})).await?,
                );
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
        }
    }
    let durable = fixture::durable(&binding).await?;
    ensure(
        durable[other] == initial[other],
        "Schedule altered unrelated destination",
    )?;
    ensure(
        binding.refused.load(Ordering::Relaxed) == 0,
        "Fixture input routing refused a change",
    )?;
    if binding.phase == "restart" || stale {
        ensure(durable == initial, "No-op phase changed durable inventory")?;
    }
    let actual_messages = messages(&state.peek());
    ensure(
        actual_messages == initial_messages,
        "Schedule projected a chat message/pending delivery",
    )?;
    let final_queue = queue(&state.peek());
    let attempts = binding.requests.lock().unwrap().clone();
    ensure(
        attempts.len()
            == if binding.phase == "exercise" && !stale {
                if server_reject { 2 } else { 1 }
            } else {
                0
            },
        "Schedule write was lost or replayed",
    )?;
    if binding.phase == "exercise" && !stale {
        let expected_text = if newer || rejected || server_reject {
            fixture::NEWER
        } else {
            ""
        };
        ensure(
            durable[which]["canonical"]["composer"]["text"]
                .as_str()
                .unwrap_or("")
                == expected_text,
            "Schedule discarded newer/separate local or retained consumed source",
        )?;
        if !held && !lost {
            let id = operation["base"]["id"]
                .as_str()
                .ok_or("Frozen draft ID absent")?;
            let raw = &attempts.last().unwrap()["rawResponse"]["draft"];
            ensure(
                final_queue.get(id).is_some_and(|row| {
                    row["blocks"] == frozen["blocks"]
                        && row["date_scheduled"] == frozen["postAt"]
                        && row["client_msg_id"] == operation["client_msg_id"]
                        && row["last_updated_ts"] == fixture::ACK_TS
                        && row["destinations"] == raw["destinations"]
                }),
                "Exact scheduled row not projected",
            )?;
        } else {
            ensure(
                final_queue == initial_queue,
                "Uncertain write projected Scheduled",
            )?;
        }
    }
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"nonce":binding.nonce,"pid":std::process::id(),"scenario":binding.scenario,"phase":binding.phase,"isolatedDatabase":binding.path,"target":which,
        "noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noCache":state.peek().core.cache.is_none(),"noProductionRoute":binding.isolated(),
        "syntheticInput":true,"actualDialogControl":true,"actualPreparedRequests":true,"preparedRequests":attempts.len(),"attempts":attempts,"routedInputs":binding.routed.load(Ordering::Relaxed),"refusedInputs":binding.refused.load(Ordering::Relaxed),
        "operation":operation,"consumption":consumption,"frozen":frozen,"initial":initial,"durable":durable,"messagesInitial":initial_messages,"messagesFinal":actual_messages,"queueInitial":initial_queue,"queueFinal":final_queue,"captures":captures,"viewport":geometry[0]["viewport"],"geometry":geometry,"details":details,
        "limitations":["Synthetic DOM input/clicks and isolated HTTP; no trusted input, IME, or live Slack proof.","Newer input is injected through the real mounted composer while the response is held, not trusted typing through the modal.","Restart reloads managed durability only; scheduled inventory refetch belongs to the independent recovery fixture.","No automatic managed-row release or chat projection."]}),
    )
}
async fn wait_native(target: ComposerTarget, text: &str) -> Result<Value, String> {
    for _ in 0..150 {
        if let Ok(value) = native(target, "read", None, false).await
            && native_valid(&value, text)
        {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Consumed/current native input did not settle".into())
}
