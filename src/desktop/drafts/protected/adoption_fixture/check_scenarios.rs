//! Seven bounded contracts; controls invoke the real shared lifecycle.
use super::*;
pub(super) async fn run(e: &mut Evidence<'_>) -> Result<(), String> {
    match e.b.scenario.as_str() {
        "adopt-channel" | "adopt-thread" => success(e, false).await,
        "failed-managed-write-channel" => success(e, true).await,
        "late-input-channel" => late(e).await,
        "native-refusals-channel" => refusals(e).await,
        "owner-replacement-channel" => replacement(e).await,
        "cancel-boundaries-channel" => canceled(e).await,
        _ => Err("Unknown adoption scenario".into()),
    }
}
async fn success(e: &mut Evidence<'_>, failed: bool) -> Result<(), String> {
    let before = e.attempt("prepared", "prepared").await?;
    e.b.release_gate("prepared");
    ensure(
        e.result("installResult").await?["kind"] == "installed",
        "Actual installation refused",
    )?;
    let after = e.capture("installed").await?;
    ensure(
        same_native(&before["native"]["native"], &after["native"]["native"]),
        "Installation changed native text/selection/owner",
    )?;
    e.details
        .insert("oldLaneRefusal".into(), fixture::old_lane_refuses(e.b)?);
    if !failed {
        e.details.insert(
            "typed".into(),
            type_text(e.state, e.b, fixture::NEWER, None).await?,
        );
        e.capture("newer").await?;
        return Ok(());
    }
    fixture::arm_failure(e.b).await?;
    e.details.insert(
        "typed".into(),
        type_text(e.state, e.b, fixture::NEWER, Some("Storage")).await?,
    );
    // An explicit Check status updates the typed fixture panel after background IO.
    e.button("failedCheck", "check", "Check status").await?;
    e.result("failedStatus").await?;
    let failed = e.capture("failed").await?;
    fixture::replace_shell(e.state, e.b, "away")?;
    e.details.insert(
        "awayReady".into(),
        ready(e.state, e.b.target(), &failed["native"]["native"]["visit"]).await?,
    );
    let away = e.capture("away").await?;
    fixture::replace_shell(e.state, e.b, "return")?;
    e.details.insert(
        "returnReady".into(),
        ready(e.state, e.b.target(), &away["native"]["native"]["visit"]).await?,
    );
    e.capture("returned").await?;
    e.button("retry", "retry", "Retry saving").await?;
    ensure(
        e.result("retryResult").await?["kind"] == "input-retried",
        "Exact managed retry failed",
    )?;
    e.capture("retried").await?;
    e.button("resume", "resume", "Resume editing").await?;
    ensure(
        e.result("resumeResult").await?["kind"] == "rebound",
        "Explicit retained route rebind failed",
    )?;
    e.capture("resumed").await?;
    e.details.insert(
        "typedAfterRetry".into(),
        type_text(e.state, e.b, FINAL, None).await?,
    );
    e.capture("newer").await?;
    Ok(())
}
async fn late(e: &mut Evidence<'_>) -> Result<(), String> {
    e.attempt("registering", "registering").await?;
    e.details.insert(
        "typed".into(),
        type_text(e.state, e.b, fixture::NEWER, Some("ReviewRequired")).await?,
    );
    e.capture("interleaved").await?;
    e.b.release_gate("registering");
    ensure(
        e.result("refusal").await?["kind"] == "install-refused",
        "Changed original registration unexpectedly installed",
    )?;
    e.capture("refused").await?;
    Ok(())
}
async fn refusals(e: &mut Evidence<'_>) -> Result<(), String> {
    e.attempt("selection-prepared", "prepared").await?;
    e.details.insert(
        "selectionChange".into(),
        native(e.state, e.b.target(), "forward", None).await?,
    );
    e.capture("selection-changed").await?;
    e.b.release_gate("prepared");
    ensure(
        e.result("selectionResult").await?["kind"] == "install-refused",
        "Selection change installed",
    )?;
    e.capture("selection-refused").await?;
    native(e.state, e.b.target(), "selection", None).await?;
    e.attempt("composition-prepared", "prepared").await?;
    e.details.insert(
        "compositionStart".into(),
        native(e.state, e.b.target(), "composition-start", None).await?,
    );
    e.capture("composition-active").await?;
    e.b.release_gate("prepared");
    ensure(
        e.result("compositionResult").await?["kind"] == "install-refused",
        "Active composition installed",
    )?;
    e.details.insert(
        "compositionEnd".into(),
        native(e.state, e.b.target(), "composition-end", None).await?,
    );
    e.capture("composition-refused").await?;
    e.attempt("expiry-locked", "locked").await?;
    let began = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(6200)).await;
    e.details
        .insert("expiredWritable".into(), writable(e.state, e.b).await?);
    e.details
        .insert("expiryElapsedMs".into(), json!(began.elapsed().as_millis()));
    e.capture("expiry-restored").await?;
    e.b.release_gate("locked");
    ensure(
        e.result("expiryResult").await?["kind"] == "install-refused",
        "Expired lease installed",
    )?;
    e.capture("expiry-refused").await?;
    e.attempt("aba-prepared", "prepared").await?;
    e.details.insert(
        "abaChangedInput".into(),
        type_text(
            e.state,
            e.b,
            &format!("{} temporary", fixture::INITIAL),
            Some("ReviewRequired"),
        )
        .await?,
    );
    e.details.insert(
        "sameTextInput".into(),
        type_text(e.state, e.b, fixture::INITIAL, Some("ReviewRequired")).await?,
    );
    e.capture("aba-input").await?;
    e.b.release_gate("prepared");
    ensure(
        e.result("abaResult").await?["kind"] == "install-refused",
        "Same-text input counter change installed",
    )?;
    e.capture("aba-refused").await?;
    Ok(())
}
async fn replacement(e: &mut Evidence<'_>) -> Result<(), String> {
    let before = e.attempt("prepared", "prepared").await?;
    fixture::replace_shell(e.state, e.b, "same-epoch")?;
    e.details.insert(
        "replacementReady".into(),
        ready(e.state, e.b.target(), &before["native"]["native"]["visit"]).await?,
    );
    e.capture("replaced").await?;
    e.b.release_gate("prepared");
    ensure(
        e.result("refusal").await?["kind"] == "install-refused",
        "Old owner installation succeeded",
    )?;
    e.capture("refused").await?;
    e.button("check", "check", "Check status").await?;
    let result = e.result("recoveredResult").await?;
    ensure(
        result["kind"] == "registered" && result["current"] == false,
        "Old registration status became current",
    )?;
    e.capture("recovered").await?;
    Ok(())
}
async fn canceled(e: &mut Evidence<'_>) -> Result<(), String> {
    e.attempt("before-prepared", "prepared").await?;
    e.button("cancelBefore", "cancel", "Cancel waiting").await?;
    ensure(
        e.result("canceledBefore").await?["kind"] == "canceled",
        "Before-commit cancel missed handler",
    )?;
    e.capture("canceled-before").await?;
    e.button("checkBefore", "check", "Check status").await?;
    ensure(
        e.result("registeredResult").await?["kind"] == "registered",
        "Canceled registration not recoverable",
    )?;
    e.capture("registered").await?;
    e.attempt("after-installed", "installed").await?;
    e.button("cancelAfter", "cancel", "Cancel waiting").await?;
    ensure(
        e.result("canceledAfter").await?["kind"] == "canceled",
        "After-commit cancel missed handler",
    )?;
    e.capture("canceled-after").await?;
    e.button("checkAfter", "check", "Check status").await?;
    ensure(
        e.result("recoveredResult").await?["kind"] == "installed",
        "Installed receipt lost on cancellation",
    )?;
    e.capture("recovered").await?;
    e.details
        .insert("oldLaneRefusal".into(), fixture::old_lane_refuses(e.b)?);
    e.details.insert(
        "typed".into(),
        type_text(e.state, e.b, fixture::NEWER, None).await?,
    );
    e.capture("newer").await?;
    Ok(())
}
