//! Native Reading controls with an isolated backend and owned capture handshake.
use super::*;
use serde_json::{Value, json};
use std::time::Duration;
pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::requested() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let receipt =
            match tokio::time::timeout(Duration::from_secs(120), run(state, &desktop)).await {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => json!({"ok":false,"error":e}),
                Err(_) => json!({"ok":false,"error":"Reading preference check timed out"}),
            };
        eprintln!("read-preferences-check: {receipt}");
    });
}
async fn eval(script: String) -> Result<Value, String> {
    let mut e = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(4), e.recv::<Value>()).await;
    let _ = e.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Reading DOM: {v}")),
        _ => Err("Reading DOM timed out".into()),
    }
}
async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
async fn click(id: &str) -> Result<(), String> {
    eval(format!(r#"const e=document.getElementById({});if(e){{e.focus({{preventScroll:true}});e.click();}}dioxus.send({{ok:!!e&&!e.matches(':disabled')}});await dioxus.recv();"#, json!(id))).await.map(|_|())
}
async fn wait(
    state: Signal<ShellState>,
    predicate: impl Fn(&ShellState) -> bool,
) -> Result<(), String> {
    for _ in 0..200 {
        if predicate(&state.peek()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Reading state did not settle".into())
}
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn writes(s: &ShellState) -> usize {
    s.read_settings.fixture.as_ref().unwrap().attempts.len()
}
fn shown(s: &ShellState) -> Option<Presentation> {
    let visit = s.read_settings.view.as_ref()?.visit;
    presentation(s, visit)
}
async fn ready() -> Result<(std::path::PathBuf, String, String), String> {
    let (dir, nonce) = fixture::directory()?;
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing alias")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":name});
    let path = dir.join("ready.json");
    if path.exists() {
        return Err("Readiness marker must be new".into());
    }
    eprintln!("read-preferences-waiting: {expected}");
    for _ in 0..300 {
        let check = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            let meta = match std::fs::symlink_metadata(&check) {
                Ok(meta) => meta,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err("Cannot inspect readiness".into()),
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 1024 {
                return Err("Invalid readiness file".into());
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("Readiness reparse point refused".into());
                }
            }
            serde_json::from_slice(&std::fs::read(check).map_err(|_| "Cannot read readiness")?)
                .map(Some)
                .map_err(|_| "Invalid readiness JSON".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            if found != expected {
                return Err("Readiness owner mismatch".into());
            }
            return Ok((dir, nonce, name));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}
async fn capture(
    d: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    stage: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    // The state receipt can settle before the component's queued focus effect.
    // Wait for the actual DOM condition rather than racing two independent evals.
    let expected_focus = match stage {
        "newest-read" => Some("reading-preference-newest-read"),
        "mismatch" | "confirmed" | "unsupported" | "restored" => Some("reading-preferences-reload"),
        _ => None,
    };
    if let Some(expected) = expected_focus {
        eval(format!(r#"const id={};let count=0;function check(){{const e=document.getElementById(id);if(e===document.activeElement&&e.matches(':focus-visible')){{dioxus.send({{ok:true}});return;}}if(++count>=120){{dioxus.send({{ok:false,expected:id,actual:document.activeElement?.id||null}});return;}}requestAnimationFrame(check);}}check();await dioxus.recv();"#,json!(expected))).await?;
    }
    let mut measured = eval(r#"const rect=e=>{const r=e.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height}};const dialog=document.querySelector('.modal:has(#reading-preferences)');const targets=[...document.querySelectorAll('#reading-preferences input'),document.querySelector('.modal header button'),document.getElementById('reading-preferences-reload')];const controls=targets.filter(Boolean).map((e,i)=>{const r=rect(e);return {name:e.id||'close',rect:r,hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)),disabled:e.matches(':disabled'),checked:e.checked??null}});const panel=document.getElementById('reading-preferences'),active=document.activeElement,focused=panel?.contains(active)&&active.matches(':focus-visible');let focusRingContained=true,focusRing=null;if(focused){const a=active.getBoundingClientRect(),p=panel.getBoundingClientRect(),cs=getComputedStyle(active),outset=Math.max(0,parseFloat(cs.outlineWidth)+parseFloat(cs.outlineOffset));focusRing={x:a.x-outset,y:a.y-outset,width:a.width+2*outset,height:a.height+2*outset};focusRingContained=focusRing.x>=p.x-.5&&focusRing.y>=p.y-.5&&focusRing.x+focusRing.width<=p.right+.5&&focusRing.y+focusRing.height<=p.bottom+.5;}dioxus.send({ok:!!dialog&&controls.length===5&&controls.every(e=>e.hit&&e.rect.y>=0&&e.rect.y+e.rect.height<=innerHeight)&&document.documentElement.scrollWidth<=innerWidth+1&&focusRingContained,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth,dialog:dialog?rect(dialog):null,panel:panel?rect(panel):null,controls,focusRingContained,focusRing,focusedControl:focused?active.id:null});await dioxus.recv();"#.into()).await?;
    let selected: Vec<_> = measured["controls"]
        .as_array()
        .ok_or("Missing radio measurements")?
        .iter()
        .filter(|c| c["checked"] == true)
        .filter_map(|c| c["name"].as_str())
        .collect();
    let expected: &[&str] = match stage {
        "initial" | "saving" | "restored" => &["reading-preference-left-off"],
        "newest-read" | "uncertain" | "mismatch" => &["reading-preference-newest-read"],
        "confirmed" => &["reading-preference-newest-unseen"],
        "cooldown" | "rotation" | "unsupported" => &[],
        _ => return Err("Unknown reading capture stage".into()),
    };
    ensure(
        selected == expected,
        "Visible radio selection was not the canonical preference",
    )?;
    if let Some(expected) = expected_focus {
        ensure(
            measured["focusedControl"] == expected,
            "Reading control focus was not preserved",
        )?;
    }
    measured["stage"] = json!(stage);
    geometry.push(measured);
    eprintln!(
        "read-preferences-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) = crate::agent::capture::window(
        Some(
            dir.join(format!("{stage}.png"))
                .to_string_lossy()
                .into_owned(),
        ),
        d,
    )
    .await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "read-preferences-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
async fn run(
    mut s: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, name) = ready().await?;
    wait(s, |s| {
        shown(s).is_some_and(|p| p.can_change && p.mode == Some(ReadMode::LeftOffAndRead))
    })
    .await?;
    let before = serde_json::to_value(&s.peek().core.workspaces["T1"].notifications)
        .map_err(|_| "Cannot capture unrelated preferences")?;
    let mut captures = serde_json::Map::new();
    let mut geometry = Vec::new();
    ensure(
        fixture::enabled(&s.peek()) && writes(&s.peek()) == 0,
        "Connected fixture or opening write",
    )?;
    capture(d, &dir, "initial", &mut captures, &mut geometry).await?;
    click("reading-preference-newest-read").await?;
    wait(s, |s| writes(s) == 1 && shown(s).is_some_and(|p| p.busy)).await?;
    capture(d, &dir, "saving", &mut captures, &mut geometry).await?;
    eval("document.getElementById('reading-preference-newest-read').dispatchEvent(new Event('change',{bubbles:true}));dioxus.send({ok:true});await dioxus.recv();".into()).await?;
    ensure(writes(&s.peek()) == 1, "Duplicate setting dispatched")?;
    fixture::complete(&mut s.write(), true, Ok(()));
    wait(s, |s| {
        shown(s).is_some_and(|p| p.can_change && p.mode == Some(ReadMode::NewestAndRead))
    })
    .await?;
    capture(d, &dir, "newest-read", &mut captures, &mut geometry).await?;
    click("reading-preference-newest-unseen").await?;
    wait(s, |s| writes(s) == 2).await?;
    fixture::complete(
        &mut s.write(),
        false,
        Err(Error::Transport("synthetic lost response".into())),
    );
    wait(s, |s| shown(s).is_some_and(|p| p.uncertain && !p.busy)).await?;
    capture(d, &dir, "uncertain", &mut captures, &mut geometry).await?;
    // The first explicit read disagrees and cannot unlock or replay the write.
    click("reading-preferences-reload").await?;
    wait(s, |s| {
        s.read_settings.fixture.as_ref().unwrap().reads.len() == 5
    })
    .await?;
    ensure(
        shown(&s.peek()).is_some_and(|p| p.uncertain && !p.can_change) && writes(&s.peek()) == 2,
        "Mismatch released or replayed",
    )?;
    capture(d, &dir, "mismatch", &mut captures, &mut geometry).await?;
    fixture::apply_unconfirmed(&mut s.write());
    click("reading-preferences-reload").await?;
    wait(s, |s| {
        shown(s).is_some_and(|p| p.can_change && p.mode == Some(ReadMode::NewestKeepUnseen))
    })
    .await?;
    capture(d, &dir, "confirmed", &mut captures, &mut geometry).await?;
    click("reading-preference-left-off").await?;
    wait(s, |s| writes(s) == 3).await?;
    fixture::complete(
        &mut s.write(),
        false,
        Err(Error::RateLimited {
            retry_after_secs: Some(2),
        }),
    );
    ensure(
        cooling(&s.peek()) && shown(&s.peek()).is_some_and(|p| !p.can_read && !p.can_change),
        "429 did not hold controls",
    )?;
    capture(d, &dir, "cooldown", &mut captures, &mut geometry).await?;
    // This only waits for the synthetic Retry-After; it performs no retry.
    for _ in 0..100 {
        if !cooling(&s.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    ensure(
        !cooling(&s.peek()) && writes(&s.peek()) == 3,
        "Cooldown replayed or failed to expire",
    )?;
    // Request a rerender without advancing or changing any preference.
    {
        let _shell = s.write();
    }
    frames().await?;
    click("reading-preferences-reload").await?;
    wait(s, |s| shown(s).is_some_and(|p| p.can_change)).await?;
    click("reading-preference-left-off").await?;
    wait(s, |s| writes(s) == 4).await?;
    let previous = s.peek().core.workspaces["T1"].read_preferences.clone();
    s.write().core.account_epoch += 1;
    fixture::complete(&mut s.write(), true, Ok(()));
    ensure(
        s.peek().core.workspaces["T1"].read_preferences == previous,
        "Rotated owner adopted late write",
    )?;
    wait(s, |s| shown(s).is_some_and(|p| p.uncertain && p.can_read)).await?;
    capture(d, &dir, "rotation", &mut captures, &mut geometry).await?;
    click("reading-preferences-reload").await?;
    wait(s, |s| {
        shown(s).is_some_and(|p| p.can_change && p.mode == Some(ReadMode::LeftOffAndRead))
    })
    .await?;
    s.write()
        .read_settings
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .insert("mark_msgs_read_immediately".into(), json!("unsupported"));
    click("reading-preferences-reload").await?;
    wait(s, |s| {
        shown(s).is_some_and(|p| !p.can_change && !p.busy && p.mode.is_none())
    })
    .await?;
    ensure(
        s.peek().core.workspaces["T1"]
            .read_preferences
            .mode()
            .is_err(),
        "Malformed preference retained old policy",
    )?;
    capture(d, &dir, "unsupported", &mut captures, &mut geometry).await?;
    s.write()
        .read_settings
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .extend(ReadMode::LeftOffAndRead.patch());
    click("reading-preferences-reload").await?;
    wait(s, |s| {
        shown(s).is_some_and(|p| p.can_change && p.mode == Some(ReadMode::LeftOffAndRead))
    })
    .await?;
    capture(d, &dir, "restored", &mut captures, &mut geometry).await?;
    let shell = s.peek();
    let f = shell.read_settings.fixture.as_ref().unwrap();
    ensure(
        f.valid
            && f.attempts.len() == 4
            && f.reads.len() == 12
            && captures.len() == fixture::STAGES.len(),
        "Request or capture count mismatch",
    )?;
    ensure(
        serde_json::to_value(&shell.core.workspaces["T1"].notifications)
            .map_err(|_| "Cannot compare unrelated preferences")?
            == before
            && f.raw["unknown_pref"] == json!({"keep":[1,2]})
            && fixture::enabled(&shell),
        "Unrelated preferences changed or isolation lost",
    )?;
    Ok(
        json!({"ok":true,"nonce":nonce,"pid":std::process::id(),"fixture":name,"noSession":true,"noTransport":true,"syntheticInput":true,"actualPreparedRequests":true,"viewport":geometry[0]["viewport"],"captures":captures,"geometry":geometry,"attempts":f.attempts,"readRequests":f.reads,"preparedRequests":16,"finalRaw":f.raw,"checks":{"openNoWrite":true,"duplicateBlocked":true,"unknownReadOnlyCheck":true,"mismatchRetained":true,"exactReadbackConfirmed":true,"cooldownNoReplay":true,"lateOwnerRefused":true,"malformedRevoked":true,"unknownPreserved":true,"unrelatedPreferencesPreserved":true}}),
    )
}
