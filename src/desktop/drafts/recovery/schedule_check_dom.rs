//! Renderer-owned schedule recovery fixture evidence.
use super::*;
pub(super) async fn composer(target: ComposerTarget) -> Result<Value, String> {
    eval(format!(r#"
const field=document.getElementById({}),wrap=field?.closest('.thread-composer,.composer-wrap');
const pane=field?.closest('.thread-pane,.conversation'),header=pane?.querySelector(':scope > header');
const rect=e=>{{if(!e)return null;const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}};
const inside=r=>r&&r.width>0&&r.height>0&&r.x>=-1&&r.y>=-1&&r.x+r.width<=innerWidth+1&&r.y+r.height<=innerHeight+1;
const clear=(e,r)=>!!e&&inside(r)&&[.05,.5,.95].every(x=>[.05,.5,.95].every(y=>e.contains(document.elementFromPoint(r.x+r.width*x,r.y+r.height*y))));
const f=rect(field),w=rect(wrap),h=rect(header),fieldUnobscured=clear(field,f),composerUnobscured=clear(wrap,w),fieldFocused=document.activeElement===field,headerUnobscured=clear(header,h);
dioxus.send({{ok:fieldUnobscured&&composerUnobscured&&fieldFocused&&headerUnobscured,kind:'composer',field:f,composer:w,header:h,headerUnobscured,fieldUnobscured,composerUnobscured,fieldFocused,activeId:document.activeElement?.id,scope:field?.dataset.draftScope,visit:field?.dataset.projectionVisit,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}});await dioxus.recv();
"#,json!(format!("{}-composer",label(target))))).await
}

// Scroll only the inventory body, never a header or an outer shell ancestor.
pub(super) async fn surface(mode: &str, needle: &str, click: bool) -> Result<Value, String> {
    eval(format!(r#"
const mode={},needle={},click={};
const body=document.querySelector('.scheduled-body'),panel=document.querySelector('.scheduled-panel'),header=panel?.querySelector('.list-panel-header');
const tabs=[...document.querySelectorAll('.later-tabs [role=tab]')],tab=tabs.find(e=>e.textContent.trim().startsWith('Scheduled'));
const rows=[...body?.querySelectorAll('article.scheduled-item')||[]];
const emptyMessage=body?.querySelector('.scheduled-empty');
const row=mode==='tab'?tab:mode==='scheduled'?(rows[0]||emptyMessage||body):rows.find(e=>(mode!=='local'||e.dataset.managedDraftRecovery==='local')&&e.innerText.includes(needle));
const contentKind=mode==='tab'?'tab':row===body?'empty-body':row===emptyMessage?'empty-message':'row';
if(row&&body&&row!==body&&mode!=='tab'){{const a=row.getBoundingClientRect(),p=body.getBoundingClientRect();body.scrollTop+=a.top-p.top-(p.height-a.height)/2;}}
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const rect=e=>{{if(!e)return null;const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}};
const a=rect(row),p=rect(body),h=rect(header),t=rect(tab);
const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1&&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
const clear=(e,r)=>!!e&&!!r&&[.05,.5,.95].every(x=>[.05,.5,.95].every(y=>e.contains(document.elementFromPoint(r.x+r.width*x,r.y+r.height*y))));
const viewport={{x:0,y:0,width:innerWidth,height:innerHeight}},rowUnobscured=clear(row,a),headerUnobscured=clear(header,h),threadPaneAbsent=!document.querySelector('.thread-pane');
const selectedBefore=tab?.getAttribute('aria-selected'),checkCount=row?.querySelectorAll('[data-draft-operation]').length??0;
const emptyBodyValid=contentKind!=='empty-body'||(rows.length===0&&!emptyMessage&&body?.innerText.trim()==='');
const ok=emptyBodyValid&&threadPaneAbsent&&inside(a,mode==='tab'?viewport:p)&&inside(h,viewport)&&rowUnobscured&&headerUnobscured&&(!click||mode==='tab');
let clicked=false;if(ok&&click&&!tab.disabled){{tab.focus({{preventScroll:true}});tab.click();clicked=true;}}
dioxus.send({{ok,kind:mode,contentKind,row:a,panel:p,header:h,tab:t,rowUnobscured,headerUnobscured,threadPaneAbsent,text:row?.innerText,actions:[...row?.querySelectorAll('button')||[]].map(e=>e.textContent.trim()),checkCount,buttonText:tab?.textContent,selectedBefore,clickRequested:click,clicked,rowCount:rows.length,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}});await dioxus.recv();
"#,json!(mode),json!(needle),json!(click))).await
}
pub(super) fn label(target: ComposerTarget) -> &'static str {
    if matches!(target, ComposerTarget::Thread) {
        "thread"
    } else {
        "channel"
    }
}
pub(super) fn ensure(value: bool, reason: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(reason.into()) }
}
pub(super) async fn eval(script: String) -> Result<Value, String> {
    let mut e = document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), e.recv::<Value>()).await;
    let _ = e.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Schedule DOM: {v}")),
        _ => Err("Schedule DOM timeout".into()),
    }
}
pub(super) async fn frames() -> Result<(), String> {
    eval("requestAnimationFrame(()=>requestAnimationFrame(()=>dioxus.send({ok:true})));await dioxus.recv();".into()).await.map(|_|())
}
pub(super) async fn native(
    target: ComposerTarget,
    operation: &str,
    text: Option<&str>,
    input: bool,
) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await draftProjectionCheck({},{}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("../projection_check.js"),json!(operation),json!({"id":format!("{}-composer",label(target)),"text":text,"input":input}))).await
}
async fn anchor_measure(expected: &Value) -> Result<Value, String> {
    eval(format!(
        "{};dioxus.send({{ok:true,anchor:scheduleHomeAnchor({})}});await dioxus.recv();",
        include_str!("schedule_check_home.js"),
        expected
    ))
    .await
    .map(|v| v["anchor"].clone())
}
fn same_timeline(state: Signal<ShellState>, expected: &Value) -> bool {
    let shell = state.peek();
    json!(shell.timeline_scope()) == expected["scope"]
        && json!(shell.channel_rows.len()) == expected["windowEnd"]
        && json!(shell.channel_rows.last().map(|m| &m.id)) == expected["latestId"]
}
pub(super) async fn home(
    mut state: Signal<ShellState>,
    target: ComposerTarget,
) -> Result<Value, String> {
    // Real component unmount/remount restores stored text. Never sets DOM text.
    {
        let mut s = state.write();
        s.main_view = MainView::Unreads;
        s.thread_root = None;
    }
    let mut removed = false;
    for _ in 0..100 {
        if native(target, "absent", None, false).await.is_ok() {
            removed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(removed, "Composer did not unmount")?;
    use super_platinum_core::domain::PendingScrollTarget;
    let expected = {
        let mut s = state.write();
        s.main_view = MainView::Home;
        s.thread_root = matches!(target, ComposerTarget::Thread).then(|| fixture::ROOT.into());
        s.sync_composer_drafts();
        if s.thread_root.is_some() {
            s.refresh_thread_from_core();
        }
        // Publish the intended virtual window together with the Home mount.
        // A mounted composer alone does not prove that its timeline tail exists.
        let id = s.core.active_channel.clone().ok_or("Channel missing")?;
        s.core.pending_scroll_to = Some((id, PendingScrollTarget::Latest));
        s.stick_to_bottom = true;
        s.reset_timeline_window();
        json!({"scope":s.timeline_scope(),"latestId":s.channel_rows.last().map(|m|&m.id),"windowEnd":s.channel_rows.len()})
    };
    let scope = crate::view::composer::field_scope(&state.peek(), target);
    let mut last = Value::Null;
    for _ in 0..150 {
        ensure(
            same_timeline(state, &expected),
            "Home timeline identity changed while mounting",
        )?;
        let field = native(target, "read", None, false).await.ok();
        last = anchor_measure(&expected).await?;
        if let Some(field) = field
            && field["scope"] == scope
            && field["visit"].as_str().is_some_and(|s| !s.is_empty())
            && last["ready"] == true
        {
            let before = last;
            ensure(
                same_timeline(state, &expected),
                "Home timeline changed before navigation",
            )?;
            let landed =
                crate::bootstrap::scroll_to_pending(state, PendingScrollTarget::Latest).await;
            frames().await?;
            let mut value = native(target, "show", None, false).await?;
            let after = anchor_measure(&expected).await?;
            if !landed
                || !same_timeline(state, &expected)
                || after["atBottom"] != true
                || before["inputGeneration"] != after["inputGeneration"]
                || field["scope"] != value["scope"]
                || field["visit"] != value["visit"]
            {
                return Err(format!(
                    "Timeline anchor did not settle: {}",
                    json!({"landed":landed,"before":before,"after":after,"sameTimeline":same_timeline(state,&expected)})
                ));
            }
            value["homeAnchor"] = json!({"landed":landed,"before":before,"after":after});
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "Restored composer/timeline mount unavailable: {last}"
    ))
}
pub(super) async fn drafts(mut s: Signal<ShellState>, op: &Operation) -> Result<Value, String> {
    {
        let mut shell = s.write();
        // Match the UI close flow: checkpoint both composers before removing
        // the thread overlay, keeping its draft available for home(Thread).
        shell.close_thread();
        shell.main_view = MainView::Scheduled;
        shell.core.scheduled.filter = super_platinum_core::scheduled::DraftFilter::Drafts;
    }
    let mut closed = false;
    for _ in 0..150 {
        if eval(
            "dioxus.send({ok:!document.querySelector('.thread-pane')});await dioxus.recv();".into(),
        )
        .await
        .is_ok()
        {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(closed, "Thread pane did not close before Drafts review")?;
    for _ in 0..150 {
        if let Ok(v) = row(op, false).await {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Schedule recovery row unavailable".into())
}
pub(super) async fn row(op: &Operation, click: bool) -> Result<Value, String> {
    eval(format!(r#"
const host=[...document.querySelectorAll('[data-draft-operation]')].find(e=>e.dataset.draftOperation==={});
const article=host?.closest('article'),button=host?.querySelector('button'),body=article?.closest('.scheduled-body'),header=body?.closest('.scheduled-panel')?.querySelector('.list-panel-header');
if(article&&body){{const a=article.getBoundingClientRect(),p=body.getBoundingClientRect();body.scrollTop+=a.top-p.top-(p.height-a.height)/2;}}
await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
const rect=e=>{{const r=e.getBoundingClientRect();return {{x:r.x,y:r.y,width:r.width,height:r.height}}}};
const a=article?rect(article):null,b=button?rect(button):null,p=body?rect(body):null,h=header?rect(header):null;
const inside=(r,p)=>!!r&&!!p&&r.width>0&&r.height>0&&r.x>=Math.max(0,p.x)-1&&r.y>=Math.max(0,p.y)-1&&r.x+r.width<=Math.min(innerWidth,p.x+p.width)+1&&r.y+r.height<=Math.min(innerHeight,p.y+p.height)+1;
// Sample the full row and action, not only a button centre that may remain
// exposed while a thread overlay hides the surrounding explanation.
const unobscured=(e,r)=>!!e&&!!r&&[0.05,0.5,0.95].every(x=>[0.05,0.5,0.95].every(y=>e.contains(document.elementFromPoint(r.x+r.width*x,r.y+r.height*y))));
const threadPaneAbsent=!document.querySelector('.thread-pane');
const rowUnobscured=unobscured(article,a),buttonUnobscured=unobscured(button,b),headerUnobscured=unobscured(header,h);
const hit=!!button&&button.contains(document.elementFromPoint(b.x+b.width/2,b.y+b.height/2));
const ok=!!host&&host.dataset.draftOperationKind==='schedule'&&threadPaneAbsent&&inside(h,{{x:0,y:0,width:innerWidth,height:innerHeight}})&&headerUnobscured&&inside(a,p)&&inside(b,p)&&rowUnobscured&&buttonUnobscured&&hit;
const result={{ok,kind:host?.dataset.draftOperationKind,operation:host?.dataset.draftOperation,text:article?.innerText,buttonText:button?.textContent,clickRequested:{},clicked:{}&&ok&&!button?.disabled,disabled:button?.disabled,hit,threadPaneAbsent,rowUnobscured,buttonUnobscured,row:a,button:b,panel:p,header:h,headerUnobscured,viewport:[innerWidth,innerHeight],documentWidth:document.documentElement.scrollWidth}};
if({}&&ok&&!button.disabled){{button.focus({{preventScroll:true}});button.click();}}
dioxus.send(result);await dioxus.recv();
"#,json!(op.id),json!(click),json!(click),json!(click))).await
}
pub(super) async fn flush(editor: &Arc<checkpoint::Editor>) -> Result<(), String> {
    let _ = editor.flush().await;
    for _ in 0..250 {
        if !editor.snapshot().running {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Schedule editor did not settle".into())
}
pub(super) async fn type_newer(
    state: Signal<ShellState>,
    b: &fixture::FixtureBinding,
    target: ComposerTarget,
) -> Result<Value, String> {
    let routed = b.routed.load(Ordering::Relaxed);
    native(target, "selection", None, false).await?;
    native(target, "set", Some(fixture::NEWER), true).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Relaxed) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Relaxed) > routed,
        "Native input never reached owned writer",
    )?;
    flush(&b.editors[&fixture::key(target)]).await?;
    let saved = fixture::durable(b).await?;
    ensure(
        saved[label(target)]["canonical"]["composer"]["text"] == fixture::NEWER,
        "Newer input was not durably paired",
    )?;
    wait_input_publication(state, b, target, &saved).await?;
    native(target, "read", None, false).await
}

// Editor.flush proves paired storage, not publication into the serial shell.
// The normal runtime tick owns that publication. Observe it without calling
// integration::poll or assigning a managed projection from this fixture.
async fn wait_input_publication(
    state: Signal<ShellState>,
    b: &fixture::FixtureBinding,
    target: ComposerTarget,
    saved: &Value,
) -> Result<(), String> {
    let key = fixture::key(target);
    let captured = b.editors.get(&key).ok_or("Captured input editor missing")?;
    let serial = captured.snapshot().serial;
    let expected = &saved[label(target)];
    ensure(
        expected["scope"] == json!(captured.owner().scope)
            && expected["canonical"] == json!(fixture::local(fixture::NEWER, target))
            && expected["blocked"].is_null()
            && expected["alternates"] == json!([]),
        "Input publication requires exact owned durable newer text",
    )?;
    for _ in 0..150 {
        {
            let shell = state.peek();
            ensure(
                b.current(&shell)
                    && shell.composer_key(target).as_ref() == Some(&key)
                    && shell
                        .draft_writes
                        .writers
                        .get(fixture::ACCOUNT, b.epoch, &key)
                        .is_some_and(|current| Arc::ptr_eq(&current, captured)),
                "Input publication owner, scope, or captured writer changed",
            )?;
            let snapshot = captured.snapshot();
            ensure(
                snapshot.serial == serial,
                "New input arrived while awaiting publication",
            )?;
            if snapshot.error.is_none()
                && publication_ready(
                    &fixture::projection(&shell, b),
                    label(target),
                    expected,
                    serial,
                )
            {
                return Ok(());
            }
        }
        // Yield to the unchanged runtime tick and asynchronous writer. This is
        // a bounded readiness barrier, not an added projection or persistence.
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Settled input was not published to the managed shell view".into())
}

fn publication_ready(projection: &Value, target: &str, durable: &Value, serial: u64) -> bool {
    let view = json!({"local":durable["canonical"],"stored":durable["stored"],
        "blocked":null,"uncommitted":null});
    projection["managed"][target] == view
        && projection["editors"][target]
            == json!({"sameCaptured":true,"serial":serial,
            "view":view,"pending":null,"running":false,"unsaved":false})
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    fn sample() -> (Value, Value) {
        let durable = json!({"canonical":fixture::local(fixture::NEWER,ComposerTarget::Thread),
            "stored":{"revision":3,"state":{"identity":"same retained intent"}}});
        let view = json!({"local":durable["canonical"],"stored":durable["stored"],
            "blocked":null,"uncommitted":null});
        let projection = json!({"managed":{"thread":view},"editors":{"thread":{
            "sameCaptured":true,"serial":1,"view":view,"pending":null,"running":false,"unsaved":false}}});
        (projection, durable)
    }

    #[test]
    fn actual_publication_requires_complete_settled_editor_and_managed_view() {
        let (projection, durable) = sample();
        assert!(publication_ready(&projection, "thread", &durable, 1));
        for (field, bad) in [
            ("sameCaptured", json!(false)),
            ("serial", json!(2)),
            ("pending", durable["canonical"].clone()),
            ("running", json!(true)),
            ("unsaved", json!(true)),
        ] {
            let mut changed = projection.clone();
            changed["editors"]["thread"][field] = bad;
            assert!(
                !publication_ready(&changed, "thread", &durable, 1),
                "{field}"
            );
        }
    }

    #[test]
    fn sr2_exact_newer_uncommitted_lag_is_not_a_settled_publication() {
        let (mut projection, durable) = sample();
        // Observed sr2 ordering: Editor is already durable revision 3, while
        // the earlier runtime tick published revision 1 plus pending NEWER.
        projection["managed"]["thread"]["stored"]["revision"] = json!(1);
        projection["managed"]["thread"]["local"] =
            json!(fixture::local(fixture::CURRENT, ComposerTarget::Thread));
        projection["managed"]["thread"]["uncommitted"] = durable["canonical"].clone();
        assert!(!publication_ready(&projection, "thread", &durable, 1));
        projection["managed"]["thread"] = projection["editors"]["thread"]["view"].clone();
        assert!(publication_ready(&projection, "thread", &durable, 1));
    }

    #[test]
    fn publication_requires_exact_content_selection_revision_and_null_authority() {
        let (projection, durable) = sample();
        for field in ["local", "stored", "blocked", "uncommitted"] {
            let mut changed = projection.clone();
            changed["managed"]["thread"][field] = json!({"unexpected":true});
            assert!(!publication_ready(&changed, "thread", &durable, 1));
        }
        let mut changed = projection.clone();
        changed["managed"]["thread"]["local"]["composer"]["selection"]["end"] = json!(0);
        assert!(!publication_ready(&changed, "thread", &durable, 1));
        let mut changed = projection.clone();
        changed["editors"]["thread"]["view"]["stored"]["revision"] = json!(4);
        assert!(!publication_ready(&changed, "thread", &durable, 1));
    }
}
pub(super) async fn capture(
    d: &dioxus::desktop::DesktopContext,
    b: &fixture::FixtureBinding,
    stage: &str,
    measurement: Value,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    frames().await?;
    let mut measurement = measurement;
    measurement["stage"] = json!(stage);
    geometry.push(measurement);
    let directory = std::env::var_os("SUPER_PLATINUM_DRAFT_SCHEDULE_RECOVERY_CAPTURE_DIR")
        .ok_or("Missing capture directory")?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let path = std::path::PathBuf::from(directory)
        .join(format!("{alias}-{}-{}-{stage}.png", b.scenario, b.phase));
    eprintln!(
        "draft-schedule-recovery-stage: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), d).await?;
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    eprintln!(
        "draft-schedule-recovery-stage: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
pub(super) fn native_valid(value: &Value, text: &str) -> bool {
    value["text"] == text
        && value["display"] == text
        && value["codePoints"] == json!(text.chars().map(|c| c as u32).collect::<Vec<_>>())
        && value["utf16Length"] == json!(text.encode_utf16().count())
}
