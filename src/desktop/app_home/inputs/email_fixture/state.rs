use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_value"] = json!("Changed@Example.com");
        r["blocks"][0]["element"]["placeholder"] =
            json!({"type":"plain_text","text":"Owner email"});
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "mounted-initial-no-sync",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(
        &mut s.write(),
        |r| r["state"] = json!({"values":{"enter":{"value":{"type":"email_text_input","value":"saved@example.com"}}}}),
    );
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "mounted-saved-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let pressed = button().await?;
    counts(s, 0, 0, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Email initial-state Button reply missing".into());
    }
    counts(s, 0, 0, 1, 1).await?;
    shot(s, d, n, "first-button-type-only", pressed, st, cap).await?;
    let p = navigation::revisit(s, d).await?;
    shot(s, d, n, "existing-parent-return", p, st, cap).await?;
    let edit = input(s, d, "enter", "Retained@Example.com").await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_value"] = json!("Fresh@Example.com")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "draft-current-props",
        json!({"input":edit,"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let p = navigation::revisit(s, d).await?;
    shot(s, d, n, "fresh-overwrites-parent", p, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
    });
    settle(s, d).await?;
    let p = navigation::revisit(s, d).await?;
    shot(
        s,
        d,
        n,
        "missing-initial-retains-parent",
        json!({"source":source,"navigation":p}),
        st,
        cap,
    )
    .await?;
    let started = Instant::now();
    let a = input(s, d, "live", "lead@example.com").await?;
    counts(s, 1, 1, 1, 1).await?;
    let leading = helpers::leading(&s.peek(), started, 1)?;
    let b = input(s, d, "live", "queued@example.com").await?;
    let c = input(s, d, "live", "a@localhost").await?;
    let sibling = notes(s, "Invalid email sibling").await?;
    let before = trailing(&s.peek(), started, 1)?;
    counts(s, 2, 2, 1, 1).await?;
    shot(s,d,n,"queued-valid-invalid-parent",json!({"first":a,"leading":leading,"queued":b,"invalid":c,"notes":sibling,"beforeDelivery":before}),st,cap).await?;
    let a = input(s, d, "enter", "lead@example.com").await?;
    let started = Instant::now();
    let b = enter(s, d, "enter", "lead@example.com").await?;
    counts(s, 3, 3, 1, 1).await?;
    let leading = helpers::leading(&s.peek(), started, 3)?;
    let c = input(s, d, "enter", "queued@example.com").await?;
    let queued = enter(s, d, "enter", "queued@example.com").await?;
    let clear = input(s, d, "enter", "").await?;
    let sibling = notes(s, "Required email sibling").await?;
    let before = trailing(&s.peek(), started, 3)?;
    counts(s, 4, 4, 1, 1).await?;
    shot(s,d,n,"required-empty-keeps-queued",json!({"input":a,"enter":b,"leading":leading,"secondInput":c,"queued":queued,"clear":clear,"notes":sibling,"beforeDelivery":before}),st,cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][1]["element"]["dispatch_action_config"] = json!({"trigger_actions_on":[]})
    });
    settle(s, d).await?;
    let a = input(s, d, "live", "quietagain@example.com").await?;
    let b = ignored_enter(
        s,
        d,
        "live",
        "quietagain@example.com",
        wire("quietagain@example.com"),
    )
    .await?;
    let c = blur(
        s,
        d,
        "live",
        "quietagain@example.com",
        wire("quietagain@example.com"),
    )
    .await?;
    let quiet = silence(s).await?;
    counts(s, 4, 4, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "empty-config-blur",
        json!({"source":source,"input":a,"enter":b,"blur":c,"silence":quiet}),
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
        "reset-raw-parent-display",
        json!({"reset":p,"silence":quiet}),
        st,
        cap,
    )
    .await
}
