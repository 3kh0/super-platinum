//! Drives only the disconnected service; asserts rendered canonical state and wire.
use super::*;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = match run(state, &desktop).await {
            Ok(v) => v,
            Err(e) => json!({"ok":false,"error":e}),
        };
        eprintln!("channel-notifications-check: {result}");
    });
}
async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    if state.peek().core.session.is_some() || state.peek().core.transport.is_some() {
        return Err("Connected fixture refused".into());
    }
    wait(
        d,
        "({ok:document.getElementById('channel-notifications-open')?.disabled===false})",
    )
    .await?;
    click(d, "channel-notifications-open", false).await?;
    wait(
        d,
        "({ok:!!document.getElementById('channel-notifications-settings')})",
    )
    .await?;
    click(d, "channel-notifications-open", false).await?;
    wait(
        d,
        "({ok:!document.getElementById('channel-notifications-settings')})",
    )
    .await?;
    if count(&state.peek()) != 0 {
        return Err("Disclosure dispatched".into());
    }
    click(d, "channel-notifications-open", false).await?;
    switch(d, "everything", "aria-pressed", false).await?;
    step(state, d, "everything", 1, false).await?;
    switch(d, "everything", "aria-pressed", true).await?;
    switch(d, "threads", "aria-checked", false).await?;
    switch(d, "badges", "aria-checked", false).await?;
    click(d, "channel-notifications-mobile-disclosure", false).await?;
    step(state, d, "mobile-mentions", 2, false).await?;
    switch(d, "mobile-mentions", "aria-pressed", true).await?;
    step(state, d, "mentions", 3, false).await?;
    switch(d, "mentions", "aria-pressed", true).await?;
    switch(d, "broadcasts", "aria-checked", true).await?;
    step(state, d, "desktop", 4, false).await?;
    switch(d, "desktop", "aria-checked", false).await?;
    step(state, d, "mobile", 5, false).await?;
    switch(d, "mobile", "aria-checked", false).await?;
    wait(
        d,
        "({ok:!document.getElementById('channel-notifications-mobile-mentions')})",
    )
    .await?;
    step(state, d, "mobile", 6, false).await?;
    switch(d, "mobile", "aria-checked", true).await?;
    step(state, d, "threads", 7, false).await?;
    switch(d, "threads", "aria-checked", true).await?;
    step(state, d, "broadcasts", 8, false).await?;
    switch(d, "broadcasts", "aria-checked", false).await?;
    step(state, d, "everything", 9, false).await?;
    step(state, d, "badges", 10, false).await?;
    switch(d, "badges", "aria-checked", true).await?;
    step(state, d, "desktop", 11, true).await?;
    wait(d,"({ok:document.getElementById('channel-mute-check')?.disabled===false&&document.getElementById('channel-notifications-desktop')?.disabled===true})").await?;
    click(d, "channel-mute-check", true).await?;
    crate::slack_links_check::wait_state(state, |s| {
        s.channel_mute
            .settings_fixture
            .as_ref()
            .unwrap()
            .read
            .is_some()
    })
    .await?;
    fixture::complete_read(&mut state.write());
    switch(d, "desktop", "aria-checked", true).await?;
    let initial_layout = layout(d).await?;
    // Close/reopen preserves canonical settings and performs no new request.
    click(d, "channel-notifications-open", false).await?;
    click(d, "channel-notifications-open", false).await?;
    switch(d, "everything", "aria-pressed", true).await?;
    let final_layout = layout(d).await?;
    let s = state.peek();
    let f = s.channel_mute.settings_fixture.as_ref().unwrap();
    if !f.valid || f.requests.len() != 11 || f.reads != 11 {
        return Err(format!(
            "Request bounds/transport contract: valid={} writes={} reads={}",
            f.valid,
            f.requests.len(),
            f.reads
        ));
    }
    let req = &f.requests;
    let first: Value = serde_json::from_str(req[0]["prefs"].as_str().ok_or("missing prefs")?)
        .map_err(|e| e.to_string())?;
    if req[0]["sync"] != "true"
        || first
            != json!([{"name":"desktop","value":"everything","sync":true},{"name":"follow_all_threads","value":"false","sync":false},{"name":"badge_all_unreads","value":"false","sync":false}])
    {
        return Err(format!("Primary mode wire mismatch: {}", req[0]));
    }
    if req[2]["sync"] != "false" {
        return Err("Different mobile override was synchronized".into());
    }
    for (i, name, value) in [
        (1, "mobile", "mentions_dms"),
        (3, "desktop_push_enabled", "false"),
        (4, "mobile", "nothing"),
        (5, "mobile", "mentions_dms"),
        (6, "follow_all_threads", "true"),
        (7, "suppress_at_channel", "true"),
        (9, "badge_all_unreads", "true"),
        (10, "desktop_push_enabled", "true"),
    ] {
        if req[i]["channel_id"] != "C2"
            || req[i]["name"] != name
            || req[i]["value"] != value
            || req[i]["sync"] != "false"
        {
            return Err(format!("Advanced request {i} mismatch: {}", req[i]));
        }
    }
    let p = &s.core.workspaces["T1"].notifications;
    if p.global["unknown_global"] != json!({"keep":true})
        || p.channels["C1"]["unknown_other"] != 7
        || p.channels["C2"]["unknown_target"] != json!([1, 2])
    {
        return Err("Sibling preferences changed".into());
    }
    Ok(
        json!({"ok":true,"noTransport":true,"disclosureNoWrite":true,"primaryModes":true,"desktopPush":true,"mobilePush":true,"mobileOverride":true,"advancedOptions":true,"duplicateBlocked":true,"sharedMuteLock":true,"uncertainNoReplay":true,"canonicalReadback":true,"siblingPreserved":true,"exactWire":true,"attempts":11,"reads":11,"layout":initial_layout,"finalLayout":final_layout}),
    )
}
fn count(s: &ShellState) -> usize {
    s.channel_mute
        .settings_fixture
        .as_ref()
        .unwrap()
        .requests
        .len()
}
async fn step(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    n: usize,
    unknown: bool,
) -> Result<(), String> {
    click(d, &format!("channel-notifications-{id}"), true).await?;
    crate::slack_links_check::wait_state(s, |s| count(s) == n).await?;
    wait(d,"({ok:document.getElementById('channel-mute-action')?.disabled===true&&[...document.querySelectorAll('#channel-notifications-settings button:not(#channel-notifications-mobile-disclosure)')].every(b=>b.disabled)})").await?;
    // Injection during pending must not invoke mute or another setting.
    act(d,"document.getElementById('channel-mute-action').click();document.getElementById('channel-notifications-mentions').click()").await?;
    fixture::complete(&mut s.write(), unknown);
    if !unknown {
        wait(
            d,
            "({ok:document.getElementById('channel-notifications-everything')?.disabled===false})",
        )
        .await?;
    }
    if count(&s.peek()) != n {
        return Err("Duplicate action escaped shared receipt lock".into());
    }
    Ok(())
}
async fn switch(
    d: &dioxus::desktop::DesktopContext,
    id: &str,
    attr: &str,
    value: bool,
) -> Result<(), String> {
    wait(d,&format!("({{ok:document.getElementById('channel-notifications-{id}')?.getAttribute('{attr}')==='{value}'&&document.getElementById('channel-notifications-{id}')?.disabled===false}})")).await.map(|_|())
}
async fn click(d: &dioxus::desktop::DesktopContext, id: &str, twice: bool) -> Result<(), String> {
    wait(
        d,
        &format!("({{ok:document.getElementById('{id}')?.disabled===false}})"),
    )
    .await?;
    act(d,&format!("const b=document.getElementById('{id}');b.scrollIntoView({{block:'center'}});b.focus();b.click();{}",if twice{"b.click();"}else{""})).await
}
async fn act(d: &dioxus::desktop::DesktopContext, s: &str) -> Result<(), String> {
    wait(d, &format!("(()=>{{{s};return {{ok:true}}}})()"))
        .await
        .map(|_| ())
}
async fn wait(d: &dioxus::desktop::DesktopContext, s: &str) -> Result<Value, String> {
    crate::slack_links_check::wait_dom(d, s)
        .await
        .map_err(|e| format!("{e}; predicate={s}"))
}
async fn layout(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    act(d,"document.getElementById('channel-notifications-everything').scrollIntoView({block:'start'})").await?;
    wait(d,r#"(()=>{const modal=document.getElementById('channel-details-dialog'),panel=document.getElementById('channel-notifications-settings'),body=modal?.querySelector('.channel-details-body');if(!modal||!panel||!body)return {ok:false};const m=modal.getBoundingClientRect(),b=body.getBoundingClientRect(),buttons=[...panel.querySelectorAll('button')],mode=document.getElementById('channel-notifications-everything').getBoundingClientRect();const rows=buttons.map(e=>{const r=e.getBoundingClientRect();return [e.id,r.width,r.height,r.left,r.right]});return {ok:buttons.length>=8&&rows.every(r=>r[1]>0&&r[2]===32&&r[3]>=m.left&&r[4]<=m.right)&&m.left>=0&&m.top>=0&&m.right<=innerWidth&&m.bottom<=innerHeight&&mode.top>=b.top&&mode.bottom<=b.bottom,viewport:[innerWidth,innerHeight],rows,dialog:[m.left,m.top,m.right,m.bottom]};})()"#).await
}
