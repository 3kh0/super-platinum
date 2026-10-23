//! Original Profile controls and shared member recovery, without model edits.
use super::super::{files as file, member_recovery as member, user_title as source};
use super::*;
use crate::profile_edit::sections::rich::files as cache;

pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":error,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"User title native timeout"})
            }
        };
        eprintln!("profile-user-title-check: {result}");
    });
}
async fn held(kind: &str) -> Result<u64, String> {
    for _ in 0..100 {
        let rows = member::pending();
        if rows.len() == 1 && rows[0]["kind"] == kind {
            return rows[0]["serial"]
                .as_u64()
                .ok_or("Missing member serial".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("Expected held {kind}: {:?}", member::requests()))
}
async fn files_settled(completed: usize, held: bool) -> Result<(), String> {
    for _ in 0..100 {
        let status = file::status();
        if status["requests"].as_array().map(Vec::len) == Some(1)
            && status["completed"] == completed
            && status["held"] == held
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("Unexpected files.info: {}", file::status()))
}
async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let proof = json!({"source":source::snapshot(&state.peek()),"cache":cache::fixture_status(&state.peek(),file::ID),"exercise":exercise});
    capture(state, desktop, name, stage, proof, stages, captures).await
}
async fn unavailable(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&document.getElementById('profile-section-rich-status')?.textContent==='Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'})").await.map(|_|())
}
async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed user title owner missing".into());
    }
    select_values(state, desktop).await?;
    source::seed(state).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    let case = source::case();
    file::mode(file::Mode::Held);
    click(desktop, "Edit description", false).await?;
    files_settled(0, true).await?;
    shot(
        state,
        desktop,
        name,
        "cold-loading",
        json!({"cold":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let release = file::release();
    if release["delivered"] != true {
        return Err("Original file reply not delivered".into());
    }
    files_settled(1, false).await?;
    if !matches!(case.as_str(), "known" | "hidden") {
        let discovery = held("discovery").await?;
        shot(
            state,
            desktop,
            name,
            "member-discovery",
            json!({"release":release}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let team = state
            .peek()
            .core
            .active_team
            .clone()
            .ok_or("Missing team")?;
        member::release(
            discovery,
            Ok((
                json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{team}"),"ttl_seconds":300}),
                None,
            )),
        )?;
        let serial = held("member").await?;
        shot(
            state,
            desktop,
            name,
            "member-held",
            json!({"serial":serial}),
            &mut stages,
            &mut captures,
        )
        .await?;
        if case == "cancel" {
            click(desktop, "Cancel description edit", false).await?;
            wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await?;
            let release = member::release_observed(serial, Ok((member::success(&team), None)))?;
            // Shared request may complete, but its retired opening cannot adopt
            // a label or publish a private member election without a subscriber.
            for _ in 0..100 {
                if member::snapshot(&state.peek())["driver"]["active"] == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            if member::snapshot(&state.peek())["driver"]["active"] != 0 {
                return Err("Cancelled member callback did not settle".into());
            }
            shot(
                state,
                desktop,
                name,
                "cancelled",
                json!({"release":release}),
                &mut stages,
                &mut captures,
            )
            .await?;
        } else {
            if case == "failure" {
                member::release(
                    serial,
                    Err(Error::Offline("closed nested title member".into())),
                )?;
                unavailable(desktop).await?;
                shot(
                    state,
                    desktop,
                    name,
                    "unavailable",
                    Value::Null,
                    &mut stages,
                    &mut captures,
                )
                .await?;
                click(desktop, "Retry description", false).await?;
                // Each new shared acquisition owns its own endpoint cache. The
                // failed dispatch's discovery is not reused by this Retry.
                let discovery = held("discovery").await?;
                files_settled(1, false).await?;
                shot(
                    state,
                    desktop,
                    name,
                    "retry-member-discovery",
                    json!({"serial":discovery,"fileReused":true}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
                member::release(
                    discovery,
                    Ok((
                        json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{team}"),"ttl_seconds":300}),
                        None,
                    )),
                )?;
                let retry = held("member").await?;
                files_settled(1, false).await?;
                shot(
                    state,
                    desktop,
                    name,
                    "retry-member-held",
                    json!({"serial":retry,"fileReused":true}),
                    &mut stages,
                    &mut captures,
                )
                .await?;
                member::release(retry, Ok((member::success(&team), None)))?;
            } else {
                member::release(serial, Ok((member::success(&team), None)))?;
            }
        }
    }
    if case != "cancel" {
        editor_label(desktop, "Working with ", true, source::label()).await?;
        files_settled(1, false).await?;
        shot(
            state,
            desktop,
            name,
            "opened",
            json!({"release":release}),
            &mut stages,
            &mut captures,
        )
        .await?;
        edit_label(desktop, "Saved with ", source::label()).await?;
        use_description(state, desktop, "Saved with ").await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Saved with ", true, source::label()).await?;
        files_settled(1, false).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"confirmedSave":true,"fileReused":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"userTitle":true,"case":case,"fixture":name,"readiness":ready,"mentionKind":"canvas","titleBlocks":true,
        "stages":stages,"captures":captures,"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"visualReview":"Original PNG inspection required separately"}),
    )
}
