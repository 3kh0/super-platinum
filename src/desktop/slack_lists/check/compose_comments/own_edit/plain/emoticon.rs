//! Actual original key handling; synthetic default Space is recorded explicitly.
use super::*;

async fn event(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "({})({},{})",
            include_str!("emoticon.js"),
            json!(action),
            arg
        ),
    )
    .await
}
async fn key(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    ops: Value,
) -> Result<Value, String> {
    let v = event(d, "key", json!({"key":name})).await?;
    if v["after"]["ops"] != ops {
        return Err(format!("Original {name} Delta differed: {v}"));
    }
    wait(state, m, |v| {
        v["edit"]["active"]["plainDelta"]["ops"] == ops && v["capture"]["canSave"] == true
    })
    .await?;
    durable(state, &ops).await?;
    Ok(v)
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    fixture::completion::seed(state).await?;
    let seed_source = data::seed(state)?;
    checked(d, include_str!("../../../comments.js")).await?;
    checked(d, include_str!("../../../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(d, "Independent reply A 中🙂").await?;
    settled(d).await?;
    let remembered = command(d, "remember", Value::Null).await?;
    let disk = wire::Disk::capture(&state.peek())?;
    let enabled = source::emoticon::enabled();
    let mut p = json!({"source":seed_source,"replySeed":seed,"replyBlocks":remembered["blocks"],"opens":[],"inputs":[],"actions":[],"storageControls":[],"originalDisk":disk.snapshot().await?,"emoticonsEnabled":enabled});
    let mut rows = Vec::new();
    take(state, &m, d, "draft", &p, &mut rows, false).await?;
    open_plain(state, &m, d, &mut p).await?;
    replace(state, &m, d, &mut p, "Happy :)").await?;
    take(state, &m, d, "editing", &p, &mut rows, false).await?;
    let began = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    p["historyWaitMillis"] = json!(began.elapsed().as_millis() as u64);
    let semantic = json!([{"insert":"Happy "},{"insert":{"slackemoji":{"text":":slightly_smiling_face:"}}},{"insert":" \n"}]);
    p["space"] = key(
        state,
        &m,
        d,
        "Space",
        if enabled {
            semantic.clone()
        } else {
            json!([{"insert":"Happy :) \n"}])
        },
    )
    .await?;
    take(
        state,
        &m,
        d,
        if enabled { "space" } else { "literal" },
        &p,
        &mut rows,
        false,
    )
    .await?;
    if enabled {
        p["undo"] = key(state, &m, d, "Undo", json!([{"insert":"Happy :)\n"}])).await?;
        take(state, &m, d, "undo", &p, &mut rows, false).await?;
        p["redo"] = key(state, &m, d, "Redo", semantic).await?;
        take(state, &m, d, "redo", &p, &mut rows, false).await?;
        let mut guards = Vec::new();
        for (name, text) in [("code", "`Happy :)"), ("url", "https://example.test/:)")] {
            replace(state, &m, d, &mut p, text).await?;
            guards.push(json!({"kind":name,"key":key(state,&m,d,"Space",json!([{"insert":format!("{text} \n")}])).await?}));
        }
        replace(state, &m, d, &mut p, "Happy :)").await?;
        let selected = event(d, "select", json!({"start":6,"end":8})).await?;
        guards.push(json!({"kind":"selection","selection":selected,"key":key(state,&m,d,"Space",json!([{"insert":"Happy  \n"}])).await?}));
        replace(state, &m, d, &mut p, "Happy :)").await?;
        let start = event(d, "composition", json!({"active":true})).await?;
        let observed = event(d, "key", json!({"key":"Space"})).await?;
        let end = event(d, "composition", json!({"active":false})).await?;
        if start["after"]["composing"] != true
            || observed["after"]["ops"] != json!([{"insert":"Happy :)\n"}])
            || end["after"]["composing"] != false
        {
            return Err("Original composition guard changed content or phase".into());
        }
        guards.push(json!({"kind":"composition","start":start,"key":observed,"end":end}));
        p["guards"] = json!(guards);
        wait(state, &m, |v| {
            v["capture"]["canSave"] == true && v["edit"]["active"]["composing"] == false
        })
        .await?;
        // Model completion can precede the DOM render of the composition-end
        // controls. Capture only the same original editor's settled paint.
        wait_dom(d,"(()=>{const e=document.getElementById('slack-list-comment-edit-draft'),i=e?.querySelector('.ql-editor');return {ok:!!e&&e===window.__ownEditOriginalEditor&&i===window.__ownEditOriginalInput&&i?.isContentEditable===true&&!e.closest('[inert],[hidden]')&&document.getElementById('slack-list-comment-edit-save')?.disabled===false&&document.getElementById('slack-list-comment-edit-cancel')?.disabled===false};})()").await?;
        take(state, &m, d, "guards", &p, &mut rows, false).await?;
    }
    p["cancel"] = button(d, "slack-list-comment-edit-cancel").await?;
    closed(state, &m, d).await?;
    take(state, &m, d, "cancelled", &p, &mut rows, false).await?;
    if enabled {
        open_plain(state, &m, d, &mut p).await?;
        replace(state, &m, d, &mut p, "Enter :)").await?;
        data::mode(&state.peek(), data::Mode::HeldSuccess)?;
        p["enter"] = event(d, "key", json!({"key":"Enter"})).await?;
        wait_fixture(state, |v| v["held"] == true).await?;
        p["doubleSave"] = command(d, "double-save", Value::Null).await?;
        take(state, &m, d, "saving", &p, &mut rows, false).await?;
        p["release"] = data::release(&state.peek())?;
        closed(state, &m, d).await?;
        take(state, &m, d, "saved", &p, &mut rows, false).await?;
        wire::publish(&state.peek())?;
        p["actions"]
            .as_array_mut()
            .unwrap()
            .push(button(d, "slack-list-comments-refresh").await?);
        for _ in 0..400 {
            if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
                == wire::future_message()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        if wire::snapshot(&state.peek(), &m)["entryPage"]["raw"]["messages"][1]
            != wire::future_message()
        {
            return Err("Original emoticon readback missing".into());
        }
        wait_dom(
            d,
            "({ok:document.getElementById('slack-list-comments-refresh')?.disabled===false})",
        )
        .await?;
        take(state, &m, d, "readback", &p, &mut rows, false).await?;
    }
    p["closedDisk"] = disk.snapshot().await?;
    Ok(
        json!({"ok":true,"case":if enabled{"own-edit-plain-emoticon"}else{"own-edit-plain-emoticon-off"},"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"proof":p,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,"limits":["Original synthetic keydown and DOM selection; refused Space default is explicit insertText, not OS keyboard automation.","Composition guard observes synthetic composition events, not full IME behavior.","Normal 1000ms history grouping is retained; no history cutoff or semantic Delta injection."]}),
    )
}
