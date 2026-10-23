//! Closed original-caret List mixed group completion, through the actual editor UI.
use super::*;
use crate::slack_lists::comments::{completion as controller, fixture::completion as data};
mod custody;
mod drift;
mod restricted;
mod success;
pub(super) fn snapshot(state: Signal<ShellState>, model: &Model) -> Value {
    let s = state.peek();
    let mut v = compose::snapshot(&s, model);
    v["completionSource"] = data::snapshot(&s);
    v["completion"] = controller::snapshot(&s);
    if data::title_users::named() || data::empty_canvas::named() {
        v["titleUsers"] = crate::slack_lists::comments::title_users::snapshot(&s);
        v["titleAcquisition"] = crate::slack_lists::comments::file_acquisition::snapshot(&s);
    }
    if data::empty_canvas::named() {
        v["coldFiles"] = crate::slack_lists::comments::fixture::cold_files::snapshot(&s);
    }
    v
}
pub(super) async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    test: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..250 {
        let v = snapshot(state, m);
        if test(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "List completion did not settle: {}",
        snapshot(state, m)
    ))
}
fn searches(v: &Value) -> usize {
    v["completionSource"]["requests"].as_array().map_or(0, |r| {
        r.iter()
            .filter(|r| r["method"] == "usergroups/search")
            .count()
    })
}
fn search_ready(v: &Value, n: usize, completed: usize, cancelled: usize, held: usize) -> bool {
    searches(v) == n
        && v["completionSource"]["completed"] == completed
        && v["completionSource"]["cancelled"] == cancelled
        && v["completionSource"]["held"] == held
}
pub(super) async fn command(
    d: &dioxus::desktop::DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    checked(
        d,
        &format!(
            "({})({},{},({}))",
            include_str!("group_completion.js"),
            json!(action),
            arg,
            include_str!("reaction_actions/text.js")
        ),
    )
    .await
}
pub(super) fn capture_argument(model: &Value, name: &str, sending: bool) -> Value {
    let q = &model["completion"];
    let active = q["current"] == true && q["consumed"] == false;
    let status = match name {
        "query-retired" | "stale-refused" | "empty" => "No matching mentions",
        "query-replaced" | "late-query" if data::title_users::named() => "No matching mentions",
        "malformed" | "failure" => {
            "Mention suggestions could not load. Edit your query to try again."
        }
        _ if data::title_users::named()
            && active
            && q["revision"] == 0
            && q["labels"].as_array().is_some_and(Vec::is_empty) =>
        {
            "Searching mentions..."
        }
        _ => "",
    };
    json!({"sending":sending,"expectedCompletion":{
        "open":active,"sequence":q["sequence"],"revision":q["revision"],
        "labels":q["labels"],"status":status
    }})
}
pub(super) async fn geometry(
    d: &dioxus::desktop::DesktopContext,
    arg: &Value,
) -> Result<Value, String> {
    wait_dom(
        d,
        &format!(
            "({})(\"snapshot\",{},({}))",
            include_str!("group_completion.js"),
            arg,
            include_str!("reaction_actions/text.js")
        ),
    )
    .await
}
pub(super) async fn query(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    name: &str,
    term: &str,
    mode: data::Mode,
) -> Result<(), String> {
    data::mode(&state.peek(), mode).map_err(|e| e.to_string())?;
    wait_dom(d,"({ok:document.getElementById('slack-list-comment-draft')?.__listCommentCompletion?.current()===true})").await?;
    let previous = checked(d, "({ok:true,sequence:Number(document.querySelector('.slack-list-channel-completions')?.dataset.querySequence||0)})").await?["sequence"]
        .as_u64().ok_or("Missing previous completion sequence")?;
    let input = command(d, "query", json!(term)).await?;
    wait_dom(d, &format!("({{ok:Number(document.querySelector('.slack-list-channel-completions')?.dataset.querySequence||0)>{previous}}})")).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":name,"kind":"query","value":input}));
    stable_drafts(state, m).await?;
    Ok(())
}
pub(super) async fn current_blocks(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    Ok(checked(d,"(()=>{const e=document.getElementById('slack-list-comment-draft'),v=e&&window.superPlatinumRichEditor.snapshot(e);return {ok:!!v,blocks:v?.blocks};})()").await?["blocks"].clone())
}
pub(super) async fn choose(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    name: &str,
    label: &str,
    id: &str,
    keyboard: bool,
) -> Result<(), String> {
    let mut publication = Value::Null;
    for _ in 0..250 {
        let current = snapshot(state, m)["completion"].clone();
        if current["current"] == true && current["batchCurrent"] == true {
            let ready = command(d, "ready", current.clone()).await?;
            if ready["ready"] == true && snapshot(state, m)["completion"] == current {
                publication = current;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if publication.is_null() {
        return Err(format!(
            "Group selection publication did not settle: {}",
            snapshot(state, m)
        ));
    }
    let choices = publication["values"]
        .as_array()
        .cloned()
        .ok_or("Missing original member choices")?;
    let index = choices
        .iter()
        .position(|c| {
            ((c["value"]["type"] == "usergroup" && c["value"]["usergroup_id"] == id)
                || (c["value"]["type"] == "user" && c["value"]["user_id"] == id)
                || (matches!(c["value"]["type"].as_str(), Some("canvas" | "file"))
                    && c["value"]["file_id"] == id))
                && c["label"] == label
        })
        .ok_or("Original member choice missing")?;
    let value = command(
        d,
        "choose",
        json!({"label":label,"index":index,"keyboard":keyboard,
            "sequence":publication["sequence"],"revision":publication["revision"],"labels":publication["labels"]}),
    )
    .await?;
    wait_dom(d,&format!("(()=>{{const e=document.getElementById('slack-list-comment-draft'),v=e&&window.superPlatinumRichEditor.snapshot(e);return {{ok:!!v&&JSON.stringify(v.blocks).includes({})&&!document.querySelector('.slack-list-channel-option')}};}})()",json!(id))).await.map_err(|error| format!("{error}; original publication: {publication}; current model: {}", snapshot(state,m)))?;
    stable_drafts(state, m).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":name,"kind":"choose","selected":id,"values":choices,"publication":publication,"value":value}));
    p["expectedBlocks"] = current_blocks(d).await?;
    Ok(())
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    sending: bool,
) -> Result<(), String> {
    if !sending {
        stable_drafts(state, m).await?;
    }
    if sending != (name == "sending") {
        return Err("Wrong completion capture mode".into());
    }
    let before = snapshot(state, m);
    let arg = capture_argument(&before, name, sending);
    let g = geometry(d, &arg).await?;
    let mut proof = p.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if before != snapshot(state, m) {
        return Err(format!(
            "Completion changed while waiting for {name} DOM commit"
        ));
    }
    let source = &before["completionSource"];
    if source["enabled"] != true
        || source["identityCurrent"] != true
        || source["protectedFixture"] != true
        || source["httpCapability"] != false
        || source["networkCalls"] != 0
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || before["noSession"] != false
        || before["noTransport"] != false
    {
        return Err("Completion escaped its exact closed identity".into());
    }
    if proof["journal"]["blocks"] != proof["expectedBlocks"] {
        return Err(format!("Completion journal mismatch at {name}"));
    }
    if !matches!(name, "sent" | "readback") && g["editor"]["blocks"] != proof["expectedBlocks"] {
        return Err(format!("Completion native mismatch at {name}"));
    }
    let capture = harness::capture(d, name).await?;
    // Check immediately after the screenshot; do not wait for a later render to
    // repair a capture that crossed a popup publication or retirement.
    command(d, "snapshot", arg).await?;
    let after = snapshot(state, m);
    let journal = compose::journal_snapshot(state).await?;
    if before != after || proof["journal"] != journal {
        return Err(format!(
            "Completion changed during {name} capture: {}",
            json!({"before":before,"after":after,"journalBefore":proof["journal"],"journalAfter":journal})
        ));
    }
    rows.push(json!({"stage":name,"model":before,"geometry":g,"proof":proof,"capture":capture}));
    if matches!(name, "local-held" | "held" | "restricted-results") {
        command(d, "scroll", json!("bottom")).await?;
        let bottom = geometry(d, &capture_argument(&before, name, false)).await?;
        let image = harness::capture(d, &format!("{name}-bottom")).await?;
        command(d, "snapshot", capture_argument(&before, name, false)).await?;
        if before != snapshot(state, m) {
            return Err("Group scroll crossed publication".into());
        }
        rows.last_mut().unwrap()["proof"]["bottomWindow"] =
            json!({"geometry":bottom,"capture":image,"model":before});
        command(d, "scroll", json!("top")).await?;
    }
    Ok(())
}
pub(super) async fn reload(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
) -> Result<(), String> {
    let file = serde_json::from_value(snapshot(state, m)["initialDocument"].clone())
        .map_err(|_| "Fixture file malformed")?;
    click(d, "slack-list-close").await?;
    wait_dom(d, "({ok:!document.querySelector('.slack-list-dialog')})").await?;
    if !crate::slack_lists::comments::draft::fixture_forget_saved(&mut state.write()) {
        return Err("Completion draft was not persisted".into());
    }
    reopen(state, d, &file).await?;
    stable_drafts(state, m).await?;
    p["runtimeForgotten"] = json!(true);
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let m = fixture::begin(&mut state.write())?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Workspace missing")?
        .rt_generation = 7;
    data::seed(state).await?;
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(
        d,
        "Final review \u{4e2d}\u{1f642}\nSecond line <literal> & *punctuation*.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    edits::witness(d).await?;
    let original = current_blocks(d).await?;
    let mut p = json!({"seed":seed,"originalA":original,"expectedBlocks":original,"inputs":[],"releases":[],"runtimeForgotten":false});
    let mut rows = vec![];
    if case == "group-completion" {
        success::run(state, &m, d, &mut p, &mut rows).await?;
    } else if case == "group-completion-restricted" {
        restricted::run(state, &m, d, &mut p, &mut rows).await?;
    } else {
        custody::run(state, &m, d, &mut p, &mut rows).await?;
        drift::run(state, &m, d, &mut p, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m),"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true,
        "limitations":["Named synthetic Context carries no HTTP client; all request factories receive closed replies.","Native DOM keyboard and pointer handlers, not OS IME.","Popup samples prioritize suggestions; after dismissal real editor text is measured."]}),
    )
}
