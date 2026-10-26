//! Offline retained-history pair replacement. No transport or alternate edit API.
use super::*;
use crate::canvas::text::check::{checked, editor_layout, wait};
use std::sync::atomic::{AtomicBool, Ordering};
use super_platinum_core::canvas::edit::structure::{ReplacementIntent, StructureIntent};
mod capture;
pub(super) mod confirmed;
mod interaction;
mod pair;
pub(super) mod retirement_cancellation;
#[cfg(test)]
mod tests;
static STARTED: AtomicBool = AtomicBool::new(false);
#[derive(Default)]
pub(super) struct Evidence {
    preflight: Option<Value>,
    completion: Option<Value>,
    restore: Option<Value>,
}
pub(in crate::canvas::text) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-last-live-replacement"
                | "canvas-last-live-replacement-narrow"
                | "canvas-confirmed-provenance"
                | "canvas-confirmed-provenance-narrow"
                | "canvas-retirement-cancellation"
                | "canvas-retirement-cancellation-narrow")
        )
}
fn case() -> String {
    if retirement_cancellation::requested() {
        return retirement_cancellation::case();
    }
    if confirmed::requested() {
        return confirmed::case();
    }
    env(
        "SUPER_PLATINUM_CANVAS_LAST_LIVE_REPLACEMENT_CASE",
        "success",
    )
}
pub(super) fn seed() -> Arc<CanvasCollabSnapshot> {
    let source = owner_first::seed_variant("ascii", "bootstrap", "plain");
    let child = seed::child(&source, "paragraph");
    let prepared = StructureIntent::delete(&source, &child, 100)
        .unwrap()
        .prepare(&source, 100)
        .unwrap();
    let (accepted, fresh, _) = completion::delete(prepared, source.clone());
    assert!(accepted.confirms_readback(&fresh).unwrap());
    assert_eq!(fresh.section("paragraph"), source.section("paragraph"));
    assert_eq!(
        fresh.section(&child).unwrap().deleted().unwrap(),
        Some(true)
    );
    assert_eq!(fresh.sections.len(), 4);
    fresh
}
pub(super) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !requested()
        || (if retirement_cancellation::requested() {
            env("SUPER_PLATINUM_CANVAS_RETIREMENT_CANCELLATION_CHECK", "0")
        } else if confirmed::requested() {
            env("SUPER_PLATINUM_CANVAS_CONFIRMED_PROVENANCE_CHECK", "0")
        } else {
            env("SUPER_PLATINUM_CANVAS_LAST_LIVE_REPLACEMENT_CHECK", "0")
        }) != "1"
        || STARTED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let mut record = capture::Record::default();
        let result = if retirement_cancellation::requested() {
            retirement_cancellation::run(state, &desktop, &mut record).await
        } else if confirmed::requested() {
            confirmed::run(state, &desktop, &mut record).await
        } else {
            interaction::run(state, &desktop, &mut record).await
        };
        let receipt = record.finish(result);
        let prefix = if retirement_cancellation::requested() {
            "canvas-retirement-cancellation-check"
        } else if confirmed::requested() {
            "canvas-confirmed-provenance-check"
        } else {
            "canvas-last-live-replacement-check"
        };
        eprintln!("{prefix}: {receipt}");
    });
}
pub(in crate::canvas::text) fn dispatch(
    mut state: Signal<ShellState>,
    draft: Signal<Option<Draft>>,
    generation: u64,
    attempt: checklist::Attempt,
    intent: ReplacementIntent,
) {
    if !requested() || !enabled(&state.peek()) {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let source = snapshot(&state.peek());
        let original = source.clone();
        let variant = if fixture(&mut state.write()).last_live.preflight.is_some() {
            "success".to_owned()
        } else {
            case()
        };
        let result = tokio::task::spawn_blocking(move || {
            let fresh = pair::preflight_source(&original, &variant);
            let proof = seed::saved(&fresh);
            let prepared =
                super_platinum_core::canvas::edit::structure::carry_replacement_provenance(
                    &original, &fresh,
                )
                .and_then(|fresh| intent.prepare(&fresh, checklist::save::now_usec().unwrap()));
            (prepared, proof)
        })
        .await;
        let Ok((prepared, proof)) = result else {
            replacement_save::reject(
                state,
                draft,
                generation,
                &attempt,
                "Fixture pair worker failed.",
            );
            return;
        };
        if !same_source(&state.peek(), &source) || !checklist::dispatchable(&state.peek(), &attempt)
        {
            replacement_save::reject(state, draft, generation, &attempt, "Fixture owner changed.");
            return;
        }
        fixture(&mut state.write()).last_live.preflight = Some(json!({
            "canonical":proof,"accepted":prepared.is_ok(),"case":case()
        }));
        let prepared = match prepared {
            Ok(p) => p,
            Err(_) => {
                replacement_save::reject(
                    state,
                    draft,
                    generation,
                    &attempt,
                    "Fixture preflight changed.",
                );
                return;
            }
        };
        let (prepared, ticket) =
            match checklist::recovery::reserve_replacement(state, &attempt, prepared).await {
                Ok(t) => t,
                Err(reason) => {
                    replacement_save::reject(state, draft, generation, &attempt, reason);
                    return;
                }
            };
        if !same_source(&state.peek(), &source) || !checklist::dispatchable(&state.peek(), &attempt)
        {
            let _ = checklist::recovery::complete(state, &ticket).await;
            replacement_save::reject(state, draft, generation, &attempt, "Fixture owner changed.");
            return;
        }
        let mut shell = state.write();
        let f = shell.canvas_checklist.structure_fixture.as_mut().unwrap();
        f.pair_reservations += 1;
        f.reserved_replacement = Some((attempt, prepared, ticket));
    });
}
