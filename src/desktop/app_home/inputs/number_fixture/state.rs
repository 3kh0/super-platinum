use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_value"] = json!("4.00")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "initial-prop-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source =
        refresh(
            &mut s.write(),
            |r| {
                r["state"] =
                    json!({"values":{"enter":{"value":{"type":"number_input","value":"8"}}}})
            },
        );
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "saved-prop-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_value");
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "initial-removed-ignored",
        json!({"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let p = navigation::revisit(s, d).await?;
    shot(s, d, n, "retained-parent-return", p, st, cap).await?;
    let edit = input(s, d, "enter", "6.0", json!("6.0"), wire(json!("6"))).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_value"] = json!("7.00")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "draft-retained-current-metadata",
        json!({"input":edit,"source":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let p = enter(s, d, "enter", json!("6.0"), wire(json!("6.0"))).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "literal-current-enter", p, st, cap).await?;
    let p = blur(s, d, "enter", json!("6"), wire(json!("6"))).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s, d, n, "blur-current-metadata", p, st, cap).await?;
    let started = Instant::now();
    let a = input(s, d, "live", "6", json!("6"), wire(json!("6"))).await?;
    let b = input(s, d, "live", "12", json!("12"), wire(json!("12"))).await?;
    let before = trailing(&s.peek(), started, 2)?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "invalid-parent-keeps-queued",
        json!({"valid":a,"invalid":b,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    let p = input(s, d, "enter", "4", json!("4"), wire(json!("4"))).await?;
    let started = Instant::now();
    let a = enter(s, d, "enter", json!("4"), wire(json!("4"))).await?;
    let b = input(s, d, "enter", "", json!(""), wire(Value::Null)).await?;
    let before = trailing(&s.peek(), started, 3)?;
    counts(s, 4, 4, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "required-empty-keeps-queued",
        json!({"input":p,"enter":a,"clear":b,"beforeDelivery":before}),
        st,
        cap,
    )
    .await?;
    let p = input(s, d, "integer", "-1", json!("-1"), wire(json!("-1"))).await?;
    counts(s, 5, 5, 0, 0).await?;
    shot(s, d, n, "zero-bound-domain-policy", p, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][1]["element"]["dispatch_action_config"] = json!({"trigger_actions_on":[]})
    });
    settle(s, d).await?;
    let a = input(s, d, "live", "8.00", json!("8.00"), wire(json!("8"))).await?;
    let quiet = silence(s).await?;
    let b = blur(s, d, "live", json!("8"), wire(json!("8"))).await?;
    counts(s, 6, 6, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "empty-config-blur-emits",
        json!({"source":source,"input":a,"silence":quiet,"blur":b}),
        st,
        cap,
    )
    .await?;
    let p = navigation::revisit(s, d).await?;
    shot(s, d, n, "null-parent-fallback-display", p, st, cap).await?;
    let p = button().await?;
    counts(s, 6, 6, 1, 0).await?;
    if !buttons::release(&mut s.write(), true) {
        return Err("Number state Button reply missing".into());
    }
    counts(s, 6, 6, 1, 1).await?;
    shot(s, d, n, "button-preserves-null-parent", p, st, cap).await?;
    let p = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-raw-saved",
        json!({"reset":p,"silence":quiet}),
        st,
        cap,
    )
    .await
}
