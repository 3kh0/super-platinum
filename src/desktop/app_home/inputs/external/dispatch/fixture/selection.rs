use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let started = Instant::now();
    let first = choose(s, d, "pick", 0, alpha()).await?;
    let leading = prepared(s, 1, started).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, false);
    let second = choose(s, d, "pick", 0, gamma()).await?;
    let cleared = clear(s, d, "pick").await?;
    let sibling = notes_received(s, "Latest sibling").await?;
    let elapsed = started.elapsed().as_millis();
    let before = snapshot(&s.peek());
    if elapsed >= 450 || before["requests"].as_array().map(Vec::len) != Some(1) {
        return Err("External rapid callbacks missed leading/trailing window".into());
    }
    counts(s, 2, 2, 0, 0).await?;
    shot(s,d,n,"rapid-leading-trailing",json!({"first":first,"leadingPrepared":leading,"second":second,"clear":cleared,"sibling":sibling,"elapsedMs":elapsed,"beforeDelivery":before}),st,cap).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Options, false);
    let first = choose(s, d, "pick", 0, alpha()).await?;
    counts(s, 3, 3, 0, 0).await?;
    let repeated = choose(s, d, "pick", 0, alpha()).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "same-descriptor-callback",
        json!({"first":first,"repeated":repeated}),
        st,
        cap,
    )
    .await?;
    let opened = open(s, d, "pick").await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Replacement, true);
    let held = query(s, d, "pick", "refresh-descriptors", 6, true).await?;
    shot(
        s,
        d,
        n,
        "suggestions-held",
        json!({"open":opened,"query":held}),
        st,
        cap,
    )
    .await?;
    let chosen = choose(s, d, "pick", 0, alpha()).await?;
    counts(s, 5, 5, 0, 0).await?;
    for _ in 0..150 {
        if suggestions::pending_closed(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let released = suggestions::release(&mut s.write());
    if released["delivered"] != false {
        return Err("Chosen old suggestion did not retire pending query".into());
    }
    let observed_silence = silence(s).await?;
    shot(
        s,
        d,
        n,
        "held-options-selectable",
        json!({"choose":chosen,"release":released,"silence":observed_silence}),
        st,
        cap,
    )
    .await?;
    let opened = open(s, d, "pick").await?;
    shot(s, d, n, "replacement-results", opened, st, cap).await?;
    let renamed = choose(s, d, "pick", 0, gamma()).await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(s, d, n, "same-value-new-descriptor", renamed, st, cap).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::EmptyValue, false);
    let required = choose(
        s,
        d,
        "required",
        0,
        suggestions::option("Empty value release", ""),
    )
    .await?;
    counts(s, 7, 7, 0, 0).await?;
    suggestions::configure(&mut s.write(), suggestions::Reply::Options, false);
    let quiet = choose(s, d, "quiet", 1, beta()).await?;
    let press = click(s, d).await?;
    counts(s, 7, 7, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("External Button reply not held".into());
    }
    counts(s, 7, 7, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "empty-value-button",
        json!({"required":required,"quiet":quiet,"click":press,"requiredClearUIClaim":false}),
        st,
        cap,
    )
    .await?;
    let reset = reset(s, d).await?;
    let silence = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":reset,"silence":silence,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
