//! Activity subscriber to the one root-lived unknown-member manager. The
//! account owns IO and its receipts; this query owns only renderer adoption.
use super::*;
use crate::directory_member_fetches::driver::Client;
use super_platinum_core::block_actions::directory_endpoint::member_info::unknown::PlaceholderAsset;

// Actual literal webpack publicPath captured by root's read-only expression
// d855d037 in calendar-public-map-reference1.json, not inferred from an asset URL.
const OBSERVED_PUBLIC_PATH: &str = "https://a.slack-edge.com/bv1-13/";

pub(in crate::app_home::about::management::activity::members) async fn recover_absent(
    mut state: Signal<ShellState>,
    q: &Query,
) -> Result<(), Error> {
    let requested = {
        let s = state.peek();
        if !q.current(&s) {
            return Err(unavailable());
        }
        let Some(recovery) = Recovery::capture(&s, q)? else {
            return Ok(());
        };
        let known: std::collections::BTreeSet<_> = recovery.prior.source_ids().collect();
        let absent: Vec<_> = recovery
            .requested()
            .iter()
            .filter(|id| !known.contains(id.as_str()))
            .cloned()
            .collect();
        if absent.is_empty() {
            return Ok(());
        }
        // Named picker fixtures may use complete raw source observations, but
        // must never start the root-lived real directory IO manager.
        if q.fixture || crate::fixture::is_fixture() {
            return Err(unavailable());
        }
        if !q.can_dispatch(&s) {
            return Err(unavailable());
        }
        absent
    };
    // Obtain renderer inputs before registration. A later accepted sibling429
    // must not force another HTTP read before this consumer can adopt its batch.
    let raw = read::request(
        state,
        q,
        super_platinum_core::slack::api::list_experiments(
            &q.owner.ctx.client,
            &q.owner.ctx.session,
        ),
    )
    .await?;
    let flags = Flags::decode(&raw)?;
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    let asset = PlaceholderAsset::observed(OBSERVED_PUBLIC_PATH)?;
    let Some(mut client) = Client::begin(state, &q.owner.ctx.session.team_id, &requested)? else {
        return Ok(());
    };
    loop {
        let elected = tokio::select! {
            biased;
            elected = client.next(|s| q.current(s)) => elected?,
            _ = q.retired(state) => return Err(unavailable()),
        };
        let Some(mut election) = elected else { break };
        let mut stage = {
            let s = state.peek();
            if !q.current(&s) || !election.current(&s) {
                return Err(unavailable());
            }
            election.stage(&s, flags.clone(), q.generation(), &asset)?
        };
        loop {
            let pending = {
                let s = state.peek();
                if !q.current(&s) {
                    return Err(unavailable());
                }
                stage.prepare(&s, &election)?
            };
            let Some(pending) = pending else { break };
            let reply = render::hydrate(state, q, &pending).await?;
            let s = state.peek();
            if !q.current(&s) {
                return Err(unavailable());
            }
            stage.apply(&s, &election, pending, reply)?;
        }
        // No shared raw batch has been enqueued in canonical state while the
        // renderer was suspended. A retired adopter drops its private Stage;
        // Election Drop makes the original receipt available to a sibling.
        stage.install(&mut state.write(), &mut election, |s| q.current(s))?;
        crate::bootstrap::persist_workspace_metadata(&state, &q.owner.ctx.session.team_id);
    }
    // Shared settlement is not a Ready/eligibility certificate. The caller now
    // captures its own canonical view and performs any known-row drift repair.
    q.current(&state.peek())
        .then_some(())
        .ok_or_else(unavailable)
}
