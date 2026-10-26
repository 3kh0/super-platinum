use super::*;
pub(super) async fn mount(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for block in FIELDS {
        let source = flags(&mut s.write(), Some(block));
        let navigation = navigation::revisit(s, d, Some(block)).await?;
        let silence = quiet(s, block).await?;
        shot(
            s,
            d,
            n,
            &format!("mount-{block}"),
            block,
            json!({"source":source,"navigation":navigation,"silence":silence}),
            st,
            cap,
        )
        .await?;
    }
    let source = flags(&mut s.write(), None);
    let navigation = navigation::revisit(s, d, None).await?;
    let silence = quiet(s, "qsingle").await?;
    no_field_focus(&silence["last"])?;
    shot(
        s,
        d,
        n,
        "mount-false",
        "qsingle",
        json!({"source":source,"navigation":navigation,"silence":silence}),
        st,
        cap,
    )
    .await
}
pub(super) async fn updates(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for block in FIELDS {
        let before = snapshot(&s.peek());
        let source = flags(&mut s.write(), Some(block));
        settle(s, d).await?;
        let observed = active(d, block, true).await?;
        if widget(&before, block)["instance"] != widget(&snapshot(&s.peek()), block)["instance"] {
            return Err("Flag change replaced logical instance".into());
        }
        shot(
            s,
            d,
            n,
            &format!("focus-{block}"),
            block,
            json!({"source":source,"before":before,"observed":observed}),
            st,
            cap,
        )
        .await?;
        let away = probe("away", block, json!({})).await?;
        settle(s, d).await?;
        // Number's established blur reducer canonically commits 5.00 -> 5.
        // Let this legitimate action complete before updating its source.
        if block == "amount" {
            counts(s, 1, 1).await?;
        }
        let blur_settled = snapshot(&s.peek());
        let source = helpers::refresh(&mut s.write(), |raw| {
            let b = raw["blocks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r["block_id"] == block)
                .unwrap();
            b["element"]["placeholder"] = json!({"type":"plain_text","text":"Updated focus hint"});
        });
        settle(s, d).await?;
        let silence = quiet(s, block).await?;
        if silence["last"]["active"] != away["effect"]["active"]
            || widget(&before, block)["instance"] != widget(&snapshot(&s.peek()), block)["instance"]
        {
            return Err(
                "Unchanged true stole explicit focus-away or changed logical instance".into(),
            );
        }
        shot(
            s,
            d,
            n,
            &format!("away-{block}"),
            block,
            json!({"away":away,"blurSettled":blur_settled,"source":source,"silence":silence}),
            st,
            cap,
        )
        .await?;
    }
    Ok(())
}
pub(super) async fn lifecycle(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = flags(&mut s.write(), Some("qsingle"));
    settle(s, d).await?;
    let observed = active(d, "qsingle", true).await?;
    let input = probe("input", "qsingle", json!({"value":"Reset draft"})).await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["formState"]["values"]["qsingle"]["value"]["value"] == "Reset draft"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if snapshot(&s.peek())["formState"]["values"]["qsingle"]["value"]["value"] != "Reset draft" {
        return Err("Reset draft input was not acknowledged".into());
    }
    settle(s, d).await?;
    let before = snapshot(&s.peek());
    shot(
        s,
        d,
        n,
        "reset-start",
        "qsingle",
        json!({"source":source,"observed":observed,"input":input}),
        st,
        cap,
    )
    .await?;
    let reset = probe("reset", "qsingle", json!({})).await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    settle(s, d).await?;
    let silence = quiet(s, "qsingle").await?;
    if widget(&before, "qsingle")["instance"] != widget(&snapshot(&s.peek()), "qsingle")["instance"]
        || silence["last"]["target"]["active"] == true
    {
        return Err("Product Reset unexpectedly rearmed focus".into());
    }
    shot(
        s,
        d,
        n,
        "reset-no-refocus",
        "qsingle",
        json!({"before":before,"reset":reset,"silence":silence,"productResetAdapter":true}),
        st,
        cap,
    )
    .await?;
    let navigation = navigation::revisit(s, d, Some("qsingle")).await?;
    shot(
        s,
        d,
        n,
        "return-fresh",
        "qsingle",
        json!({"navigation":navigation}),
        st,
        cap,
    )
    .await?;
    let prior = snapshot(&s.peek());
    let removed = prior["rawView"]["blocks"][0].clone();
    let source = helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["block_id"] != "qsingle")
    });
    settle(s, d).await?;
    if !widget(&snapshot(&s.peek()), "qsingle").is_null() {
        return Err("Removed focus instance retained".into());
    }
    shot(
        s,
        d,
        n,
        "removed",
        "single",
        json!({"before":prior,"source":source}),
        st,
        cap,
    )
    .await?;
    let source = helpers::refresh(&mut s.write(), |raw| {
        raw["blocks"].as_array_mut().unwrap().insert(0, removed)
    });
    settle(s, d).await?;
    let observed = active(d, "qsingle", true).await?;
    if widget(&prior, "qsingle")["instance"] == widget(&snapshot(&s.peek()), "qsingle")["instance"]
    {
        return Err("Reintroduced field reused retired focus instance".into());
    }
    shot(
        s,
        d,
        n,
        "readded",
        "qsingle",
        json!({"source":source,"observed":observed}),
        st,
        cap,
    )
    .await?;
    for (name, multiline) in [("kind-multiline", true), ("kind-single", false)] {
        let before = snapshot(&s.peek());
        let source = helpers::refresh(&mut s.write(), |raw| {
            raw["blocks"][0]["element"]["multiline"] = json!(multiline)
        });
        settle(s, d).await?;
        let observed = active(d, "qsingle", true).await?;
        if widget(&before, "qsingle")["instance"]
            == widget(&snapshot(&s.peek()), "qsingle")["instance"]
        {
            return Err("Changed kind retained focus instance".into());
        }
        shot(
            s,
            d,
            n,
            name,
            "qsingle",
            json!({"before":before,"source":source,"observed":observed}),
            st,
            cap,
        )
        .await?;
    }
    let away = probe("away", "qsingle", json!({})).await?;
    let before = snapshot(&s.peek());
    let source = {
        let mut state = s.write();
        let source = flags(&mut state, Some("email"));
        state.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        source
    };
    capture::wait(d,"!!document.querySelector('.app-home') && !document.querySelector('.app-home input,.app-home textarea')").await?;
    for _ in 0..200 {
        let x = snapshot(&s.peek());
        if x["textMounts"] == json!([]) && x["queue"] == json!([]) && x["activity"] == json!([]) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let silence = quiet(s, "home-empty").await?;
    no_field_focus(&silence["last"])?;
    shot(s,d,n,"owner-retired","home-empty",json!({"before":before,"away":away,"source":source,"silence":silence,"sameWriteOwnerRetirement":true}),st,cap).await
}
