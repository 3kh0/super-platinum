use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let started = Instant::now();
    let first = toggle(s, d, "pick", BETA, &[ALPHA, BETA]).await?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    let second = toggle(s, d, "pick", ALPHA, &[BETA]).await?;
    let third = toggle(s, d, "pick", GAMMA, &[BETA, GAMMA]).await?;
    let invalid = toggle(s, d, "pick", ALPHA, &[BETA, GAMMA, ALPHA]).await?;
    let sibling = notes_received(s, "Latest sibling").await?;
    let elapsed = started.elapsed().as_millis();
    let before = snapshot(&s.peek());
    if elapsed >= 450 || before["requests"] != json!([]) {
        return Err("Multi-channel burst was not trailing-only within450ms".into());
    }
    counts(s, 1, 1, 0, 0).await?;
    shot(s,d,n,"trailing-keeps-overmax",json!({"first":first,"second":second,"third":third,"invalid":invalid,"sibling":sibling,"elapsedMs":elapsed,"beforeDelivery":before,"interCallbackDelayMs":120}),st,cap).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "overmax-no-action",
        json!({"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let corrected = remove(s, d, "pick", GAMMA, &[BETA, ALPHA]).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "corrected-order", corrected, st, cap).await?;
    let started = Instant::now();
    let valid = toggle(s, d, "required", ALPHA, &[ALPHA]).await?;
    let empty = remove(s, d, "required", ALPHA, &[]).await?;
    let sibling = notes_received(s, "Required sibling").await?;
    let elapsed = started.elapsed().as_millis();
    let before = snapshot(&s.peek());
    if elapsed >= 450 || before["requests"].as_array().map(Vec::len) != Some(2) {
        return Err("Required multi-channel invalid callback missed queued window".into());
    }
    counts(s, 3, 3, 0, 0).await?;
    shot(s,d,n,"required-empty-keeps-queued",json!({"valid":valid,"empty":empty,"sibling":sibling,"elapsedMs":elapsed,"beforeDelivery":before}),st,cap).await?;
    let quiet = toggle(s, d, "quiet", BETA, &[ALPHA, BETA]).await?;
    let single = choose(s, d, "single", GAMMA).await?;
    let pressed = click(s, d).await?;
    counts(s, 3, 3, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Multi-channel Button reply not held".into());
    }
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "quiet-single-button",
        json!({"quiet":quiet,"single":single,"click":pressed}),
        st,
        cap,
    )
    .await?;
    let removed = remove(s, d, "pick", ALPHA, &[BETA]).await?;
    counts(s, 4, 4, 1, 1).await?;
    let query = search(s, d, "pick", "release", 1).await?;
    shot(
        s,
        d,
        n,
        "remote-results",
        json!({"remove":removed,"query":query}),
        st,
        cap,
    )
    .await?;
    let remote = toggle(s, d, "pick", REMOTE, &[BETA, REMOTE]).await?;
    counts(s, 5, 5, 1, 1).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.channels.contains_key(REMOTE))
    {
        return Err("Multi-channel remote result polluted workspace cache".into());
    }
    shot(
        s,
        d,
        n,
        "remote-selected",
        json!({"choice":remote,"remoteNotInWorkspace":true}),
        st,
        cap,
    )
    .await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":reset,"silence":quiet,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
