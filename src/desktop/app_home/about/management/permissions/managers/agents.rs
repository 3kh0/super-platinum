//! App facts belong to this original manager query, never Activity's permit.
use super::*;
use protocol::manager_settings::eligibility::{Eligibility, Member};
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::app_eligibility::{self, agents as facts},
    slack::PreparedRequest,
};

pub(super) struct Attempt {
    generation: u64,
    query: String,
    rows: Vec<Value>,
    hydrate: bool,
    receipts: Vec<Value>,
    status: &'static str,
}
impl Attempt {
    pub(super) fn loading(&self, generation: u64) -> bool {
        self.generation == generation && self.status == "loading"
    }
    pub(super) fn retry(&self, generation: u64) -> bool {
        self.generation == generation && self.status == "failed"
    }
    pub(super) fn retry_rows(&self, generation: u64) -> Option<(Vec<Value>, bool)> {
        self.retry(generation)
            .then(|| (self.rows.clone(), self.hydrate))
    }
    pub(super) fn finish(&mut self, generation: u64, success: bool) {
        if self.loading(generation) {
            self.status = if success { "ready" } else { "failed" };
        }
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"query":self.query,"rows":self.rows,
            "hydrate":self.hydrate,"receipts":self.receipts,"status":self.status,
            "clientContext":true})
    }
}

pub(super) struct Acquired {
    generation: u64,
    rows: Vec<Value>,
    profiles: BTreeMap<String, facts::Profile>,
    access: Option<facts::Access>,
}
impl Acquired {
    pub(super) fn project(&self, generation: u64, team: &str) -> Result<Vec<source::Row>, Error> {
        if self.generation != generation {
            return Err(unavailable());
        }
        self.rows
            .iter()
            .map(|raw| {
                let member = Member::decode(raw)?;
                let profile =
                    app_eligibility::app_id(member.user()).and_then(|app| self.profiles.get(app));
                source::with_facts(raw, team, profile, self.access.as_ref()).ok_or_else(unavailable)
            })
            .collect()
    }
}

// Search and local seed can contain the same member. Last actual row wins,
// retaining the first occurrence's order, before any app fact is acquired.
pub(super) fn distinct(rows: Vec<Value>) -> Result<Vec<Value>, Error> {
    let mut out = vec![];
    let mut indices = BTreeMap::new();
    for raw in rows {
        let member = Member::decode(&raw)?;
        let id = member.user().id.clone();
        if let Some(index) = indices.get(&id).copied() {
            out[index] = raw;
        } else {
            indices.insert(id, out.len());
            out.push(raw);
        }
    }
    Ok(out)
}

async fn request(
    mut state: Signal<ShellState>,
    o: &Original,
    generation: u64,
    request: PreparedRequest,
) -> Result<Value, Error> {
    acquire::authorize(state, o, generation).await?;
    let result = if fixture::enabled(&state.peek()) {
        tokio::select! {
            r=crate::app_home::about::management::permissions::picker_read(state,&request,true)=>r.ok_or_else(unavailable)?,
            _=acquire::retired(state,o,generation)=>return Err(unavailable()),
        }
    } else if crate::fixture::is_fixture() {
        return Err(unavailable());
    } else {
        tokio::select! {
            r=o.owner.ctx.transport.execute(request)=>r,
            _=acquire::retired(state,o,generation)=>return Err(unavailable()),
        }
    };
    acquire::authorize(state, o, generation).await?;
    let raw = result?;
    let mut s = state.write();
    if !ready(&s, o, generation) {
        return Err(unavailable());
    }
    let attempt = owned_mut(&mut s)
        .and_then(|m| m.agent_facts.as_mut())
        .filter(|a| a.loading(generation))
        .ok_or_else(unavailable)?;
    // Keep an actual response before decoding; malformed replies are evidence,
    // not permission. Transport errors never manufacture a raw response.
    attempt.receipts.push(raw.clone());
    Ok(raw)
}

