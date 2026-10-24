//! Finite shared-group title acceptance through existing native Profile controls.
use super::super::{files as file, group_recovery as transport, group_title as source};
use super::*;
use crate::profile_edit::sections::rich;
mod helpers;
mod retirement;
use helpers::*;

pub(super) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(150),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":e,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Group title native timeout"})
            }
        };
        let _ =
            tokio::task::spawn_blocking(move || eprintln!("profile-group-title-check: {result}"))
                .await;
    });
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let ready = readiness(name).await?;
    if !source::named() || !file::enabled(&state.peek()) {
        return Err("Closed group title owner required".into());
    }
    select_values(state, desktop).await?;
    source::seed(state).await?;
    let case = source::case();
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    file::mode(file::Mode::Held);
    click(desktop, "Edit description", false).await?;
    held_file().await?;
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
    if file::release()["delivered"] != true {
        return Err("Initial file reply not delivered".into());
    }
    file::mode(file::Mode::Normal);
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    quiet(state, desktop, "Working with ", source::ORIGINAL).await?;
    if !transport::requests().is_empty() {
        return Err("Fresh shared group source unexpectedly fetched".into());
    }
    shot(
        state,
        desktop,
        name,
        "fresh-opened",
        json!({"actualSourceSink":true,"noGroupRead":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let reset = reset_open(state, desktop, "Working with ", source::ORIGINAL).await?;
    let first = request(state, "directory").await?;
    shot(
        state,
        desktop,
        name,
        "mounted-held",
        json!({"reset":reset,"request":first,"immediateAdmission":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        retirement::run(state, desktop, name, first, &mut stages, &mut captures).await?;
    } else if case == "cancel" {
        edit_label(desktop, "Uncommitted draft with ", source::ORIGINAL).await?;
        click(desktop, "Cancel description edit", false).await?;
        no_editor(desktop).await?;
        if transport::receiver_retired(first) {
            return Err("Editor Cancel retired shared group request".into());
        }
        let released = transport::release_observed(first, Ok(source::response(true)))?;
        settled(state, 1).await?;
        require_label(state, "@renamed")?;
        if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
            return Err("Cancel changed description form".into());
        }
        shot(
            state,
            desktop,
            name,
            "late-settled",
            json!({"release":released,"cancelledEditorNotResumed":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Working with ", true, source::RENAMED).await?;
        quiet(state, desktop, "Working with ", source::RENAMED).await?;
        shot(
            state,
            desktop,
            name,
            "reopened",
            json!({"newOpeningLabel":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        edit_label(desktop, "Saved with ", source::RENAMED).await?;
        use_description(state, desktop, "Saved with ").await?;
        shot(
            state,
            desktop,
            name,
            "used",
            json!({"rawTokensRetained":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Saved with ", true, source::RENAMED).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"confirmedSave":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        let mut finished = 1;
        let text = if case == "failure" {
            "Retained draft with "
        } else {
            "Working with "
        };
        if case == "failure" {
            edit_label(desktop, text, source::ORIGINAL).await?;
            let first_release = transport::release_observed(
                first,
                Err(Error::Offline("closed group title directory".into())),
            )?;
            let fallback = request(state, "direct").await?;
            let fallback_release = transport::release_observed(
                fallback,
                Err(Error::Offline("closed group title fallback".into())),
            )?;
            settled(state, 1).await?;
            require_label(state, "@design")?;
            quiet(state, desktop, text, source::ORIGINAL).await?;
            shot(state,desktop,name,"failure-retained",json!({"firstRelease":first_release,"fallbackRelease":fallback_release,"quiet":true,"draftRetained":true}),&mut stages,&mut captures).await?;
            use_description(state, desktop, text).await?;
            let count = transport::requests().len();
            click(desktop, "Edit description", false).await?;
            quiet(state, desktop, text, source::ORIGINAL).await?;
            if transport::requests().len() != count {
                return Err("Attempted-fresh group retried without lifecycle change".into());
            }
            shot(
                state,
                desktop,
                name,
                "fresh-reopened",
                json!({"noAdditionalRead":true,"attemptedFresh":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            let reset = reset_open(state, desktop, text, source::ORIGINAL).await?;
            let retry = request(state, "directory").await?;
            shot(
                state,
                desktop,
                name,
                "retry-held",
                json!({"reset":reset,"request":retry,"selectorHitAfterReconnect":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
            transport::release(retry, Ok(source::response(true)))?;
            finished = 2;
        } else {
            transport::release(first, Ok(source::response(true)))?;
        }
        settled(state, finished).await?;
        require_label(state, "@renamed")?;
        editor_label(desktop, text, true, source::ORIGINAL).await?;
        shot(
            state,
            desktop,
            name,
            "renamed-mounted",
            json!({"mountedLabelRetained":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        edit_label(desktop, "Saved with ", source::ORIGINAL).await?;
        use_description(state, desktop, "Saved with ").await?;
        shot(
            state,
            desktop,
            name,
            "used",
            json!({"rawTokensRetained":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        save(state, desktop, Outcome::Saved, 1, 1).await?;
        click(desktop, "Edit description", false).await?;
        editor_label(desktop, "Saved with ", true, source::RENAMED).await?;
        shot(
            state,
            desktop,
            name,
            "confirmed",
            json!({"confirmedSave":true,"newOpeningLabel":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"groupTitle":true,"case":case,"fixture":name,"readiness":ready,"mentionKind":"canvas","titleBlocks":true,
        "stages":stages,"captures":captures,"requests":transport::requests(),"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"visualReview":"Original PNG inspection required separately"}),
    )
}
