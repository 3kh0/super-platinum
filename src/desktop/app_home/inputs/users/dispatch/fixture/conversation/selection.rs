use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let started = Instant::now();
    let first = choose(s, d, "pick", BETA).await?;
    let leading = prepared(s, 1, started).await?;
    let second = choose(s, d, "pick", ADA).await?;
    let cleared = clear(s, d, "pick", ADA).await?;
    let sibling = notes("Latest sibling").await?;
    let elapsed = started.elapsed().as_millis();
    if elapsed >= 450 {
        return Err("Conversation rapid callbacks missed 450ms bound".into());
    }
    let before = snapshot(&s.peek());
    if before["requests"].as_array().map(Vec::len) != Some(1) {
        return Err("Trailing conversation action prepared before burst proof".into());
    }
    counts(s, 2, 2, 0, 0).await?;
    shot(s,d,n,"rapid-leading-trailing",json!({"first":first,"leadingPrepared":leading,"second":second,"clear":cleared,"sibling":sibling,"elapsedMs":elapsed,"beforeDelivery":before}),st,cap).await?;
    let first = choose(s, d, "pick", BETA).await?;
    counts(s, 3, 3, 0, 0).await?;
    let repeated = choose(s, d, "pick", BETA).await?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "same-row-callback",
        json!({"first":first,"repeated":repeated}),
        st,
        cap,
    )
    .await?;
    let query = search(s, d, "pick", "Ada, Blair", 1).await?;
    shot(s, d, n, "remote-results", json!({"query":query}), st, cap).await?;
    // The result is selected from the current query; reopening would retire it.
    let selected = capture::click(&format!(
        "#app-home [data-modal-users='pick'] [data-entity-id='{REMOTE_USER}']"
    ))
    .await?;
    counts(s, 5, 5, 0, 0).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.users.contains_key(REMOTE_USER))
    {
        return Err("Remote member choice polluted workspace users".into());
    }
    shot(
        s,
        d,
        n,
        "remote-selected",
        json!({"selected":selected,"remoteNotInWorkspace":true}),
        st,
        cap,
    )
    .await?;
    let query = search(s, d, "pick", "Ada, Blair", 2).await?;
    let selected = capture::click(&format!(
        "#app-home [data-modal-users='pick'] [data-entity-id='{REMOTE}']"
    ))
    .await?;
    counts(s, 6, 6, 0, 0).await?;
    if s.peek()
        .core
        .workspaces
        .values()
        .any(|w| w.channels.contains_key(REMOTE) || w.users.contains_key(REMOTE_USER))
    {
        return Err("Remote conversation choices polluted workspace caches".into());
    }
    let active = snapshot(&s.peek())["activeConversation"].clone();
    if active != st[0]["model"]["activeConversation"] {
        return Err("Conversation selection opened a different conversation".into());
    }
    shot(s,d,n,"remote-mpim-selected",json!({"query":query,"selected":selected,"remoteNotInWorkspace":true,"activeConversationUnchanged":true}),st,cap).await?;
    let quiet = choose(s, d, "quiet", ADA).await?;
    let required = choose(s, d, "required", PRIVATE).await?;
    counts(s, 7, 7, 0, 0).await?;
    let press = click(s, d).await?;
    counts(s, 7, 7, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Conversation Button reply was not held".into());
    }
    counts(s, 7, 7, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "required-quiet-button",
        json!({"quiet":quiet,"required":required,"click":press,"requiredClearUIClaim":false}),
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
