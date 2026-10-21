use super::*;

// Passive native observation; no input, selection, or completion dispatch.
const READY: &str = r#"(function(expected){
 const editor=document.getElementById('slack-list-comment-draft');
 const popup=document.querySelector('.slack-list-channel-completions');
 const labels=[...document.querySelectorAll('.slack-list-channel-option-label')].map(n=>n.textContent);
 const sequence=Number(popup?.dataset.querySequence||0),revision=Number(popup?.dataset.queryRevision||0);
 const status=popup?.querySelector('[role=status]')?.textContent||'';
 const ready=!!popup&&editor?.__listCommentCompletion?.current()===true
   &&sequence===expected.sequence&&revision===expected.revision&&status===expected.status
   &&Array.isArray(expected.labels)&&labels.length===expected.labels.length
   &&labels.every((label,i)=>label===expected.labels[i]);
 return {ok:true,ready,sequence,revision,labels,status};
})"#;

pub(super) async fn settled_status(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    stage: &str,
    failed: bool,
) -> Result<(), String> {
    for _ in 0..250 {
        let before = snapshot(state, m);
        let q = &before["completion"];
        if q["current"] == true
            && q["batchCurrent"] == !failed
            && (failed || q["settledBatch"] == true)
        {
            let expected = capture_argument(&before, stage, false)["expectedCompletion"].clone();
            let native = checked(d, &format!("({READY})({expected})")).await?;
            if native["ready"] == true && snapshot(state, m)["completion"] == *q {
                if !p["settled"].is_object() {
                    p["settled"] = json!({});
                }
                p["settled"][stage] = json!({"completion":q,"native":native});
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Final {stage} publication did not settle: {}",
        snapshot(state, m)
    ))
}

pub(super) async fn run(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(state, m, d, "draft", p, rows, false).await?;
    query(state, m, d, p, "held", "@alex", data::Mode::Held).await?;
    wait(state, m, |v| {
        search_ready(v, 1, 0, 0, 1)
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == false
            && v["completion"]["labels"]
                .as_array()
                .is_some_and(|a| a.len() == 7)
    })
    .await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "held", p, rows, false).await?;
    let original = rows.last().unwrap()["geometry"]["completion"].clone();
    query(
        state,
        m,
        d,
        p,
        "query-retired",
        "@nomatch",
        data::Mode::Success,
    )
    .await?;
    // Request completion precedes result publication; wait for the settled model.
    wait(state, m, |v| {
        search_ready(v, 2, 2, 1, 1)
            && v["completion"]["current"] == true
            && v["completion"]["started"] == true
            && v["completion"]["consumed"] == false
            && v["completion"]["batchCurrent"] == true
            && v["completion"]["settledBatch"] == true
            && v["completion"]["labels"] == json!([])
    })
    .await?;
    settled_status(state, m, d, p, "query-retired", false).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    capture(state, m, d, "query-retired", p, rows, false).await?;
    let stale = command(d, "stale", original).await?;
    if stale["before"] != stale["after"] {
        return Err("Stale suggestion changed editor".into());
    }
    let release = data::handle(&state.peek())
        .map_err(|e| e.to_string())?
        .release()
        .map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Retired member query received late rows".into());
    }
    p["staleChoice"] = stale;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"stale-refused","value":release}));
    settled_status(state, m, d, p, "stale-refused", false).await?;
    capture(state, m, d, "stale-refused", p, rows, false).await?;
    for (stage, term, mode, n) in [
        ("malformed", "@malformed", data::Mode::Malformed, 3),
        ("failure", "@offline", data::Mode::Offline, 4),
        ("empty", "@empty", data::Mode::Empty, 5),
    ] {
        query(state, m, d, p, stage, term, mode).await?;
        wait(state, m, |v| {
            search_ready(v, n, n, 1, 0)
                && v["completion"]["current"] == true
                && v["completion"]["started"] == true
                && v["completion"]["consumed"] == false
                && v["completion"]["batchCurrent"] == (stage == "empty")
                && (stage != "empty" || v["completion"]["settledBatch"] == true)
                && v["completion"]["labels"] == json!([])
        })
        .await?;
        settled_status(state, m, d, p, stage, stage != "empty").await?;
        p["expectedBlocks"] = current_blocks(d).await?;
        capture(state, m, d, stage, p, rows, false).await?;
    }
    query(
        state,
        m,
        d,
        p,
        "cancelled-reopened",
        "@cancelled",
        data::Mode::Held,
    )
    .await?;
    wait(state, m, |v| search_ready(v, 6, 5, 1, 1)).await?;
    p["expectedBlocks"] = current_blocks(d).await?;
    reload(state, m, d, p).await?;
    wait(state, m, |v| search_ready(v, 6, 6, 2, 1)).await?;
    let release = data::handle(&state.peek())
        .map_err(|e| e.to_string())?
        .release()
        .map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Departed original query received late rows".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"cancelled-reopened","value":release}));
    capture(state, m, d, "cancelled-reopened", p, rows, false).await?;
    Ok(())
}
