use super::*;
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_users"] = json!([CASEY, BLAIR]);
        r["blocks"][1]["element"]["initial_users"] = json!([BLAIR]);
    });
    shot(s, d, n, "pristine-initial-changed", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["state"] = json!({"values":{
            "pick":{"value":{"type":"multi_users_select","selected_users":[BLAIR]}},
            "required":{"value":{"type":"multi_users_select","selected_users":[CASEY]}}
        }})
    });
    shot(s, d, n, "saved-only-ignored", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["max_selected_items"] = json!(1)
    });
    shot(s, d, n, "max-only-guidance", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_users");
    });
    shot(s, d, n, "initial-removed-ignored", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_users"] = json!([]);
        r["blocks"][1]["element"]["initial_users"] = json!([]);
    });
    shot(s, d, n, "initial-empty-display", source, st, cap).await?;
    let chosen = toggle(s, d, "pick", BLAIR, &[BLAIR]).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "callback-current-metadata", chosen, st, cap).await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-parent-seeded", navigation, st, cap).await?;
    let cleared = remove(s, d, "pick", BLAIR, &[]).await?;
    counts(s, 2, 2, 0, 0).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_users"] = json!([CASEY])
    });
    shot(
        s,
        d,
        n,
        "explicit-empty-parent-retained",
        json!({"clear":cleared,"refresh":source}),
        st,
        cap,
    )
    .await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-explicit-empty", navigation, st, cap).await?;
    let reset = reset(s, d).await?;
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset-saved-seed",
        json!({"reset":reset,"silence":quiet,"resetIsProductAdapter":true}),
        st,
        cap,
    )
    .await
}
