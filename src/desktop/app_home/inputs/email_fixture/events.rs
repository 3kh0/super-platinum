use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let a = input(s, d, "default", "Default@Example.com").await?;
    let b = ignored_enter(
        s,
        d,
        "default",
        "Default@Example.com",
        wire("Default@Example.com"),
    )
    .await?;
    let c = blur(
        s,
        d,
        "default",
        "Default@Example.com",
        wire("Default@Example.com"),
    )
    .await?;
    let quiet = silence(s).await?;
    counts(s, 0, 0, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "default-no-dispatch",
        json!({"input":a,"enter":b,"blur":c,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let a = input(s, d, "enter", "Mixed+tag@Example.COM").await?;
    let sibling = notes(s, "Leading email sibling").await?;
    let started = Instant::now();
    let b = enter(s, d, "enter", "Mixed+tag@Example.COM").await?;
    counts(s, 1, 1, 0, 0).await?;
    let leading = helpers::leading(&s.peek(), started, 1)?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "literal-enter-leading",
        json!({"input":a,"notes":sibling,"enter":b,"leading":leading,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let started = Instant::now();
    let a = input(s, d, "live", "first@example.com").await?;
    counts(s, 2, 2, 0, 0).await?;
    let leading = helpers::leading(&s.peek(), started, 2)?;
    let b = input(s, d, "live", "second@example.com").await?;
    let equal = input(s, d, "live", "second@example.com").await?;
    let sibling = notes(s, "Latest email sibling").await?;
    let before = trailing(&s.peek(), started, 2)?;
    counts(s, 3, 3, 0, 0).await?;
    shot(s,d,n,"character-leading-trailing",json!({"first":a,"leading":leading,"second":b,"equal":equal,"notes":sibling,"beforeDelivery":before}),st,cap).await?;
    let p = input(s, d, "live", "a@localhost").await?;
    let quiet = silence(s).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "invalid-parent-policy",
        json!({"input":p,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let p = input(s, d, "live", ".a..b.@example.com").await?;
    counts(s, 4, 4, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "unusual-valid-address",
        json!({"input":p,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][2]["element"]["dispatch_action_config"] =
            json!({"trigger_actions_on":["on_enter_pressed"]})
    });
    settle(s, d).await?;
    let a = input(s, d, "default", "").await?;
    let b = enter(s, d, "default", "").await?;
    counts(s, 5, 5, 0, 0).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "optional-empty-enter",
        json!({"source":source,"input":a,"enter":b,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let a = input(s, d, "quiet", "Quiet+Tag@Example.com").await?;
    let b = blur(
        s,
        d,
        "quiet",
        "Quiet+Tag@Example.com",
        wire("Quiet+Tag@Example.com"),
    )
    .await?;
    let quiet = silence(s).await?;
    let clicked = button().await?;
    counts(s, 5, 5, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Email Button reply missing".into());
    }
    counts(s, 5, 5, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "quiet-button-state",
        json!({"input":a,"blur":b,"silence":quiet,"button":clicked}),
        st,
        cap,
    )
    .await?;
    let p = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":p,"silence":quiet}),
        st,
        cap,
    )
    .await
}
