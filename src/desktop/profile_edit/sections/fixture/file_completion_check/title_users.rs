//! Named nested-user completion through original input, choices, insertion and Save.
use super::super::member_recovery as member;
use super::*;
use source::title_users as title;
const PROBE: &str = include_str!("title_users.js");

pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(190),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => json!({"ok":false,"fixture":name,"case":title::case(),"error":error,
                "titleCompletion":title::snapshot(&state.peek()),"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())}),
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":title::case(),"error":"Title completion timeout"})
            }
        };
        eprintln!("profile-user-title-completion-check: {result}");
    });
}
async fn held(kind: &str) -> Result<u64, String> {
    for _ in 0..120 {
        let rows = member::pending();
        if rows.len() == 1 && rows[0]["kind"] == kind {
            return rows[0]["serial"]
                .as_u64()
                .ok_or("Missing member serial".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Expected original held {kind}: {:?}",
        member::requests()
    ))
}
async fn settled(state: Signal<ShellState>) -> Result<(), String> {
    for _ in 0..120 {
        if member::snapshot(&state.peek())["driver"]["active"] == 0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Original member driver did not settle".into())
}
async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    index: Option<usize>,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let observed = super::super::super::rich::fixture_choices(&state.peek());
    let source_files =
        super::super::super::rich::search::file_source::fixture_completion_proof(&state.peek())
            .ok();
    let proof = json!({"exercise":exercise,"titleCompletion":title::snapshot(&state.peek()),"choices":observed,"sourceFiles":source_files});
    capture(state, desktop, name, stage, index, proof, stages, captures).await
}
async fn held_pair(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    prefix: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<u64, String> {
    let serial = held("discovery").await?;
    base::ledger(state, 0, 0).await?;
    shot(
        state,
        desktop,
        name,
        &format!("{prefix}member-discovery"),
        None,
        json!({"serial":serial}),
        stages,
        captures,
    )
    .await?;
    member::release(
        serial,
        Ok((
            json!({"ok":true,"url":"https://edgeapi.slack.com/cache/TINIT0001","ttl_seconds":300}),
            None,
        )),
    )?;
    let serial = held("member").await?;
    shot(
        state,
        desktop,
        name,
        &format!("{prefix}member-held"),
        None,
        json!({"serial":serial}),
        stages,
        captures,
    )
    .await?;
    Ok(serial)
}
async fn ready(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<(Value, usize), String> {
    base::rows(desktop, &[title::label()], false).await?;
    base::ledger(state, 0, 0).await?;
    let expected = choices(&state.peek())?;
    if expected["titleUsers"][files::CANVAS]["current"] != true {
        return Err("Original nested title source is not current".into());
    }
    let index = expected["values"]
        .as_array()
        .ok_or("Missing choices")?
        .iter()
        .position(|r| r["value"]["file_id"] == files::CANVAS && r["label"] == title::label())
        .ok_or("Missing resolved Canvas choice")?;
    let native = helper::wait_dom(desktop, &format!("({PROBE})({expected})")).await?;
    Ok((json!({"choices":expected,"native":native}), index))
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !title::named() || !files::enabled(&state.peek()) || !mentions::user_mode() {
        return Err("Closed title completion unavailable".into());
    }
    title::seed(state).await?;
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, desktop).await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 2).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    shot(
        state,
        desktop,
        name,
        "opened",
        None,
        json!({"initialBlocks":initial()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::query(desktop, "@alex").await?;
    let case = title::case();
    if matches!(case.as_str(), "recovery" | "retirement") {
        let first = held_pair(state, desktop, name, "", &mut stages, &mut captures).await?;
        if case == "recovery" {
            member::release(
                first,
                Err(Error::Offline(
                    "Synthetic nested completion member unavailable".into(),
                )),
            )?;
            settled(state).await?;
            base::status(desktop, FAILURE).await?;
            shot(
                state,
                desktop,
                name,
                "unavailable",
                None,
                json!({"failedMember":first}),
                &mut stages,
                &mut captures,
            )
            .await?;
            base::query(desktop, "@al").await?;
            let retry =
                held_pair(state, desktop, name, "retry-", &mut stages, &mut captures).await?;
            member::release(retry, Ok((member::success("TINIT0001"), None)))?;
            settled(state).await?;
        } else {
            base::query(desktop, "#project").await?;
            base::rows(desktop, &["#project-completion", "#project-cove"], true).await?;
            base::ledger(state, 0, 0).await?;
            let replacement = choices(&state.peek())?;
            shot(
                state,
                desktop,
                name,
                "query-replaced",
                None,
                json!({"heldMember":first,"replacement":replacement}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let release =
                member::release_observed(first, Ok((member::success("TINIT0001"), None)))?;
            settled(state).await?;
            let after = choices(&state.peek())?;
            if after != replacement || member::snapshot(&state.peek())["sourcePresent"] == true {
                return Err("Retired member adopted into replacement query/source".into());
            }
            shot(
                state,
                desktop,
                name,
                "late-settled",
                None,
                json!({"release":release,"replacement":replacement}),
                &mut stages,
                &mut captures,
            )
            .await?;
            base::query(desktop, "@al").await?;
            let cancel = held_pair(
                state,
                desktop,
                name,
                "replacement-",
                &mut stages,
                &mut captures,
            )
            .await?;
            helper::click(desktop, "Cancel description edit", false).await?;
            helper::wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})").await?;
            let release =
                member::release_observed(cancel, Ok((member::success("TINIT0001"), None)))?;
            settled(state).await?;
            if mentions::snapshot(&state.peek())["blocks"] != json!(initial())
                || member::snapshot(&state.peek())["sourcePresent"] == true
            {
                return Err("Cancelled title completion changed original draft/source".into());
            }
            shot(
                state,
                desktop,
                name,
                "cancelled",
                None,
                json!({"release":release,"expectedBlocks":initial()}),
                &mut stages,
                &mut captures,
            )
            .await?;
            return Ok(
                json!({"ok":true,"fixture":name,"case":case,"readiness":readiness,"stages":stages,"captures":captures,
                "initialBlocks":initial(),"titleCompletion":title::snapshot(&state.peek()),"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())}),
            );
        }
    }
    let (proof, index) = ready(state, desktop).await?;
    shot(
        state,
        desktop,
        name,
        "query-ready",
        Some(index),
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    let choice = choose(state, desktop, files::CANVAS, true).await?;
    let native = base::tokens(desktop, 3).await?;
    inserted(&native, &[files::CANVAS])?;
    let expected = native["native"]["snapshot"]["blocks"].clone();
    shot(
        state,
        desktop,
        name,
        "inserted",
        None,
        json!({"choice":choice,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Use description", false).await?;
    helper::wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})").await?;
    if mentions::snapshot(&state.peek())["blocks"] != expected {
        return Err("Use description changed original rich wire".into());
    }
    helper::save(state, desktop, Outcome::Saved, 1, 1).await?;
    helper::click(desktop, "Edit description", false).await?;
    let reopened = base::tokens(desktop, 3).await?;
    inserted(&reopened, &[files::CANVAS])?;
    if reopened["native"]["snapshot"]["blocks"] != expected {
        return Err("Save/readback changed rich wire".into());
    }
    shot(
        state,
        desktop,
        name,
        "saved",
        None,
        json!({"expectedBlocks":expected,"exactReadback":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case,"readiness":readiness,"stages":stages,"captures":captures,
        "initialBlocks":initial(),"savedBlocks":expected,"titleCompletion":title::snapshot(&state.peek()),"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek()),
        "limitations":["Closed synthetic reads only; no live Slack.","DOM input/keyboard/mouse, not OS IME.","Popup capture targets Canvas; other rows may be outside its scroll viewport."]}),
    )
}
