//! Closed clock observations and held completed projection, never a label setter.
use super::*;
use std::{collections::BTreeMap, sync::OnceLock};
use super_platinum_core::state::directory_channels::inline_label::{self, Policy};

pub(crate) const START_SECONDS: u64 = 1_772_643_600;
pub(crate) const UTC_END: i64 = 1_773_075_600_000;
pub(crate) const EASTERN_END: i64 = 1_773_072_000_000;
pub(crate) const BETWEEN: i64 = 1_773_073_800_000;
#[derive(Clone, Copy, serde::Serialize)]
pub(crate) struct Sample {
    pub now_ms: i64,
    pub eastern: bool,
}
struct Data {
    sample: Sample,
    hold_next: bool,
    held: Option<oneshot::Sender<()>>,
    rows: Vec<Value>,
    changes: Vec<Value>,
}
static DATA: OnceLock<Mutex<Data>> = OnceLock::new();
fn data() -> &'static Mutex<Data> {
    DATA.get_or_init(|| {
        Mutex::new(Data {
            sample: Sample {
                now_ms: UTC_END - 1,
                eastern: false,
            },
            hold_next: false,
            held: None,
            rows: vec![],
            changes: vec![],
        })
    })
}
pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_ACTIVITY_GRACE_CLOCK_CHECK").as_deref() == Ok("1")
        && super::super::case() == "channel-frozen-grace"
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    requested() && shared_frozen::enabled(s)
}
pub(crate) fn sample(s: &ShellState) -> Option<Sample> {
    enabled(s).then(|| data().lock().unwrap().sample)
}
pub(crate) fn project(
    row: &Value,
    plan: Option<&Value>,
    sample: Option<Sample>,
) -> Result<Policy, inline_label::Error> {
    if let Some(sample) = sample {
        #[cfg(feature = "protected-native-fixture")]
        {
            return inline_label::test_clock::project_at(row, plan, sample.now_ms, sample.eastern);
        }
        #[cfg(not(feature = "protected-native-fixture"))]
        {
            let _ = sample;
            return Err(inline_label::Error::UnsupportedLocalTime);
        }
    }
    inline_label::project_local(row, plan)
}
pub(crate) fn set(s: &ShellState, now_ms: i64, eastern: bool, hold: bool) -> Result<(), String> {
    if !enabled(s) {
        return Err("Original grace clock fixture required".into());
    }
    let mut d = data().lock().unwrap();
    if d.held.is_some() && hold {
        return Err("A completed projection is already held".into());
    }
    d.sample = Sample { now_ms, eastern };
    d.hold_next = hold;
    d.changes
        .push(json!({"nowMs":now_ms,"eastern":eastern,"holdNext":hold}));
    Ok(())
}
pub(crate) struct Completion {
    pub id: Option<usize>,
    pub wait: Option<oneshot::Receiver<()>>,
}
pub(crate) fn completed(
    s: &ShellState,
    generation: u64,
    sample: Option<Sample>,
    policies: &BTreeMap<String, Policy>,
) -> Completion {
    if !enabled(s) {
        return Completion {
            id: None,
            wait: None,
        };
    }
    let mut d = data().lock().unwrap();
    let id = d.rows.len();
    let hold = d.hold_next;
    d.hold_next = false;
    let policies:Vec<_>=policies.iter().map(|(id,p)|json!({"id":id,"nowMs":p.captured_now_ms,
        "graceEndMs":p.grace_end_ms,"inGrace":p.in_grace,"archiveIcon":p.archive_icon,"archivedSuffix":p.archived_suffix})).collect();
    d.rows.push(json!({"id":id,"generation":generation,"sample":sample,"policies":policies,"held":hold,"finished":false,"applied":false}));
    let wait = if hold {
        let (tx, rx) = oneshot::channel();
        d.held = Some(tx);
        Some(rx)
    } else {
        None
    };
    Completion { id: Some(id), wait }
}
pub(crate) fn settled(id: Option<usize>, applied: bool) {
    let Some(id) = id else { return };
    let mut d = data().lock().unwrap();
    d.rows[id]["finished"] = json!(true);
    d.rows[id]["applied"] = json!(applied);
}
pub(crate) fn release(s: &ShellState) -> Result<(), String> {
    if !enabled(s) {
        return Err("Original grace clock fixture required".into());
    }
    data()
        .lock()
        .unwrap()
        .held
        .take()
        .ok_or("No held projection")?
        .send(())
        .map_err(|_| "Held projection receiver retired".into())
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    if !enabled(s) {
        return Value::Null;
    }
    let (sample, changes, projections, held) = {
        let d = data().lock().unwrap();
        (
            d.sample,
            d.changes.clone(),
            d.rows.clone(),
            d.held.is_some(),
        )
    };
    let labels = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .map(|h| super::super::super::channel_labels::snapshot(s, h.id));
    json!({"sample":sample,"changes":changes,"projections":projections,"held":held,"labels":labels,
        "startSeconds":START_SECONDS,"utcEnd":UTC_END,"easternEnd":EASTERN_END})
}
