//! Isolated native Schedule recovery: actual row callbacks and preserved input.
use super::{operation, schedule_fixture as fixture};
use crate::drafts::{checkpoint, storage::Backend};
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use super_platinum_core::drafts::sync::{Operation, OperationKind};
#[path = "schedule_check_dom.rs"]
mod dom;
use dom::*;

pub(crate) fn start(s: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(150), run(s, &desktop)).await;
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error}),
            Err(_) => json!({"ok":false,"error":"Schedule recovery check timeout"}),
        };
        eprintln!("draft-schedule-recovery-check: {receipt}");
    });
}

fn inventory(s: &ShellState, b: &fixture::FixtureBinding) -> Value {
    fixture::projection(s, b)
}
fn messages(s: &ShellState) -> Value {
    let channel = s.core.workspaces["T1"].messages.get("C2");
    let thread = s
        .core
        .threads
        .get(&("T1".into(), "C2".into(), fixture::ROOT.into()));
    let mut cp: Vec<_> = channel
        .into_iter()
        .flat_map(|b| b.pending.iter().cloned())
        .collect();
    cp.sort();
    let mut tp: Vec<_> = thread
        .into_iter()
        .flat_map(|b| b.pending.iter().cloned())
        .collect();
    tp.sort();
    json!({"channel":channel.map(|b|&b.messages),"thread":thread.map(|b|&b.messages),
        "pending":{"channel":cp,"thread":tp},"pending_delivery":s.pending_delivery.iter().map(|(k,v)|(k,format!("{v:?}"))).collect::<std::collections::BTreeMap<_,_>>()})
}
fn race(name: &str) -> bool {
    matches!(
        name,
        "owner-rotation-channel"
            | "writer-replacement-channel"
            | "queue-reset-channel"
            | "queue-newer-channel"
            | "successor-channel"
            | "reenroll-channel"
            | "unmount-thread"
    )
}
fn positive(name: &str) -> bool {
    !matches!(
        name,
        "absent-channel" | "changed-payload-channel" | "cooldown-channel" | "pending-check-channel"
    )
}
async fn settled_row(op: &Operation, needle: &str) -> Result<Value, String> {
    for _ in 0..200 {
        if let Ok(v) = row(op, false).await
            && v["disabled"] == false
            && v["text"].as_str().is_some_and(|s| s.contains(needle))
        {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Schedule row did not settle with {needle}"))
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(s).await?;
    fixture::wait_runner(&b).await?;
    let target = b.target();
    let which = label(target);
    let other = if which == "channel" {
        "thread"
    } else {
        "channel"
    };
    let initial = fixture::durable(&b).await?;
    let initial_messages = messages(&s.peek());
    let initial_projection = inventory(&s.peek(), &b);
    let view = b
        .owner(&fixture::key(target))
        .load()
        .await
        .map_err(|e| format!("Load schedule: {e:?}"))?;
    let op = operation(&view).ok_or("Missing uncertain Schedule")?;
    ensure(
        op.kind
            == OperationKind::Schedule {
                post_at: fixture::POST_AT,
            }
            && op.known_schedule_check(),
        "Wrong frozen Schedule",
    )?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    let mut details = serde_json::Map::new();
    let native_initial = home(s, target).await?;
    ensure(
        native_valid(&native_initial, fixture::CURRENT),
        "Initial composer not retained local text",
    )?;
    details.insert("nativeInitial".into(), native_initial);
    if matches!(b.scenario.as_str(), "positive-channel" | "positive-thread") {
        let mut fields = serde_json::Map::new();
        for destination in [ComposerTarget::Channel, ComposerTarget::Thread] {
            let value = home(s, destination).await?;
            ensure(
                native_valid(&value, fixture::CURRENT),
                "Initial secondary composer text differs",
            )?;
            fields.insert(label(destination).into(), value);
        }
        details.insert("nativeByScopeInitial".into(), fields.into());
    }
    drafts(s, &op).await?;
    if b.phase == "restart" {
        ensure(
            matches!(
                b.scenario.as_str(),
                "pending-check-channel" | "cooldown-channel"
            ),
            "Unexpected restart case",
        )?;
        let clicked = row(&op, true).await?;
        ensure(
            clicked["clicked"] == true,
            "Restart Check schedule was not clicked",
        )?;
        let refusal = settled_row(
            &op,
            if b.scenario == "cooldown-channel" {
                "cooldown"
            } else {
                "unresolved"
            },
        )
        .await?;
        details.insert("restartClick".into(), clicked);
        details.insert("restartRefusal".into(), refusal.clone());
        capture(d, &b, "restart", refusal, &mut captures, &mut geometry).await?;
        ensure(
            b.requests.lock().unwrap().is_empty(),
            "Restart repeated a read",
        )?;
    } else {
        capture(
            d,
            &b,
            "initial",
            row(&op, false).await?,
            &mut captures,
            &mut geometry,
        )
        .await?;
        for (index, withheld) in b.withheld.iter().enumerate() {
            let text = withheld
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.blocks.first())
                .and_then(|block| block.pointer("/elements/0/elements/0/text"))
                .and_then(Value::as_str)
                .ok_or("Withheld rich title missing")?;
            let v = surface("withheld", text, false).await?;
            ensure(v["checkCount"] == 0, "Unsupported Schedule exposed Check")?;
            capture(
                d,
                &b,
                &format!("withheld-{index}"),
                v,
                &mut captures,
                &mut geometry,
            )
            .await?;
        }
        drafts(s, &op).await?;
        let gate = if b.scenario == "typing-commit-thread" || b.scenario == "storage-retry-channel"
        {
            "commit"
        } else if matches!(
            b.scenario.as_str(),
            "successor-channel" | "reenroll-channel"
        ) {
            "project"
        } else {
            "read"
        };
        b.arm(gate)?;
        let clicked = row(&op, true).await?;
        ensure(
            clicked["clicked"] == true,
            "Actual Check schedule did not click",
        )?;
        b.wait_reached().await?;
        details.insert("actualClick".into(), clicked);
        let held = fixture::durable(&b).await?;
        if gate != "project" {
            ensure(
                held[which]["stored"]["state"]["check_ticket"]["operation"] == json!(op),
                "Read lacked exact durable ticket",
            )?;
        }
        ensure(
            b.requests.lock().unwrap().len() == 1,
            "Expected one actual prepared read",
        )?;
        details.insert("held".into(), held);
        details.insert("heldProjection".into(), inventory(&s.peek(), &b));
        capture(
            d,
            &b,
            "checking",
            row(&op, false).await?,
            &mut captures,
            &mut geometry,
        )
        .await?;
        let mut native_before = None;
        if b.scenario.starts_with("typing-") {
            home(s, target).await?;
            let v = type_newer(s, &b, target).await?;
            ensure(
                native_valid(&v, fixture::NEWER),
                "Typed newer text not routed",
            )?;
            let v = native(target, "selection", None, false).await?;
            details.insert("input".into(), v.clone());
            details.insert("inputProjection".into(), inventory(&s.peek(), &b));
            capture(
                d,
                &b,
                "input",
                composer(target).await?,
                &mut captures,
                &mut geometry,
            )
            .await?;
            native_before = Some(v);
        } else if race(&b.scenario) {
            home(s, target).await?;
            details.insert("beforeInterleave".into(), inventory(&s.peek(), &b));
            details.insert("mutation".into(), fixture::interleave(s, &b, &op).await?);
            details.insert("afterInterleave".into(), inventory(&s.peek(), &b));
            native_before = Some(native(target, "selection", None, false).await?);
            capture(
                d,
                &b,
                "interleaved",
                composer(target).await?,
                &mut captures,
                &mut geometry,
            )
            .await?;
        }
        if b.scenario == "storage-retry-channel" {
            fixture::storage_fault(&b, true).await?;
        }
        if b.scenario != "pending-check-channel" {
            b.release();
            b.wait_ui().await?;
            frames().await?;
            if b.scenario == "storage-retry-channel" {
                let failed = settled_row(&op, "could not be saved").await?;
                details.insert("failedStorageProjection".into(), inventory(&s.peek(), &b));
                capture(d, &b, "retry", failed, &mut captures, &mut geometry).await?;
                fixture::restore_storage(&b).await?;
                details.insert("failedStorage".into(), fixture::durable(&b).await?);
                b.arm("project")?;
                details.insert("retryClick".into(), row(&op, true).await?);
                b.wait_reached().await?;
                b.release();
                b.wait_ui().await?;
                frames().await?;
                ensure(
                    b.requests.lock().unwrap().len() == 1,
                    "Storage retry repeated HTTP",
                )?;
            }
        }
        details.insert("afterReceipt".into(), fixture::durable(&b).await?);
        details.insert("resultProjection".into(), inventory(&s.peek(), &b));
        let suppressed = matches!(
            b.scenario.as_str(),
            "owner-rotation-channel"
                | "writer-replacement-channel"
                | "queue-reset-channel"
                | "queue-newer-channel"
                | "successor-channel"
                | "reenroll-channel"
        );
        let mut expected_items = if suppressed {
            details["afterInterleave"]["queue"]["items"].clone()
        } else {
            initial_projection["queue"]["items"].clone()
        };
        if positive(&b.scenario) && !suppressed {
            let raw = fixture::positive(&op)["draft"].clone();
            let expected: Value = [
                "id",
                "date_scheduled",
                "date_created",
                "client_msg_id",
                "blocks",
                "file_ids",
                "destinations",
                "is_deleted",
                "is_sent",
                "last_updated_ts",
            ]
            .into_iter()
            .map(|k| (k.to_owned(), raw[k].clone()))
            .collect::<serde_json::Map<_, _>>()
            .into();
            expected_items[op.base.as_ref().ok_or("Missing known ID")?.id.as_str()] = expected;
        }
        ensure(
            details["resultProjection"]["queue"]["items"] == expected_items,
            "Queue differs from exact frozen Schedule / guarded race result",
        )?;
        if suppressed
            && !matches!(
                b.scenario.as_str(),
                "queue-reset-channel" | "queue-newer-channel"
            )
        {
            ensure(
                details["resultProjection"] == details["afterInterleave"],
                "Stale completion changed current shell projection",
            )?;
        }
        let result = if let Some(before) = native_before {
            let after = native(target, "read", None, false).await?;
            for field in [
                "text",
                "display",
                "codePoints",
                "utf16Length",
                "scope",
                "visit",
                "start",
                "end",
                "direction",
            ] {
                ensure(
                    before[field] == after[field],
                    "Late receipt replaced native input/selection",
                )?;
            }
            details.insert("nativeBeforeRelease".into(), before);
            details.insert("nativeFinal".into(), after);
            composer(target).await?
        } else if positive(&b.scenario) {
            let v = surface("local", fixture::CURRENT, false).await?;
            ensure(
                v["actions"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "Continue editing locally")),
                "Confirmed schedule lost explicit local return action",
            )?;
            v
        } else if b.scenario == "pending-check-channel" {
            row(&op, false).await?
        } else {
            let v = settled_row(
                &op,
                if b.scenario == "cooldown-channel" {
                    "cooldown"
                } else {
                    "still unconfirmed"
                },
            )
            .await?;
            details.insert("feedback".into(), v);
            if b.scenario == "cooldown-channel" {
                details.insert("cooldownClick".into(), row(&op, true).await?);
                settled_row(&op, "cooldown").await?
            } else {
                row(&op, false).await?
            }
        };
        capture(d, &b, "result", result, &mut captures, &mut geometry).await?;
        if matches!(b.scenario.as_str(), "positive-channel" | "positive-thread") {
            let mut fields = serde_json::Map::new();
            for destination in [ComposerTarget::Channel, ComposerTarget::Thread] {
                let value = home(s, destination).await?;
                ensure(
                    native_valid(&value, fixture::CURRENT),
                    "Positive receipt changed native composer text",
                )?;
                fields.insert(label(destination).into(), value);
            }
            details.insert("nativeByScopeFinal".into(), fields.into());
        }
        ensure(
            s.peek().core.scheduled.filter == super_platinum_core::scheduled::DraftFilter::Drafts,
            "Recovery automatically switched filter",
        )?;
        details.insert("beforeScheduledClick".into(), inventory(&s.peek(), &b));
        {
            let mut shell = s.write();
            shell.close_thread();
            shell.main_view = MainView::Scheduled;
        }
        frames().await?;
        details.insert("scheduledClick".into(), surface("tab", "", true).await?);
        frames().await?;
        ensure(
            s.peek().core.scheduled.filter
                == super_platinum_core::scheduled::DraftFilter::Scheduled,
            "Actual Scheduled tab click not handled",
        )?;
        capture(
            d,
            &b,
            "scheduled",
            surface("scheduled", "", false).await?,
            &mut captures,
            &mut geometry,
        )
        .await?;
    }
    let durable = fixture::durable(&b).await?;
    ensure(
        durable[other] == initial[other] && durable["withheld"] == initial["withheld"],
        "Unrelated durable destination changed",
    )?;
    if b.phase == "restart" {
        ensure(durable == initial, "Restart mutated durable receipt")?;
    }
    let expected = if b.scenario.starts_with("typing-") {
        fixture::NEWER
    } else {
        fixture::CURRENT
    };
    if !matches!(
        b.scenario.as_str(),
        "successor-channel" | "reenroll-channel"
    ) {
        ensure(
            durable[which]["canonical"]["composer"]["text"] == expected,
            "Current local text changed",
        )?;
    }
    ensure(
        durable[which]["canonical"]["composer"]["reply_broadcast"]
            == json!(target == ComposerTarget::Thread),
        "Schedule recovery changed broadcast preference",
    )?;
    let shell = s.peek();
    ensure(
        shell.core.session.is_none()
            && shell.core.transport.is_none()
            && shell.core.cache.is_none(),
        "Fixture isolation lost",
    )?;
    ensure(
        messages(&shell) == initial_messages,
        "Schedule read changed chat messages or pending delivery",
    )?;
    let attempts = b.requests.lock().unwrap().clone();
    ensure(
        attempts.len() == usize::from(b.phase == "exercise"),
        "Unexpected request count",
    )?;
    ensure(
        b.refused.load(Ordering::Relaxed) == 0,
        "Fixture refused native input",
    )?;
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").unwrap(),"nonce":b.nonce,"pid":std::process::id(),"scenario":b.scenario,"phase":b.phase,"isolatedDatabase":b.path,"target":which,"noSession":true,"noTransport":true,"noCache":true,"noProductionRoute":matches!(&b.account.backend,Backend::Isolated{..}),"syntheticInput":true,"actualPreparedRequests":true,"canonicalMessagesUnchanged":true,"preparedRequests":attempts.len(),"attempts":attempts,"routedInputs":b.routed.load(Ordering::Relaxed),"refusedInputs":b.refused.load(Ordering::Relaxed),"operation":op,"withheld":b.withheld,"initial":initial,"durable":durable,"messagesInitial":initial_messages,"messagesFinal":messages(&shell),"initialProjection":initial_projection,"finalProjection":inventory(&shell,&b),"captures":captures,"viewport":geometry[0]["viewport"],"geometry":geometry,"details":details,"limitations":["Synthetic isolated read responses; no live Slack, network, autosave, or realtime ingress.","Native captures and exact receipts establish only the named deterministic cases.","Pending-check exits with an unresolved durable ticket and held response."]}),
    )
}
