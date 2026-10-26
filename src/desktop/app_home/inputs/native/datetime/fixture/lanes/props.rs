use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_date_time"] = json!(1793032662i64);
        r["blocks"][0]["element"]["timezone"] = json!("Asia/Tokyo");
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "pristine-props",
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
    let draft = edit(s, vec![op("pick", "17:45", "none")], None).await?;
    let change = edit(s, vec![date_without_focus("pick", "2026-10-26")], None).await?;
    counts(s, 2, 2, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "time-draft-through-date",
        json!({"draft":draft,"dateChange":change,"dateFocusRequested":false}),
        st,
        cap,
    )
    .await?;
    let change = edit(s, vec![commit("pick", "blur")], None).await?;
    counts(s, 3, 3, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "draft-committed",
        json!({"change":change}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_date_time"] = json!(1793119062i64);
        r["state"] = json!({"values":{"pick":{"value":{"type":"datetimepicker","selected_date_time":1793205462i64}}}});
    });
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "dirty-saved-props",
        json!({"refresh":source,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let source = refresh(&mut s.write(), |r| {
        r.as_object_mut().unwrap().remove("state");
    });
    settle(s, d).await?;
    let reset_proof = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-current",
        json!({"refresh":source,"reset":reset_proof,"silence":quiet}),
        st,
        cap,
    )
    .await?;
    let clear = edit(s, vec![date("pick", "", "change")], None).await?;
    counts(s, 4, 4, 0, 0).await?;
    let navigation = revisit(s, d).await?;
    let press = click(s, d).await?;
    counts(s, 4, 4, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 4, 4, 1, 1).await?;
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
