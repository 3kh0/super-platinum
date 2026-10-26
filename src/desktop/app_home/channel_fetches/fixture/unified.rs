//! Narrow disconnected Init/Extras ingress for the unified Home fixture.
//! This does not represent a live request or a full channel-refresh result.
use super::*;

pub(crate) async fn publish(
    state: Signal<ShellState>,
    team: &str,
    init: Value,
    extras: Value,
) -> Result<(), String> {
    #[cfg(any(test, feature = "protected-native-fixture"))]
    {
        if !((crate::fixture::is_fixture()
            && std::env::var("SUPER_PLATINUM_APP_HOME_UNIFIED_CONTEXT_CHECK").as_deref()
                == Ok("1")
            && matches!(
                std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                Ok("app-home-unified-context" | "app-home-unified-context-narrow")
            ))
            || crate::app_home::inputs::external::multi_fixture::unified()
            || crate::app_home::inputs::channels::fixture::named()
            || crate::app_home::inputs::channels::fixture::multiple::named()
            || crate::app_home::inputs::channels::conversation::fixture::named()
            || crate::app_home::inputs::channels::conversation::fixture::multiple::named()
            || crate::app_home::inputs::users::fixture::named()
            || crate::app_home::inputs::users::dispatch::fixture::named()
            || crate::app_home::inputs::external::dispatch::fixture::named()
            || crate::app_home::inputs::external::multi_dispatch_fixture::named()
            || crate::app_home::inputs::number_fixture::named()
            || crate::app_home::inputs::email_fixture::named()
            || crate::app_home::inputs::focus_fixture::named()
            || crate::app_home::inputs::users::fixture::multiple::named())
        {
            return Err("Unified Home fixture ingress unavailable".into());
        }
        publish_guarded(state, team, init, extras).await
    }
    #[cfg(not(any(test, feature = "protected-native-fixture")))]
    {
        let _ = (state, team, init, extras);
        Err("Unified Home fixture ingress unavailable".into())
    }
}

#[cfg(any(test, feature = "protected-native-fixture"))]
async fn publish_guarded(
    mut state: Signal<ShellState>,
    team: &str,
    init: Value,
    extras: Value,
) -> Result<(), String> {
    let owner = {
        let mut shell = state.write();
        let ctx = Arc::new(Context::capture(&shell, team).map_err(|e| e.to_string())?);
        let transformations = Arc::new(
            members::Transformations::decode(&json!({"ok":true,"assignments":{}}))
                .map_err(|e| e.to_string())?,
        );
        initial::begin(&mut shell, ctx, transformations, true, false).map_err(|e| e.to_string())?
    };
    let result = async {
        members::drain(state, &owner)
            .await
            .map_err(|e| e.to_string())?;
        drain::wait(state, &owner)
            .await
            .map_err(|e| e.to_string())?;
        let read = {
            let shell = state.peek();
            let slot = shell
                .app_home
                .channel_fetches
                .slots
                .get(&owner.context.key())
                .ok_or("Missing unified fixture slot")?;
            initial::Read {
                owner: owner.clone(),
                token: slot
                    .initial
                    .as_ref()
                    .ok_or("Missing unified fixture attempt")?
                    .token
                    .clone(),
                channel_revision: shell.core.workspaces[team].channel_lifecycle.revision(),
            }
        };
        for (raw, kind, branch) in [
            (init, PayloadKind::Init, Branch::Init),
            (extras, PayloadKind::Extras, Branch::Extras),
        ] {
            let prepared = payload::prepare(state, &owner, kind, &raw, raw.to_string())
                .await
                .map_err(|e| e.to_string())?;
            initial::settle_payload(&mut state.write(), &read, branch, prepared)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    .await;
    // Only these two response branches were supplied. Do not fabricate a
    // Channels response, success or failure to complete this fixture scope.
    initial::retire(&mut state.write(), &owner);
    result
}
