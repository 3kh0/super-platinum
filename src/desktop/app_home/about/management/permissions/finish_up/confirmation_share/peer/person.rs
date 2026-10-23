//! Bounded known-person choices, retaining actual source/typed custody.
use super::*;
use super_platinum_core::workflow_share::person as protocol;

#[derive(Clone)]
pub struct Candidate {
    witness: Arc<Witness>,
    label: String,
}
impl Candidate {
    pub fn id(&self) -> &str {
        &self.witness.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn current(&self, s: &ShellState) -> bool {
        self.witness.current(s)
    }
    pub fn evidence(&self) -> Value {
        json!({"id":self.id(),"source":self.witness.subset.rows().first().map(|r|&r.source),
            "interaction":self.witness.prior.get(self.id())})
    }
}

/// Query results are actual published source IDs, independent of the initial
/// suggestion cap. Capture fresh source custody; never manufacture a raw peer.
pub fn candidates_for(s: &ShellState, team: &str, ids: &[String]) -> Vec<Candidate> {
    ids.iter()
        .take(24)
        .filter_map(|id| {
            let witness = Witness::capture(s, team, id).ok()?;
            let [source] = witness.subset.rows() else {
                return None;
            };
            if !witness.current(s)
                || witness.prior.get(id) == Some(&Some(false))
                || !protocol::eligible(&source.source, team)
            {
                return None;
            }
            let w = s.core.workspaces.get(team)?;
            Some(Candidate {
                label: super_platinum_core::state::display_name(w.users.get(id), id),
                witness: Arc::new(witness),
            })
        })
        .collect()
}

/// Partial known collection, deterministically ordered by actual source ID.
/// This is not Slack search/frecency ranking or an exhaustive people directory.
pub fn candidates(s: &ShellState, team: &str) -> Result<Vec<Candidate>, Error> {
    collect_candidates(s, team, Some(50))
}

/// Ranking sees the actual known collection before its combined-family cutoff.
/// Each candidate still retains the original source/interaction witness.
pub fn ranking_candidates(s: &ShellState, team: &str) -> Result<Vec<Candidate>, Error> {
    collect_candidates(s, team, None)
}

fn collect_candidates(
    s: &ShellState,
    team: &str,
    limit: Option<usize>,
) -> Result<Vec<Candidate>, Error> {
    let ctx = Context::capture(s, team)?;
    let w = s.core.workspaces.get(team).ok_or_else(unavailable)?;
    let observations =
        w.directory_members
            .observations(&ctx.account, &ctx.session, ctx.epoch, GENERATION)?;
    if !ctx.current(s)
        || !observations.current(
            &w.directory_members,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
        )
    {
        return Err(unavailable());
    }
    let mut rows = Vec::new();
    for id in observations.source_ids() {
        let Ok(witness) = Witness::capture(s, team, id) else {
            continue;
        };
        let [source] = witness.subset.rows() else {
            continue;
        };
        if !witness.current(s)
            || witness.prior.get(id) == Some(&Some(false))
            || !protocol::eligible(&source.source, team)
        {
            continue;
        }
        let label = super_platinum_core::state::display_name(w.users.get(id), id);
        rows.push(Candidate {
            witness: Arc::new(witness),
            label,
        });
        if limit.is_some_and(|limit| rows.len() == limit) {
            break;
        }
    }
    Ok(rows)
}

pub async fn acquire_person(
    state: Signal<ShellState>,
    query: &Query,
    candidate: &Candidate,
) -> Result<Arc<Peer>, Error> {
    let team = &query.ctx().session.team_id;
    if candidate.witness.ctx.session.team_id != *team {
        return Err(unavailable());
    }
    let fresh = acquire_fresh(state, query, &candidate.witness, &|s| candidate.current(s)).await?;
    if !protocol::eligible(&fresh.effective, team) {
        return Err(unavailable());
    }
    Ok(Arc::new(Peer {
        witness: candidate.witness.clone(),
        raw: fresh.raw,
        effective: fresh.effective,
        label: fresh.label,
    }))
}
