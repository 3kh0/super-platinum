use super::*;
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
fn visit(s: &ShellState) -> actions::Key {
    let h = s.app_home.surface.as_ref().unwrap();
    actions::Key {
        visit: h.id,
        revision: h.revision,
    }
}
async fn acquire(s: Signal<ShellState>) -> Option<inputs::native::Release> {
    let token = visit(&s.peek());
    inputs::native::lock_owned(s, token).await
}
async fn released(d: &DesktopContext) -> Result<(), String> {
    capture::wait(
        d,
        "[...(window.__spHomeCalendarNative?.entries.values()||[])].every(e=>!e.lease)",
    )
    .await?;
    Ok(())
}
pub(super) async fn run(
    mut s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    let adapter = cooldown(&mut s.write(), true);
    let source = flags(&mut s.write(), Some("pick"));
    observed(s, d).await?;
    let disabled = active(d, "pick", false).await?;
    if disabled["target"]["disabled"] != true {
        return Err("Date cooldown did not disable actual input".into());
    }
    shot(
        s,
        d,
        n,
        "disabled-never-focused",
        "pick",
        json!({"adapter":adapter,"source":source,"observed":disabled}),
        st,
        cap,
    )
    .await?;
    let adapter = cooldown(&mut s.write(), false);
    settle(s, d).await?;
    let focused = active(d, "pick", true).await?;
    selected_all(&focused)?;
    shot(
        s,
        d,
        n,
        "enabled-retry",
        "pick",
        json!({"adapter":adapter,"observed":focused}),
        st,
        cap,
    )
    .await?;

    static_select::fixture::configure(&mut s.write(), static_select::fixture::Reply::Held);
    let change = existing::gestures::edit(
        s,
        vec![existing::gestures::op("pick", "2026-11-10", "enter")],
        None,
    )
    .await?;
    existing::helpers::counts(s, 1, 0, 0, 0).await?;
    observed(s, d).await?;
    let pending = active(d, "pick", false).await?;
    if pending["target"]["disabled"] != true {
        return Err("Pending Date callback did not disable its actual input".into());
    }
    shot(
        s,
        d,
        n,
        "pending-disabled",
        "pick",
        json!({"change":change,"observed":pending}),
        st,
        cap,
    )
    .await?;
    let away = probe("away", "pick", json!({})).await?;
    let before = effect("pick").await?;
    let request = snapshot(&s.peek())["requests"][0]["id"]
        .as_u64()
        .ok_or("Date held request missing")?;
    if !static_select::fixture::release(
        &mut s.write(),
        request,
        static_select::fixture::Reply::Accepted,
    ) {
        return Err("Date held release missing".into());
    }
    existing::helpers::counts(s, 1, 1, 0, 0).await?;
    settle(s, d).await?;
    let silence = quiet(s).await?;
    let after = effect("pick").await?;
    if before["active"] != after["active"] || after["target"]["active"] == true {
        return Err("Date response release stole focus".into());
    }
    shot(
        s,
        d,
        n,
        "response-no-refocus",
        "pick",
        json!({"away":away,"before":before,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let off = flags(&mut s.write(), None);
    settle(s, d).await?;
    let lease = acquire(s)
        .await
        .ok_or("Actual Date lease could not be acquired")?;
    let source = flags(&mut s.write(), Some("pick"));
    observed(s, d).await?;
    let away = probe("quiet-away", "pick", json!({"id":id("notes")})).await?;
    let held = effect("pick").await?;
    if held["target"]["lease"] != true
        || held["target"]["readOnly"] != true
        || held["target"]["active"] == true
    {
        return Err("Date focus crossed held readonly calendar lease".into());
    }
    shot(s,d,n,"lease-away-held","pick",json!({"off":off,"source":source,"away":away,"observed":held,"adapter":"Direct production native::lock_owned; transport integration covered by shared admission cases"}),st,cap).await?;
    lease.release();
    released(d).await?;
    settle(s, d).await?;
    let silence = quiet(s).await?;
    let after = effect("pick").await?;
    if held["active"] != after["active"]
        || after["target"]["active"] == true
        || after["target"]["lease"] != false
    {
        return Err("Date lease release moved focus or retained lease".into());
    }
    shot(
        s,
        d,
        n,
        "lease-away-released",
        "pick",
        json!({"before":held,"after":after,"silence":silence}),
        st,
        cap,
    )
    .await?;

    let focus = probe("focus", "pick", json!({})).await?;
    observed(s, d).await?;
    let unseen = probe("unseen", "pick", json!({"value":"unreported invalid Date"})).await?;
    let rejected = acquire(s).await;
    if let Some(lease) = rejected {
        lease.release();
        return Err("Unreported Date text was admitted".into());
    }
    shot(s,d,n,"unseen-refused","pick",json!({"focus":focus,"unseen":unseen,"leaseRefused":true,"adapter":"Direct production final native lease; no reported input or permission grant"}),st,cap).await?;
    let correction = probe("input", "pick", json!({"value":"November 10th, 2026"})).await?;
    settle(s, d).await?;
    let lease = acquire(s)
        .await
        .ok_or("Acknowledged valid Date correction stayed blocked")?;
    lease.release();
    released(d).await?;
    settle(s, d).await?;
    let silence = quiet(s).await?;
    shot(
        s,
        d,
        n,
        "observed-recovery",
        "pick",
        json!({"correction":correction,"leaseAdmitted":true,"silence":silence}),
        st,
        cap,
    )
    .await
}
