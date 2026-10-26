use super::*;
// A declared offline cooldown adapter makes actual native disabled timing
// deterministic. It is not a claim about a newly delivered rate-limit response.
fn cooldown(s: &mut ShellState, enabled: bool) -> Value {
    let ctx = s.app_home.surface.as_ref().unwrap().owner.ctx.clone();
    let key = (ctx.epoch, ctx.session.team_id.clone());
    if enabled {
        s.app_home
            .cooldowns
            .insert(key, Some(Instant::now() + Duration::from_secs(60)));
    } else {
        s.app_home.cooldowns.remove(&key);
    }
    text::observe_cooling(s);
    json!({"offlineCooldownAdapter":true,"enabled":enabled,"seconds":if enabled{60}else{0},"team":ctx.session.team_id,"epoch":ctx.epoch})
}
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let disabled = cooldown(&mut s.write(), true);
    let source = flags(&mut s.write(), Some("qsingle"));
    settle(s, d).await?;
    let observed = active(d, "qsingle", false).await?;
    if observed["target"]["disabled"] != true {
        return Err("Single-line cooldown did not disable native input".into());
    }
    shot(
        s,
        d,
        n,
        "single-disabled",
        "qsingle",
        json!({"adapter":disabled,"source":source,"observed":observed}),
        st,
        cap,
    )
    .await?;
    let enabled = cooldown(&mut s.write(), false);
    let observed = active(d, "qsingle", true).await?;
    shot(
        s,
        d,
        n,
        "single-enabled-retry",
        "qsingle",
        json!({"adapter":enabled,"observed":observed}),
        st,
        cap,
    )
    .await?;
    let disabled = cooldown(&mut s.write(), true);
    let source = flags(&mut s.write(), Some("qmulti"));
    settle(s, d).await?;
    let observed = active(d, "qmulti", false).await?;
    if observed["target"]["disabled"] != true {
        return Err("Multiline cooldown did not disable native input".into());
    }
    shot(
        s,
        d,
        n,
        "multi-disabled",
        "qmulti",
        json!({"adapter":disabled,"source":source,"observed":observed}),
        st,
        cap,
    )
    .await?;
    let enabled = cooldown(&mut s.write(), false);
    settle(s, d).await?;
    let silence = quiet(s, "qmulti").await?;
    if silence["last"]["target"]["active"] != false
        || silence["last"]["target"]["disabled"] != false
    {
        return Err("Multiline unchanged true unexpectedly retried focus".into());
    }
    shot(
        s,
        d,
        n,
        "multi-enabled-no-retry",
        "qmulti",
        json!({"adapter":enabled,"silence":silence}),
        st,
        cap,
    )
    .await?;
    let off = flags(&mut s.write(), None);
    settle(s, d).await?;
    let on = flags(&mut s.write(), Some("qmulti"));
    settle(s, d).await?;
    let observed = active(d, "qmulti", true).await?;
    shot(
        s,
        d,
        n,
        "multi-toggle-recovery",
        "qmulti",
        json!({"off":off,"on":on,"observed":observed}),
        st,
        cap,
    )
    .await?;
    let source = flags(&mut s.write(), Some("email"));
    settle(s, d).await?;
    let focused = active(d, "email", true).await?;
    base::configure(&mut s.write(), base::Reply::Held);
    let press = probe("enter", "email", json!({})).await?;
    counts(s, 1, 0).await?;
    let pending = snapshot(&s.peek())["pendingIds"][0]
        .as_u64()
        .ok_or("Held email missing")?;
    let observed = active(d, "email", false).await?;
    if observed["target"]["disabled"] != true {
        return Err("Email pending state did not disable control".into());
    }
    shot(s,d,n,"email-pending","email",json!({"source":source,"focused":focused,"press":press,"pending":pending,"observed":observed}),st,cap).await?;
    if !base::release(&mut s.write(), pending, base::Reply::Accepted) {
        return Err("Held email release failed".into());
    }
    counts(s, 1, 1).await?;
    settle(s, d).await?;
    let silence = quiet(s, "email").await?;
    if silence["last"]["active"] != observed["active"]
        || silence["last"]["target"]["focus"]["attempts"] != focused["target"]["focus"]["attempts"]
    {
        return Err("Email enabled update reclaimed focus".into());
    }
    shot(
        s,
        d,
        n,
        "email-enabled-no-refocus",
        "email",
        json!({"released":pending,"silence":silence}),
        st,
        cap,
    )
    .await?;
    base::configure(&mut s.write(), base::Reply::Accepted);
    let source = flags(&mut s.write(), Some("single"));
    settle(s, d).await?;
    let focused = active(d, "single", true).await?;
    let range = probe("range", "single", json!({})).await?;
    let index = base::admission::arm(&mut s.write());
    let press = probe("enter", "single", json!({})).await?;
    let mut held = Value::Null;
    for _ in 0..250 {
        let x = snapshot(&s.peek());
        if x["admission"]["waiting"] == true {
            held = x;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if held.is_null() {
        return Err("Short text lease did not reach calendar gate".into());
    }
    let during = effect("single").await?;
    if during["target"]["lease"] != true
        || during["target"]["readOnly"] != true
        || during["target"]["selection"] != range["effect"]["target"]["selection"]
    {
        return Err("Held lease changed original focus range".into());
    }
    let away = probe("quiet-away", "single", json!({"id":id("qsingle")})).await?;
    shot(s,d,n,"lease-away-held","qsingle",json!({"source":source,"focused":focused,"range":range,"attempt":index,"press":press,"held":held,"during":during,"away":away}),st,cap).await?;
    base::admission::release(&mut s.write())?;
    counts(s, 2, 2).await?;
    settle(s, d).await?;
    let silence = quiet(s, "qsingle").await?;
    if silence["last"]["active"] != away["effect"]["active"]
        || silence["last"]["globalLease"] != false
    {
        return Err("Lease release stole explicit focus-away".into());
    }
    shot(
        s,
        d,
        n,
        "lease-away-released",
        "qsingle",
        json!({"attempt":index,"silence":silence}),
        st,
        cap,
    )
    .await
}
