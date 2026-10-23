//! Empty extracted Canvas title through actual completion insertion and save.
use super::super::empty_canvas as title;
use super::*;
const PROBE: &str = include_str!("title_users.js");
pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let value = match tokio::time::timeout(
            Duration::from_secs(190),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":"completion","error":error,"model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":"completion","error":"Empty Canvas completion timeout"})
            }
        };
        eprintln!("profile-file-completion-check: {value}");
    });
}
async fn shot(
    state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    stage: &str,
    index: Option<usize>,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let proof = json!({"exercise":exercise,"emptyCanvas":title::snapshot(),"choices":super::super::super::rich::fixture_choices(&state.peek()),"sourceFiles":super::super::super::rich::search::file_source::fixture_completion_proof(&state.peek()).ok()});
    capture(state, d, name, stage, index, proof, stages, captures).await
}
async fn run(
    mut state: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !title::completion() || !files::enabled(&state.peek()) || !mentions::user_mode() {
        return Err("Exact empty Canvas completion fixture required".into());
    }
    mentions::seed(state, "Alex Local").await?;
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, d).await?;
    helper::click(d, "Edit description", false).await?;
    base::tokens(d, 2).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        state,
        d,
        name,
        "opened",
        None,
        json!({"initialBlocks":initial()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::query(d, "@alex").await?;
    base::rows(d, &[title::EMPTY], false).await?;
    base::ledger(state, 0, 0).await?;
    let expected = choices(&state.peek())?;
    let index = expected["values"]
        .as_array()
        .ok_or("Missing completion catalog")?
        .iter()
        .position(|r| r["value"]["file_id"] == files::CANVAS && r["label"] == title::EMPTY)
        .ok_or("Empty Canvas choice unavailable")?;
    let native = helper::wait_dom(d, &format!("({PROBE})({expected})")).await?;
    shot(
        state,
        d,
        name,
        "query-ready",
        Some(index),
        json!({"choices":expected,"native":native}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let choice = choose(state, d, files::CANVAS, true).await?;
    let native = base::tokens(d, 3).await?;
    inserted(&native, &[files::CANVAS])?;
    if native["native"]["tokens"][2]["label"] != title::EMPTY {
        return Err("Inserted Canvas lost original empty-title observation".into());
    }
    let blocks = native["native"]["snapshot"]["blocks"].clone();
    shot(
        state,
        d,
        name,
        "inserted",
        None,
        json!({"choice":choice,"expectedBlocks":blocks}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(d, "Use description", false).await?;
    helper::wait_dom(d,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})").await?;
    if mentions::snapshot(&state.peek())["blocks"] != blocks {
        return Err("Use changed original wire tokens".into());
    }
    helper::save(state, d, Outcome::Saved, 1, 1).await?;
    helper::click(d, "Edit description", false).await?;
    let reopened = base::tokens(d, 3).await?;
    inserted(&reopened, &[files::CANVAS])?;
    if reopened["native"]["tokens"][2]["label"] != title::EMPTY
        || reopened["native"]["snapshot"]["blocks"] != blocks
    {
        return Err("Fresh save/readback changed raw token or fallback".into());
    }
    shot(
        state,
        d,
        name,
        "saved",
        None,
        json!({"expectedBlocks":blocks,"reopened":reopened,"exactReadback":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":"completion","readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"source":source::snapshot(&state.peek()),"emptyCanvas":title::snapshot(),"model":mentions::snapshot(&state.peek())}),
    )
}
