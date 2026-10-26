use super::*;
pub(in super::super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let source = refresh(&mut s.write(), |raw| {
        raw["state"] = json!({"values":{"pick":{"value":{"type":"multi_static_select","selected_options":[base::option("alpha","Alpha")]}}}});
        raw["blocks"][0]["element"]["initial_options"] = json!([base::option("beta", "Beta")]);
    });
    shot(s,d,n,"props-display-only",json!({"refresh":source,"rawIngressLimit":"Product raw refresh; reference establishes mapped props separately."}),st,cap).await?;
    let source = refresh(&mut s.write(), |raw| {
        raw["blocks"][0]["element"]
            .as_object_mut()
            .unwrap()
            .remove("initial_options");
        raw["blocks"][0]["element"]["option_groups"][0]["options"] = json!([
            base::option("alpha", "Alpha"),
            base::option("gamma", "Gamma"),
            base::option("", "No release")
        ]);
    });
    shot(
        s,
        d,
        n,
        "option-removed-retained-chip",
        json!({"refresh":source}),
        st,
        cap,
    )
    .await?;
    let removed = edit(s, d, "pick", vec![remove(0)]).await?;
    counts(s, 7, 7, 0, 0).await?;
    let source = refresh(&mut s.write(), |raw| {
        raw.as_object_mut().unwrap().remove("state");
        raw["blocks"][0]["element"]["option_groups"][0]["options"] = json!([
            base::option("alpha", "Alpha"),
            base::option("beta", "Beta current"),
            base::option("gamma", "Gamma"),
            base::option("", "No release")
        ])
    });
    let change = edit(s, d, "pick", vec![toggle(1)]).await?;
    counts(s, 8, 8, 0, 0).await?;
    shot(
        s,
        d,
        n,
        "current-option-metadata",
        json!({"refresh":source,"change":change,"removedUnavailableToken":removed}),
        st,
        cap,
    )
    .await?;
    let quiet_sibling = quiet_edit(s).await?;
    let press = click(s, d).await?;
    counts(s, 8, 8, 1, 0).await?;
    buttons::release(&mut s.write(), true);
    counts(s, 8, 8, 1, 1).await?;
    shot(
        s,
        d,
        n,
        "button-state",
        json!({"click":press,"quietSibling":quiet_sibling}),
        st,
        cap,
    )
    .await?;
    let before = snapshot(&s.peek());
    let reset=capture::dom(&format!(r#"(()=>{{{}const b=[...document.querySelectorAll('#app-home button')].find(e=>e.textContent.trim()==='Reset inputs');b?.scrollIntoView({{block:'center'}});if(!owned(b))return{{ok:false}};b.click();return{{ok:true,text:b.textContent,visibleHitTested:true}};}})()"#,capture::ACTION_GUARD)).await?;
    for _ in 0..200 {
        if snapshot(&s.peek())["authority"]["homeRevision"] != before["authority"]["homeRevision"] {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let quiet = silence(s).await?;
    shot(
        s,
        d,
        n,
        "reset",
        json!({"reset":reset,"silence":quiet}),
        st,
        cap,
    )
    .await
}
