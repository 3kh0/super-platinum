//! Finite offline shared missing-group lifecycle through actual Profile controls.
use super::super::{files as file, group_recovery as transport, missing_group as source};
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
                json!({"ok":false,"fixture":name,"case":source::case(),"error":"Missing-group title native timeout"})
            }
        };
        let _ = tokio::task::spawn_blocking(move || {
            eprintln!("profile-missing-group-title-check: {result}")
        })
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
        return Err("Closed missing-group fixture required".into());
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
        return Err("Cold file response not delivered".into());
    }
    file::mode(file::Mode::Normal);
    editor_label(desktop, "Working with ", true, source::ORIGINAL).await?;
    let first = if case == "restricted" {
        None
    } else {
        Some(request(state, "directory").await?)
    };
    shot(
        state,
        desktop,
        name,
        "opened",
        json!({"immediatePlaceholder":true,"request":first,"restricted":case=="restricted"}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if case == "retirement" {
        retirement::run(
            state,
            desktop,
            name,
            first.unwrap(),
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        let mut release = Value::Null;
        let mut fallback = Value::Null;
        if case == "restricted" {
            source::publish_known(state).await?;
            quiet(state, desktop, "Working with ", source::ORIGINAL).await?;
            if !transport::requests().is_empty()
                || !source::snapshot(&state.peek())["unknown"]["managers"]
                    .as_array()
                    .is_some_and(|v| v.is_empty())
            {
                return Err("Restricted selector registered acquisition".into());
            }
        } else {
            let first = first.unwrap();
            if case == "cancel" {
                edit_label(desktop, "Uncommitted draft with ", source::ORIGINAL).await?;
                click(desktop, "Cancel description edit", false).await?;
                no_editor(desktop).await?;
                if transport::receiver_retired(first) {
                    return Err("Cancel retired shared missing-group request".into());
                }
            } else {
                edit_label(desktop, "Retained draft with ", source::ORIGINAL).await?;
            }
            if case == "failure" {
                release = transport::release_observed(
                    first,
                    Err(Error::Offline("closed missing-group directory".into())),
                )?;
                let direct = request(state, "direct").await?;
                fallback = transport::release_observed(
                    direct,
                    Err(Error::Offline("closed missing-group fallback".into())),
                )?;
            } else {
                release = transport::release_observed(
                    first,
                    Ok(if case == "omission" {
                        json!({"ok":true,"results":[]})
                    } else {
                        source::response()
                    }),
                )?;
            }
            settled(state, 1).await?;
        }
        let text = if case == "cancel" || case == "restricted" {
            "Working with "
        } else {
            "Retained draft with "
        };
        if case == "cancel" {
            if mentions::snapshot(&state.peek())["blocks"] != json!(file::blocks("Working with ")) {
                return Err("Cancel changed raw description".into());
            }
            no_editor(desktop).await?;
        } else {
            quiet(state, desktop, text, source::ORIGINAL).await?;
        }
        shot(state,desktop,name,if case=="restricted"{"existing-restricted"}else{"settled"},json!({"release":release,"fallback":fallback,"quiet":true,"mountedLabelRetained":case!="cancel","sharedRequestSurvivedCancel":case=="cancel"}),&mut stages,&mut captures).await?;
        if case == "cancel" {
            click(desktop, "Edit description", false).await?;
            editor_label(desktop, text, true, source::FOUND).await?;
            shot(
                state,
                desktop,
                name,
                "reopened",
                json!({"freshKnownLabel":true}),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        let label = if case == "cancel" {
            source::FOUND
        } else {
            source::ORIGINAL
        };
        edit_label(desktop, "Saved with ", label).await?;
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
        editor_label(desktop, "Saved with ", true, source::final_label()).await?;
        quiet(state, desktop, "Saved with ", source::final_label()).await?;
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
        json!({"ok":true,"missingGroupTitle":true,"case":case,"fixture":name,"readiness":ready,"mentionKind":"canvas","titleBlocks":true,"stages":stages,"captures":captures,"requests":transport::requests(),"viewport":stages[0]["geometry"]["viewport"],"captureChecksPassed":true,"visualReview":"Original PNG inspection required separately"}),
    )
}
