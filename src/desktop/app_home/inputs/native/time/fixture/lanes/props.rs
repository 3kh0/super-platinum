use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_time"] = json!("11:00")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "pristine-initial",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["timezone"] = json!("Asia/Tokyo")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "timezone-only",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![op("pick", "12", "blur")], None).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "current-callback",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_time"] = json!("13:00")
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "dirty-initial",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(
        &mut s.write(),
        |r| r["state"] = json!({"values":{"pick":{"value":{"type":"timepicker","selected_time":"16:00","timezone":"UTC"}}}}),
    );
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "saved-only",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r.as_object_mut().unwrap().remove("state");
    });
    settle(s, d).await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-current",
        json!({"refresh":source,"reset":reset,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    {
        let mut state = s.write();
        let w = state.core.workspaces.get_mut("T1").unwrap();
        w.locale = Some("de-DE".into());
        w.notifications.user.insert("time24".into(), json!(true));
    }
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][3]["label"]["text"] = json!("Current release notes")
    });
    settle(s, d).await?;
    let change = edit(s, vec![op("pick", "5pm", "blur")], None).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(s,d,n,"locale-24h",json!({"refresh":source,"locale":"de-DE","time24":true,"actualPreferenceSources":true,"change":change}),st,cap).await?;
    let clear = edit(s, vec![op("pick", "garbage", "blur")], None).await?;
    counts(s, 3, 3, 0, 0).await?;
    let navigation = revisit(s, d).await?;
    let press = click(s, d).await?;
    counts(s, 3, 3, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 3, 3, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "cached-null-return",
        json!({"clear":clear,"navigation":navigation,"click":press}),
        st,
        cap,
    )
    .await
}
