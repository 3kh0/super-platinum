//! Activity-only channel presentation. Timezone work runs outside the renderer;
//! source receipts never authorize navigation or mutate a workflow draft.
use super::*;
use crate::{
    app_home::channel_fetches::{canonical, plan_info},
    icons::Icon,
};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use super_platinum_core::state::directory_channels::inline_label::{self, Policy};

mod clock;

#[derive(Clone, Default)]
pub(super) struct State {
    clock: clock::Clock,
    pending: Option<Arc<Input>>,
    observed: Option<Arc<Observation>>,
}
struct Input {
    generation: u64,
    canonical: Arc<canonical::Canonical>,
    plan: Option<plan_info::Display>,
    team: String,
    visit: u64,
    document: u64,
    detail: u64,
    rows: BTreeMap<String, Value>,
}
struct Observation {
    input: Arc<Input>,
    policies: BTreeMap<String, Policy>,
}
impl Input {
    fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !permitted(s, visit) {
            return None;
        }
        let d = document(s, visit).filter(|d| d.activity.active)?;
        let detail = d.activity.detail.as_ref()?;
        let team = d.team.clone();
        let canonical = canonical::observed(s, &team).ok()?;
        let messages = detail
            .trigger_message()
            .into_iter()
            .chain(detail.steps().iter().map(|step| &step["outcome_message"]));
        let mut rows = BTreeMap::new();
        for message in messages {
            for part in presentation::parts(message) {
                if let presentation::Part::Channel(id) = part
                    && let Some(row) = canonical.row(&id)
                    // Shared status never proves a foreign-team label. Only
                    // the accepted ordinary-client canonical projection does.
                    && row["fromAnotherTeam"] == false
                {
                    rows.insert(id, row.clone());
                }
            }
        }
        if rows.is_empty() {
            return None;
        }
        Some(Self {
            generation: d.activity.channel_labels.clock.generation(),
            canonical,
            plan: plan_info::display(s, &team),
            team,
            visit,
            document: d.id,
            detail: d.activity.detail_revision,
            rows,
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        permitted(s, self.visit)
            && self.canonical.current_for(s, &self.team)
            && match &self.plan {
                Some(plan) => plan.current(s),
                None => plan_info::display(s, &self.team).is_none(),
            }
            && document(s, self.visit).is_some_and(|d| {
                d.id == self.document
                    && d.team == self.team
                    && d.activity.active
                    && d.activity.detail.is_some()
                    && d.activity.detail_revision == self.detail
            })
    }
}

/// Called by an existing renderer queue_effect after releasing its shell borrow.
/// At most one task belongs to a current detail/source observation.
pub(super) fn request(mut state: Signal<ShellState>, visit: u64) {
    let input = {
        let s = state.peek();
        let Some(d) = document(&s, visit) else { return };
        let labels = &d.activity.channel_labels;
        // Even a superseded computation remains the sole in-flight worker.
        if labels.pending.as_ref().is_some_and(|v| v.current(&s))
            || labels.observed.as_ref().is_some_and(|v| {
                v.input.current(&s) && v.input.generation == labels.clock.generation()
            })
        {
            return;
        }
        let Some(input) = Input::capture(&s, visit) else {
            return;
        };
        Arc::new(input)
    };
    {
        #[allow(deprecated)]
        let mut s = state.write_silent();
        let labels = &mut document_mut(&mut s, visit).unwrap().activity.channel_labels;
        labels.clock.started(Instant::now());
        labels.pending = Some(input.clone());
    }
    let injected = fixture::channel::grace_clock::sample(&state.peek());
    dioxus::core::spawn_forever(async move {
        let raw_rows = input.rows.clone();
        let raw_plan = input.plan.as_ref().map(|v| v.raw().clone());
        let mut policies: BTreeMap<String, Policy> = tokio::task::spawn_blocking(move || {
            raw_rows
                .into_iter()
                .filter_map(|(id, row)| {
                    fixture::channel::grace_clock::project(&row, raw_plan.as_ref(), injected)
                        .ok()
                        .map(|p| (id, p))
                })
                .collect()
        })
        .await
        .unwrap_or_default();
        let receipt = fixture::channel::grace_clock::completed(
            &state.peek(),
            input.generation,
            injected,
            &policies,
        );
        if let Some(wait) = receipt.wait {
            let _ = wait.await;
        }
        let mut applied = false;
        let mut notify = false;
        {
            #[allow(deprecated)]
            let mut s = state.write_silent();
            if input.current(&s) {
                let now = now_ms(&s);
                let previous_current = observed(&s, visit).is_some();
                let labels = &mut document_mut(&mut s, visit).unwrap().activity.channel_labels;
                if labels
                    .pending
                    .as_ref()
                    .is_some_and(|v| Arc::ptr_eq(v, &input))
                {
                    labels.pending = None;
                    labels.clock.finished(input.generation, Instant::now());
                    if labels.clock.generation() == input.generation {
                        // Keep stable original provenance when the calendar result is unchanged.
                        if let Some(old) = &labels.observed {
                            for (id, p) in &mut policies {
                                if let Some(previous) = old.policies.get(id) {
                                    if let Ok(at_previous) = p.at_millis(previous.captured_now_ms) {
                                        if (Policy {
                                            captured_now_ms: previous.captured_now_ms,
                                            ..at_previous
                                        }) == *previous
                                        {
                                            *p = *previous;
                                        }
                                    }
                                }
                            }
                        }
                        notify = !previous_current;
                        if let Some(now) = now {
                            notify |= labels.clock.update_face(&policies, now);
                        }
                        labels.observed = Some(Arc::new(Observation { input, policies }));
                        applied = true;
                    }
                }
            }
        }
        fixture::channel::grace_clock::settled(receipt.id, applied);
        if notify {
            drop(state.write());
        }
    });
}

/// Existing runtime heartbeat, with no additional timer or timezone work here.
/// Only a mounted, owned grace-capable detail schedules background sampling.
pub(crate) fn tick(mut state: Signal<ShellState>) {
    let visit =
        {
            let s = state.peek();
            let Some(h) = s.app_home.about.management.surface.as_ref() else {
                return;
            };
            let Some(d) = document(&s, h.id).filter(|d| d.activity.active) else {
                return;
            };
            let labels = &d.activity.channel_labels;
            if !permitted(&s, h.id) || d.activity.detail.is_none() {
                return;
            }
            if labels.observed.as_ref().is_none_or(|o| {
                !o.input.current(&s) || !o.policies.values().any(|p| p.grace_eligible)
            }) {
                return;
            }
            h.id
        };
    let mut notify = false;
    {
        #[allow(deprecated)]
        let mut s = state.write_silent();
        let now = now_ms(&s);
        let labels = &mut document_mut(&mut s, visit).unwrap().activity.channel_labels;
        labels
            .clock
            .advance(Instant::now(), labels.pending.is_some());
        if let (Some(now), Some(observed)) = (now, &labels.observed) {
            notify = labels.clock.update_face(&observed.policies, now);
        }
    }
    if notify {
        drop(state.write());
    }
    request(state, visit);
}

fn now_ms(s: &ShellState) -> Option<i64> {
    if let Some(sample) = fixture::channel::grace_clock::sample(s) {
        return Some(sample.now_ms);
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}
fn observed(s: &ShellState, visit: u64) -> Option<&Observation> {
    let observation = document(s, visit)?
        .activity
        .channel_labels
        .observed
        .as_ref()?;
    observation.input.current(s).then_some(observation)
}
pub(super) fn presentation(s: &ShellState, visit: u64, id: &str) -> Option<(String, Option<Icon>)> {
    let observation = observed(s, visit)?;
    let policy = observation.policies.get(id)?.at_millis(now_ms(s)?).ok()?;
    let name = observation.input.rows.get(id)?["name"].as_str()?;
    let icon = match (policy.archive_icon, policy.private) {
        (true, true) => Icon::ArchiveFilled,
        (true, false) => Icon::Archive,
        (false, true) => Icon::LockFilled,
        (false, false) => Icon::Tag,
    };
    let label = if policy.archived_suffix {
        format!("{name} (archived)")
    } else {
        name.into()
    };
    Some((label, Some(icon)))
}

/// Passive evidence for the closed offline interaction harness.
pub(crate) fn snapshot(s: &ShellState, visit: u64) -> Value {
    let Some(observation) = observed(s, visit) else {
        return json!({"current":false});
    };
    let rows: Vec<_> = observation
        .input
        .rows
        .iter()
        .map(|(id, raw)| {
            let policy = observation.policies.get(id);
            json!({"id":id,"raw":raw,"policy":policy.map(|p| json!({
                "private":p.private,"archiveIcon":p.archive_icon,"archivedSuffix":p.archived_suffix,
                "inGrace":p.in_grace,"capturedNowMs":p.captured_now_ms,"graceEndMs":p.grace_end_ms
            }))})
        })
        .collect();
    let mut result =
        json!({"current":true,"plan":observation.input.plan.as_ref().map(|v|v.raw()),"rows":rows});
    if fixture::channel::warm::enabled(s) {
        result["planProvenance"] = json!(observation.input.plan.as_ref().map(|v| v.provenance()));
    }
    if fixture::channel::grace_clock::enabled(s) {
        result["clockGeneration"] = json!(
            document(s, visit)
                .unwrap()
                .activity
                .channel_labels
                .clock
                .generation()
        );
        result["observedGeneration"] = json!(observation.input.generation);
    }
    result
}