pub(super) async fn acquire(
    mut state: Signal<ShellState>,
    o: &Original,
    generation: u64,
    rows: Vec<Value>,
    hydrate: bool,
) -> Result<Acquired, Error> {
    // Preserve the existing picker intake: rows without a usable decoded
    // member/label are skipped, and cannot cause an app-profile request.
    let rows = distinct(
        rows.into_iter()
            .filter(|raw| source::row(raw, &o.owner.ctx.session.team_id).is_some())
            .collect(),
    )?;
    let members = rows
        .iter()
        .map(Member::decode)
        .collect::<Result<Vec<_>, _>>()?;
    let initial = members
        .iter()
        .map(|m| {
            m.eligibility(
                &o.owner.ctx.session.team_id,
                source::CLIENT_CONTEXT,
                None,
                None,
            )
        })
        .collect::<Vec<_>>();
    let needed = initial.iter().any(|v| {
        matches!(
            v,
            Eligibility::NeedsProfile { .. }
                | Eligibility::NeedsAgentAccess
                | Eligibility::NeedsClientContext
        )
    });
    if !ready(&state.peek(), o, generation) {
        return Err(unavailable());
    }
    if needed {
        let mut s = state.write();
        if !ready(&s, o, generation) {
            return Err(unavailable());
        }
        let m = owned_mut(&mut s).ok_or_else(unavailable)?;
        m.agent_facts = Some(Attempt {
            generation,
            query: m.query.clone(),
            rows: rows.clone(),
            hydrate,
            receipts: vec![],
            status: "loading",
        });
    }
    let ctx = &o.owner.ctx;
    let mut profiles = BTreeMap::new();
    let mut access_parts = None;
    if initial.iter().any(|v| *v == Eligibility::NeedsAgentAccess) {
        let boot = request(
            state,
            o,
            generation,
            facts::boot_request(&ctx.client, &ctx.session),
        )
        .await?;
        let (current, enterprise) = facts::boot(&ctx.session, &boot)?;
        let prefs = request(
            state,
            o,
            generation,
            facts::preferences_request(&ctx.client, &ctx.session),
        )
        .await?;
        let org = if enterprise {
            Some(
                request(
                    state,
                    o,
                    generation,
                    facts::enterprise_request(&ctx.client, &ctx.session),
                )
                .await?,
            )
        } else {
            None
        };
        let settings = facts::settings(&prefs, org.as_ref())?;
        let assignments = request(
            state,
            o,
            generation,
            super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
        )
        .await?;
        let mode = facts::list_mode(&assignments, &current)?;
        let mut accessible = BTreeSet::new();
        if mode == facts::ListMode::Fetch {
            let mut cursor = None;
            let mut seen = BTreeSet::new();
            let mut complete = false;
            for _ in 0..50 {
                let raw = request(
                    state,
                    o,
                    generation,
                    facts::list_request(&ctx.client, &ctx.session, cursor.as_deref())?,
                )
                .await?;
                let page = protocol::manager_settings::eligibility::decode_page(&raw)?;
                accessible.extend(page.accessible);
                for profile in page.profiles {
                    profiles.insert(profile.app().to_owned(), profile);
                }
                match page.cursor {
                    None => {
                        complete = true;
                        break;
                    }
                    Some(next) => {
                        if !seen.insert(next.clone()) {
                            return Err(unavailable());
                        }
                        cursor = Some(next);
                    }
                }
            }
            if !complete {
                return Err(unavailable());
            }
        }
        access_parts = Some((settings, accessible));
    }
    let apps = members
        .iter()
        .zip(&initial)
        .filter(|(_, e)| {
            matches!(
                e,
                Eligibility::NeedsProfile { .. } | Eligibility::NeedsAgentAccess
            )
        })
        .filter_map(|(m, _)| app_eligibility::app_id(m.user()))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    for app in apps {
        if profiles.contains_key(&app) {
            continue;
        }
        let raw = request(
            state,
            o,
            generation,
            app_eligibility::profile_request(&ctx.client, &ctx.session, &app)?,
        )
        .await?;
        profiles.insert(
            app.clone(),
            protocol::manager_settings::eligibility::decode_profile(&app, &raw)?,
        );
    }
    let access =
        access_parts.map(|(settings, ids)| facts::Access::new(settings, ids, profiles.clone()));
    if !ready(&state.peek(), o, generation) {
        return Err(unavailable());
    }
    Ok(Acquired {
        generation,
        rows,
        profiles,
        access,
    })
}

#[cfg(test)]
mod tests;
