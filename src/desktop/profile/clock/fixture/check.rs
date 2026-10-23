use super::*;
use dioxus::prelude::{WritableExt, document};
use std::{path::PathBuf, time::Duration};

async fn eval(expression: &str) -> Result<Value, String> {
    document::eval(&format!("dioxus.send({expression});"))
        .recv::<Value>()
        .await
        .map_err(|e| format!("Profile clock native evaluation: {e}"))
}
async fn setup(mode: &str) -> Result<Value, String> {
    eval(&format!(
        "(()=>{{const mode={};return {};}})()",
        json!(mode),
        include_str!("setup.js").trim()
    ))
    .await
}
async fn observe(state: Signal<ShellState>, initialize: bool) -> Result<Value, String> {
    let e = expected(&state.peek());
    let script = format!(
        "(()=>{{const expected={e},initialize={initialize};return {};}})()",
        include_str!("native.js").trim()
    );
    let mut last = Value::Null;
    for _ in 0..100 {
        last = eval(&script).await?;
        if last["ok"] == true {
            return Ok(last);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!("Profile clock native state did not settle: {last}"))
}
async fn ack(ordinal: usize) -> Result<(), String> {
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
            .ok_or("Missing capture directory")?,
    );
    if !directory.is_absolute() {
        return Err("Capture directory must be absolute".into());
    }
    for _ in 0..600 {
        let p = directory.join(format!("{ordinal}.ack"));
        match tokio::task::spawn_blocking(move || std::fs::read(p))
            .await
            .map_err(|_| "Capture worker")?
        {
            Ok(bytes) if bytes == b"captured\n" => return Ok(()),
            Ok(_) => return Err("Malformed capture acknowledgment".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Capture acknowledgment read failed".into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Profile clock capture timeout".into())
}
async fn stage(
    state: Signal<ShellState>,
    name: &str,
    ordinal: usize,
    action: Value,
) -> Result<Value, String> {
    let native = observe(state, false).await?;
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,
        "model":model(&state.peek()),"native":native,"clock":evidence(),"action":action});
    eprintln!("profile-local-time-stage: {receipt}");
    ack(ordinal).await?;
    let after = observe(state, false).await?;
    if after != native {
        return Err(format!("Profile clock capture changed: {after}"));
    }
    Ok(receipt)
}
async fn clock_stage(
    state: Signal<ShellState>,
    name: &str,
    ordinal: usize,
    now: i64,
) -> Result<Value, String> {
    let before = evidence()["ticks"].as_array().unwrap().len();
    advance(now);
    settled_ticks(state, now, before).await?;
    stage(
        state,
        name,
        ordinal,
        json!({"kind":"synthetic-clock","now":now,"afterTick":before}),
    )
    .await
}
pub(crate) async fn run(mut state: Signal<ShellState>) {
    if !requested() {
        return;
    }
    let result = match tokio::time::timeout(Duration::from_secs(150), run_inner(&mut state)).await {
        Ok(Ok(v)) => v,
        Ok(Err(error)) => {
            json!({"ok":false,"error":error,"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"model":model(&state.peek()),"clock":evidence()})
        }
        Err(_) => json!({"ok":false,"error":"Profile clock native timeout"}),
    };
    eprintln!("profile-local-time-check: {result}");
}
async fn run_inner(state: &mut Signal<ShellState>) -> Result<Value, String> {
    if !enabled(&state.peek()) {
        return Err("Connected Profile clock fixture refused".into());
    }
    // Real original input handler. No controller or draft seeding.
    observe(*state, true).await?;
    let exposed = setup("probe").await?;
    let close = if exposed["exposed"] == true {
        Value::Null
    } else {
        let action = setup("close").await?;
        if action["ok"] != true {
            return Err(format!("Profile setup close refused: {action}"));
        }
        for _ in 0..100 {
            if state.peek().profile_user.is_none() && state.peek().profile_hover.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        let mut detached = false;
        for _ in 0..100 {
            if setup("closed").await?["closed"] == true {
                detached = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if !detached {
            return Err("Original setup pane remained connected".into());
        }
        action
    };
    let mut input = setup("input").await?;
    if input["ok"] != true {
        return Err(format!("Original draft input failed: {input}"));
    }
    for _ in 0..100 {
        if state.peek().core.composer.text
            == "Profile clock draft \u{4e2d}\nKeep caf\u{e9} \u{1f642}."
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if state.peek().core.composer.text != "Profile clock draft \u{4e2d}\nKeep caf\u{e9} \u{1f642}."
    {
        return Err("Original draft input did not commit".into());
    }
    input["setup"] = if close.is_null() {
        Value::Null
    } else {
        let draft = json!({"model":model(&state.peek()),"native":setup("draft").await?});
        let reopen = setup("reopen").await?;
        if reopen["ok"] != true {
            return Err(format!("Profile setup reopen refused: {reopen}"));
        }
        for _ in 0..100 {
            if state.peek().profile_user.as_deref() == Some("U1") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if state.peek().profile_user.as_deref() != Some("U1") {
            return Err("Original author did not reopen U1".into());
        }
        // Re-establish the clock-only identity baseline after the intentional remount.
        observe(*state, true).await?;
        let selection = setup("selection").await?;
        if selection["ok"] != true {
            return Err(format!("Profile setup selection refused: {selection}"));
        }
        json!({"close":close,"draft":draft,"reopen":reopen,"selection":selection})
    };
    let setup_tick = evidence()["ticks"].as_array().unwrap().len();
    settled_ticks(*state, INITIAL, setup_tick).await?;
    // Input/reopen can change narrow layout. Establish the clock-only scroll
    // and node baseline after those effects and two fresh runtime observations.
    observe(*state, true).await?;
    let mut stages = vec![stage(*state, "initial", 0, input).await?];
    for (name, epoch) in [
        ("same-minute", 1735801135),
        ("minute", 1735801145),
        ("midnight-before", 1735801199),
        ("midnight", 1735801200),
    ] {
        stages.push(clock_stage(*state, name, stages.len(), epoch).await?);
    }
    for (name, offset) in [("offset", Some(19800)), ("absent", None)] {
        let before = evidence()["ticks"].as_array().unwrap().len();
        {
            let mut s = state.write();
            let w = s
                .core
                .workspaces
                .get_mut("T1")
                .ok_or("Missing original workspace")?;
            let mut u = w.users.get("U1").ok_or("Missing original member")?.clone();
            u.tz_offset = offset;
            w.apply_user_update(u);
        }
        settled_ticks(*state, 1735801200, before).await?;
        stages.push(
            stage(
                *state,
                name,
                stages.len(),
                json!({"kind":"synthetic-member-update","user":"U1","offset":offset}),
            )
            .await?,
        );
    }
    let closed=eval(r#"(()=>{const w=window.__profileTimeEvidence,r=w.root;const b=r.querySelector('[aria-label="Close profile"]');if(b){const z=b.getBoundingClientRect(),x=z.x+z.width/2,y=z.y+z.height/2;if(!b.contains(document.elementFromPoint(x,y)))return {ok:false};const e=new MouseEvent('click',{bubbles:true});b.dispatchEvent(e);return {ok:true,kind:'synthetic-dom-click',isTrusted:e.isTrusted};}const e=new MouseEvent('mouseleave',{bubbles:false});r.dispatchEvent(e);return {ok:true,kind:'synthetic-dom-mouseleave',isTrusted:e.isTrusted};})()"#).await?;
    if closed["ok"] != true {
        return Err("Original profile close was not exposed".into());
    }
    for _ in 0..100 {
        if state.peek().profile_user.is_none() && state.peek().profile_hover.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let before = evidence()["ticks"].as_array().unwrap().len();
    advance(1735801260);
    settled_ticks(*state, 1735801260, before).await?;
    stages.push(stage(*state, "closed", 7, closed).await?);
    let before = evidence()["ticks"].as_array().unwrap().len();
    {
        let mut s = state.write();
        s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
        s.core.active_account = Some("profile-clock-replacement".into());
        let w = s
            .core
            .workspaces
            .get_mut("T1")
            .ok_or("Missing replacement workspace")?;
        let mut u = w
            .users
            .get("U2")
            .ok_or("Missing replacement member")?
            .clone();
        u.tz_offset = Some(3600);
        w.apply_user_update(u);
        if std::env::var("SUPER_PLATINUM_FIXTURE")
            .unwrap()
            .starts_with("profile-hover")
        {
            s.show_profile_hover("U2".into(), 610.0, 120.0);
        } else {
            s.profile_user = Some("U2".into());
            s.core.profile_pane = Some(super_platinum_core::domain::ProfilePaneState {
                user: "U2".into(),
                loading: false,
                error: None,
            });
        }
    }
    settled_ticks(*state, 1735801260, before).await?;
    stages.push(
        stage(
            *state,
            "replacement",
            8,
            json!({"kind":"synthetic-owner-replacement","user":"U2"}),
        )
        .await?,
    );
    let native = observe(*state, false).await?;
    Ok(
        json!({"ok":true,"noSession":true,"noTransport":true,"stagesCount":stages.len(),"stages":stages,
        "model":model(&state.peek()),"clock":evidence(),"native":native,"layout":{"ok":native["ok"],"viewport":native["viewport"]}}),
    )
}
