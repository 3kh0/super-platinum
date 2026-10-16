//! One transient, bounded capture reservation; no partial value publication.
use super::*;
use std::time::{Duration, Instant};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub id: String,
    pub epoch: String,
    pub column: String,
    pub operator: String,
    #[serde(deserialize_with = "required_value")]
    pub value: Option<String>,
    pub valid: bool,
}
fn required_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    <Option<String> as serde::Deserialize>::deserialize(d)
}
pub(crate) struct Capture {
    key: Key,
    pub token: u64,
    pub serial: u64,
    pub visit: u64,
    pub structure: u64,
    rows: Vec<Row>,
    input: ViewFilterChoice,
    review: owner::Review,
    deadline: Instant,
}
impl Drop for Capture {
    fn drop(&mut self) {
        with_registry(|r| {
            if let Some(e) = r.entries.get_mut(&self.key).filter(|e| {
                e.serial == self.serial && e.filter_capture.is_some_and(|(id, _)| id == self.token)
            }) {
                e.filter_capture = None;
            }
        });
    }
}
impl Capture {
    pub(crate) fn proof(&self) -> Value {
        serde_json::json!({"token":self.token.to_string(),"serial":self.serial.to_string(),"visit":self.visit.to_string(),"structure":self.structure.to_string(),"rows":self.rows.iter().zip(self.input.conditions()).map(|(r,v)|serde_json::json!({"id":r.id.to_string(),"epoch":r.epoch.to_string(),"column":v.column_id(),"operator":operator(v),"number":matches!(v,ViewFilterChoice::Number{operator,..} if operator.requires_value())})).collect::<Vec<_>>()})
    }
    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    fn current_at(&self, shell: &ShellState, now: Instant) -> bool {
        now < self.deadline
            && self.review.navigation_current(shell)
            && generation(shell) == Some(self.visit)
            && owner::available(shell)
            && !registry::blocked(&self.key.owner)
            && with_registry(|r| {
                r.entries.get(&self.key).is_some_and(|e| {
                    e.serial == self.serial
                        && e.stage == Stage::Editing
                        && e.filter_choice == self.structure
                        && e.filter_rows == self.rows
                        && matches!(&e.input,Input::Filter(v) if v==&self.input)
                        && e.filter_capture.is_some_and(|(id, deadline)| {
                            id == self.token && deadline == self.deadline
                        })
                })
            })
    }
}
pub(super) fn operator(v: &ViewFilterChoice) -> &'static str {
    match v {
        ViewFilterChoice::Checkbox { checked, .. } => {
            if *checked {
                "is_not_empty"
            } else {
                "is_empty"
            }
        }
        ViewFilterChoice::Number { operator, .. } => operator.as_str(),
        _ => "",
    }
}
pub(crate) fn reserve(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    structure: u64,
) -> Option<Capture> {
    let key = rows::ready(shell, serial, visit)?;
    let e = entry(shell)?;
    if e.filter_choice != structure {
        return None;
    }
    let Input::Filter(input) = e.input else {
        return None;
    };
    if input.conditions().len() != e.filter_rows.len() || input.conditions().len() > 8 {
        return None;
    }
    let review = owner::Review::capture(shell)?;
    let token = next_id();
    let deadline = Instant::now() + Duration::from_secs(5);
    with_registry(|r| r.entries.get_mut(&key).unwrap().filter_capture = Some((token, deadline)));
    Some(Capture {
        key,
        token,
        serial,
        visit,
        structure,
        rows: e.filter_rows,
        input,
        review,
        deadline,
    })
}
/// All values and policy validate before either the canonical draft or job changes.
pub(crate) fn finish(
    shell: &mut ShellState,
    capture: &Capture,
    snapshot: Vec<Snapshot>,
) -> Option<(Guard, Intent)> {
    finish_at(shell, capture, snapshot, Instant::now())
}
pub(crate) fn finish_at(
    shell: &mut ShellState,
    capture: &Capture,
    snapshot: Vec<Snapshot>,
    now: Instant,
) -> Option<(Guard, Intent)> {
    if !capture.current_at(shell, now) || snapshot.len() != capture.rows.len() {
        return None;
    }
    let mut desired = capture.input.conditions().to_vec();
    for ((r, value), seen) in capture.rows.iter().zip(desired.iter_mut()).zip(snapshot) {
        if !seen.valid
            || seen.id != r.id.to_string()
            || seen.epoch != r.epoch.to_string()
            || Some(seen.column.as_str()) != value.column_id()
            || seen.operator != operator(value)
        {
            return None;
        }
        match value {
            ViewFilterChoice::Number {
                operator, value, ..
            } => {
                if !operator.valid_value(seen.value.as_deref()) {
                    return None;
                }
                *value = seen.value;
            }
            ViewFilterChoice::Checkbox { .. } => {
                if seen.value.is_some() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    let mut old = entry(shell)?;
    old.input = Input::Filter(ViewFilterChoice::from_conditions(desired));
    let loaded = shell.slack_lists.view.as_ref()?.loaded.as_ref()?;
    if loaded.document != old.original {
        return None;
    }
    let intent = text_create::capture(shell, &old, loaded).ok()?;
    let guard = Guard::capture(shell, capture.key.clone(), capture.serial)?;
    if !capture.current_at(shell, now.max(Instant::now())) {
        return None;
    }
    with_registry(|r| {
        let e = r.entries.get_mut(&capture.key).unwrap();
        e.input = old.input;
        e.filter_capture = None;
        e.intent = Some(intent.clone());
        e.job = Some(guard.job);
        e.stage = Stage::Preparing;
        e.message = "Checking access and the current view...".into();
    });
    Some((guard, intent))
}
pub(crate) fn release(shell: &ShellState, capture: &Capture) -> bool {
    let editable = capture.review.navigation_current(shell)
        && owner::available(shell)
        && !registry::blocked(&capture.key.owner);
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&capture.key).filter(|e| {
            e.serial == capture.serial
                && e.filter_capture.is_some_and(|(id, _)| id == capture.token)
        }) else {
            return false;
        };
        e.filter_capture = None;
        let restore = editable
            && e.stage == Stage::Editing
            && e.filter_choice == capture.structure
            && e.filter_rows == capture.rows;
        if restore {
            e.message =
                "Couldn't capture every condition. Review the values and try Save again.".into();
        }
        restore
    })
}
