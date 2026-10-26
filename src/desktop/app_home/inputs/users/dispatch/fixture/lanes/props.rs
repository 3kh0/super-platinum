use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_user"] = json!(CASEY)
    });
    shot(s, d, n, "pristine-initial-changed", source, st, cap).await?;
    let source = refresh(
        &mut s.write(),
        |r| {
            r["state"] =
                json!({"values":{"pick":{"value":{"type":"users_select","selected_user":BLAIR}}}})
        },
    );
    shot(s, d, n, "saved-only-ignored", source, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_user");
    });
    shot(s, d, n, "initial-removed-parent-retained", source, st, cap).await?;
    let choice = choose(s, d, "pick", BLAIR).await?;
    counts(s, 1, 1, 0, 0).await?;
    shot(s, d, n, "callback-current-metadata", choice, st, cap).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_user"] = json!(CASEY)
    });
    shot(s, d, n, "draft-initial-changed", source, st, cap).await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-parent-seeded", navigation, st, cap).await?;
    let clear = clear(s, d, "pick", BLAIR).await?;
    counts(s, 2, 2, 0, 0).await?;
    let source = refresh(&mut s.write(), |r| {
        r["blocks"][0]["element"]["initial_user"] = json!(BLAIR);
        r["state"] =
            json!({"values":{"pick":{"value":{"type":"users_select","selected_user":CASEY}}}});
    });
    shot(
        s,
        d,
        n,
        "explicit-null-parent-retained",
        json!({"clear":clear,"refresh":source}),
        st,
        cap,
    )
    .await?;
    let navigation = revisit(s, d).await?;
    shot(s, d, n, "revisit-explicit-null", navigation, st, cap).await?;
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
