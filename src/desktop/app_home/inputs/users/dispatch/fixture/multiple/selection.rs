use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let started = Instant::now();
    let first = toggle(s, d, "pick", BLAIR, &[ADA, BLAIR]).await?;
    tokio::time::sleep(Duration::from_millis(120)).await;
    let second = toggle(s, d, "pick", ADA, &[BLAIR]).await?;
    let third = toggle(s, d, "pick", CASEY, &[BLAIR, CASEY]).await?;
    let invalid = toggle(s, d, "pick", ADA, &[BLAIR, CASEY, ADA]).await?;
    let sibling = notes_received(s, "Latest sibling").await?;
    let elapsed = started.elapsed().as_millis();
    let before = snapshot(&s.peek());
    if elapsed >= 450 || before["requests"] != json!([]) {
        return Err("Multi-user burst was not trailing-only within450ms".into());
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
    let corrected = remove(s, d, "pick", CASEY, &[BLAIR, ADA]).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "corrected-order", corrected, st, cap).await?;
    let started = Instant::now();
    let valid = toggle(s, d, "required", ADA, &[ADA]).await?;
    let empty = remove(s, d, "required", ADA, &[]).await?;
    let sibling = notes_received(s, "Required sibling").await?;
    let elapsed = started.elapsed().as_millis();
    let before = snapshot(&s.peek());
    if elapsed >= 450 || before["requests"].as_array().map(Vec::len) != Some(2) {
        return Err("Required multi-user invalid callback missed queued window".into());
    }
    counts(s, 3, 3, 0, 0).await?;
    shot(s,d,n,"required-empty-keeps-queued",json!({"valid":valid,"empty":empty,"sibling":sibling,"elapsedMs":elapsed,"beforeDelivery":before}),st,cap).await?;
    let quiet = toggle(s, d, "quiet", BLAIR, &[ADA, BLAIR]).await?;
    let single = choose(s, d, "single", CASEY).await?;
    let pressed = click(s, d).await?;
    counts(s, 3, 3, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Multi-user Button reply not held".into());
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
    let removed = remove(s, d, "pick", ADA, &[BLAIR]).await?;
    counts(s, 4, 4, 1, 1).await?;
    let query = search(s, d, "pick", "reviewer", 1).await?;
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
    let remote = toggle(s, d, "pick", REMOTE, &[BLAIR, REMOTE]).await?;
    counts(s, 5, 5, 1, 1).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.users.contains_key(REMOTE))
    {
        return Err("Multi-user remote result polluted workspace cache".into());
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
