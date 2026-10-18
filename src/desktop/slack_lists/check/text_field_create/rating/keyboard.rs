use super::*;
async fn within_keyboard_deadline(
    deadline: tokio::time::Instant,
    future: impl std::future::Future<Output = Result<Value, String>>,
) -> Result<Value, String> {
    let result = tokio::time::timeout_at(deadline, future)
        .await
        .map_err(|_| "Trusted Rating key sequence timed out".to_owned())?;
    // Timeout polls the inner future first. A ready result after a blocked poll
    // must still fail the same absolute deadline.
    if tokio::time::Instant::now() >= deadline {
        return Err("Trusted Rating key sequence timed out".into());
    }
    result
}
fn cleanup_result(
    primary: Result<Value, String>,
    cleanup: Result<Value, String>,
) -> Result<Value, String> {
    match (primary, cleanup) {
        (Err(primary), Err(cleanup)) => Err(format!("{primary}; keyboard cleanup: {cleanup}")),
        (Err(primary), _) => Err(primary),
        (Ok(_), Err(cleanup)) => Err(format!("Keyboard cleanup failed: {cleanup}")),
        (Ok(value), Ok(_)) => Ok(value),
    }
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    mode: FieldCreateMode,
    generation: u64,
) -> Result<Value, String> {
    let before = model(state)?;
    let started = tokio::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(15);
    if !mode.is_rating() || rename_fixture::field_create_mode() != Some(mode) {
        return Err("Rating keyboard mode retired".into());
    }
    let (case, actions): (&str, &[&str]) = match mode {
        FieldCreateMode::RatingDefault => ("default", &["end", "right", "up", "2", "left", "down"]),
        FieldCreateMode::RatingStandard => (
            "standard",
            &[
                "right",
                "up",
                "left",
                "down",
                "end",
                "right",
                "home",
                "1",
                "2",
                "3",
                "4",
                "5",
                "0",
                "end",
                "delete",
                "end",
                "backspace",
            ],
        ),
        FieldCreateMode::RatingCustom => ("custom", &["3", "4", "5", "right", "home", "end"]),
        _ => return Err("Wrong Rating keyboard mode".into()),
    };
    checked(d, include_str!("keyboard.js")).await?;
    let nonce =
        std::env::var(format!("{}_NONCE", mode.env_prefix())).map_err(|_| "Missing owned nonce")?;
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let identity = json!({"version":1,"fixture":fixture,"nonce":nonce,"pid":std::process::id(),"owner":before["owner"]});
    let run = within_keyboard_deadline(deadline, async {
        let begin = checked(
            d,
            &format!(
                "window.__listRatingKeyboard.begin({},{generation})",
                json!(case)
            ),
        )
        .await?;
        for (ordinal, action) in actions.iter().enumerate() {
            let arm = checked(d, &format!("window.__listRatingKeyboard.arm({ordinal})")).await?;
            let mut request = identity.clone();
            request["ordinal"] = json!(ordinal);
            request["action"] = json!(format!("rating-{action}"));
            request["expected"] = arm["expected"].clone();
            tokio::task::spawn_blocking(move || {
                use std::io::Write;
                std::io::stderr()
                    .lock()
                    .write_all(format!("{}-keyboard: {request}\n", mode.prefix()).as_bytes())
            })
            .await
            .map_err(|_| "Keyboard receipt writer panicked")?
            .map_err(|_| "Keyboard receipt write failed")?;
            loop {
                let status = checked(d, "window.__listRatingKeyboard.status()").await?;
                let count = status["count"]
                    .as_u64()
                    .ok_or("Missing native Rating key count")?;
                if count > ordinal as u64 + 1 {
                    return Err("Extra native Rating key event".into());
                }
                if count == ordinal as u64 + 1 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if model(state)? != before {
                return Err("Keyboard traversal changed owned Rating worker state".into());
            }
        }
        let result = checked(d, "window.__listRatingKeyboard.end()").await?;
        Ok(json!({"begin":begin,"result":result}))
    })
    .await;
    let sequence_ms = started.elapsed().as_secs_f64() * 1000.0;
    // Cleanup is a separate bounded phase and never replaces the primary failure.
    let cleanup_started = tokio::time::Instant::now();
    let cleanup_deadline = cleanup_started + std::time::Duration::from_secs(2);
    let cleanup = tokio::time::timeout_at(
        cleanup_deadline,
        checked(d, "window.__listRatingKeyboard.cleanup()"),
    )
    .await
    .map_err(|_| "Keyboard cleanup timed out".to_owned())
    .and_then(|r| r);
    let events = cleanup_result(run, cleanup)?;
    if model(state)? != before {
        return Err("Keyboard cleanup changed owned Rating worker state".into());
    }
    let mut completion = identity;
    completion["events"] = events["result"]["events"].clone();
    completion["cleaned"] = json!(true);
    completion["sequenceElapsedMs"] = json!(sequence_ms);
    completion["cleanupElapsedMs"] = json!(cleanup_started.elapsed().as_secs_f64() * 1000.0);
    let receipt = completion.clone();
    // The immediate completion is observation only, never another input/ACK request.
    tokio::time::timeout_at(
        cleanup_deadline,
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            std::io::stderr()
                .lock()
                .write_all(format!("{}-keyboard-complete: {receipt}\n", mode.prefix()).as_bytes())
        }),
    )
    .await
    .map_err(|_| "Keyboard completion output timed out")?
    .map_err(|_| "Keyboard completion writer panicked")?
    .map_err(|_| "Keyboard completion write failed")?;
    if tokio::time::Instant::now() >= cleanup_deadline {
        return Err("Keyboard cleanup/completion deadline".into());
    }
    Ok(
        json!({"begin":events["begin"],"result":events["result"],"completion":completion,"cleaned":true,"sameWorkerState":true,"inputTargetingAtomic":false}),
    )
}
#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[tokio::test]
    async fn actual_keyboard_deadline_bounds_slow_await_and_late_ready_result() {
        struct DropProbe(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let polled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reached.clone();
        let polls = polled.clone();
        let drop_probe = DropProbe(dropped.clone());
        // Keep the sender held: scheduling cannot make this input ready when
        // both a short deadline and a longer timer have elapsed before a poll.
        let (held, mut receiver) = tokio::sync::oneshot::channel::<()>();
        let result = within_keyboard_deadline(
            tokio::time::Instant::now() + std::time::Duration::from_millis(2),
            async move {
                let _drop_probe = drop_probe;
                std::future::poll_fn(move |cx| {
                    polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::future::Future::poll(std::pin::Pin::new(&mut receiver), cx)
                })
                .await
                .expect("the held sender must not finish before timeout");
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(json!({"ok":true}))
            },
        )
        .await;
        assert!(result.is_err());
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(polled.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            held.send(()).is_err(),
            "timeout must drop its pending input"
        );
        let result = within_keyboard_deadline(
            tokio::time::Instant::now() + std::time::Duration::from_millis(2),
            async {
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok(json!({"ok":true}))
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            within_keyboard_deadline(
                tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                async { Ok(json!({"ok":true})) }
            )
            .await
            .unwrap(),
            json!({"ok":true})
        );
    }
    #[test]
    fn cleanup_failure_never_masks_primary_or_allows_success() {
        assert_eq!(
            cleanup_result(Err("primary".into()), Err("cleanup".into())).unwrap_err(),
            "primary; keyboard cleanup: cleanup"
        );
        assert_eq!(
            cleanup_result(Err("primary".into()), Ok(json!({"ok":true}))).unwrap_err(),
            "primary"
        );
        assert!(cleanup_result(Ok(json!({"ok":true})), Err("cleanup".into())).is_err());
    }
}

fn model(state: Signal<ShellState>) -> Result<Value, String> {
    let shell = state.peek();
    let e = super::editor(&shell)?;
    if e.is_null() || e["disabled"] != false || e["original"] != Value::Null {
        return Err("Rating keyboard draft unavailable".into());
    }
    let ledger = cells::snapshot(&shell)?;
    Ok(
        json!({"owner":{"visit":ledger["sync"]["visit"],"selection":ledger["selection"],"generation":e["generation"],"row":cells::ROW,"column":cells::COLUMN},"editor":e,"ledger":ledger}),
    )
}
