//! Activity includeBots=false/includeAgents=true observations. Home's more
//! permissive bot policy is deliberately not reused for these member choices.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::block_actions::{
    app_eligibility::{self, agents as facts},
    workflow_management::activity_members::{Eligibility, Member},
};
#[cfg(test)]
mod tests;

pub(super) struct Acquired {
    generation: u64,
    rows: Vec<Value>,
    indices: BTreeMap<String, usize>,
    profiles: BTreeMap<String, facts::Profile>,
    access: Option<facts::Access>,
    /// Exact response bodies, including skipped-list evidence, stay separate
    /// from permission observations. No raw response is exposed in the UI.
    receipts: Vec<Value>,
}
impl Acquired {
    pub(super) fn current(&self, s: &ShellState, q: &Query, rows: &[Value]) -> bool {
        self.generation == q.generation && self.rows == rows && q.current(s)
    }
    pub(super) fn allowed(&self, q: &Query, raw: &Value) -> bool {
        if self.generation != q.generation {
            return false;
        }
        let Some(index) = raw["id"].as_str().and_then(|id| self.indices.get(id)) else {
            return false;
        };
        if self.rows.get(*index) != Some(raw) {
            return false;
        }
        Member::decode(raw).is_ok_and(|member| {
            let profile =
                app_eligibility::app_id(member.user()).and_then(|app| self.profiles.get(app));
            member.eligibility(
                &q.owner.ctx.session.user_id,
                Some(true),
                profile,
                self.access.as_ref(),
            ) == Eligibility::Allowed
        })
    }
}

pub(super) async fn acquire(
    state: Signal<ShellState>,
    q: &Query,
    rows: &[Value],
) -> Result<Acquired, Error> {
    if !q.current(&state.peek()) {
        return Err(unavailable());
    }
    let members = rows
        .iter()
        .map(Member::decode)
        .collect::<Result<Vec<_>, _>>()?;
    let mut indices = BTreeMap::new();
    for (index, member) in members.iter().enumerate() {
        if indices.insert(member.user().id.clone(), index).is_some() {
            return Err(unavailable());
        }
    }
    // XSM4.TZ is isClientApp, not unified-user mode. This owned desktop
    // consumer is the native client context, as in the reviewed Calendar lane.
    // An enterprise ID, auth-map mode, or app association cannot set this fact.
    let needs_access = members.iter().any(|m| {
        m.eligibility(&q.owner.ctx.session.user_id, Some(true), None, None)
            == Eligibility::NeedsAgentAccess
    });
    let ctx = &q.owner.ctx;
    let mut receipts = Vec::new();
    let mut profiles = BTreeMap::new();
    let mut access_parts = None;
    if needs_access {
        let boot = read::request(state, q, facts::boot_request(&ctx.client, &ctx.session)).await?;
        let (current, enterprise) = facts::boot(&ctx.session, &boot)?;
        receipts.push(boot);
        let prefs = read::request(
            state,
            q,
            facts::preferences_request(&ctx.client, &ctx.session),
        )
        .await?;
        let org = if enterprise {
            Some(
                read::request(
                    state,
                    q,
                    facts::enterprise_request(&ctx.client, &ctx.session),
                )
                .await?,
            )
        } else {
            None
        };
        let settings = facts::settings(&prefs, org.as_ref())?;
        receipts.push(prefs);
        receipts.extend(org);
        let assignments = read::request(
            state,
            q,
            super_platinum_core::slack::api::list_experiments(&ctx.client, &ctx.session),
        )
        .await?;
        let mode = facts::list_mode(&assignments, &current)?;
        receipts.push(assignments);
        let mut accessible = BTreeSet::new();
        if mode == facts::ListMode::Fetch {
            let mut cursor = None;
            let mut seen = BTreeSet::new();
            let mut complete = false;
            for _ in 0..50 {
                let raw = read::request(
                    state,
                    q,
                    facts::list_request(&ctx.client, &ctx.session, cursor.as_deref())?,
                )
                .await?;
                let page = facts::Page::decode(&raw)?;
                receipts.push(raw);
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
    // Fetch only profiles whose actual Activity eligibility needs them, or
    // Agentforce profiles needed for the independently observed default-agent
    // exception. Source-denied ordinary members cause no app-profile request.
    let apps = members
        .iter()
        .filter(|m| {
            matches!(
                m.eligibility(&ctx.session.user_id, Some(true), None, None),
                Eligibility::NeedsProfile { .. } | Eligibility::NeedsAgentAccess
            )
        })
        .filter_map(|m| app_eligibility::app_id(m.user()))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    for app in apps {
        if profiles.contains_key(&app) {
            continue;
        }
        let raw = read::request(
            state,
            q,
            app_eligibility::profile_request(&ctx.client, &ctx.session, &app)?,
        )
        .await?;
        profiles.insert(app.clone(), facts::Profile::decode(&app, &raw)?);
        receipts.push(raw);
    }
    let access =
        access_parts.map(|(settings, ids)| facts::Access::new(settings, ids, profiles.clone()));
    let result = Acquired {
        generation: q.generation,
        rows: rows.to_vec(),
        indices,
        profiles,
        access,
        receipts,
    };
    if !result.current(&state.peek(), q, rows) {
        return Err(unavailable());
    }
    Ok(result)
}
