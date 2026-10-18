//! Native date/time custody is kept in the actual HTML input, not inferred from
//! an empty .value. Only complete owner-bound snapshots enter typed form state.
use super::*;
use serde::{Deserialize, Serialize};
use super_platinum_core::block_actions::form::{InputKind, parse_datetime_local};
mod lease;
#[cfg(test)]
pub(crate) use lease::test_pair;
pub(crate) use lease::{Release, lock_owned, retain};
#[derive(Default)]
pub(crate) struct State {
    entries: std::collections::BTreeMap<(FormOwner, InputKey), Entry>,
}
struct Entry {
    id: u64,
    mount: u64,
    seq: u64,
    pending: bool,
    invalid: bool,
    value: String,
}
#[derive(Deserialize)]
struct Snapshot {
    mount: u64,
    seq: u64,
    kind: String,
    value: String,
    valid: bool,
    bad: bool,
}
#[derive(Clone, Serialize)]
struct Expected {
    id: u64,
    mount: u64,
    seq: u64,
    value: String,
}
fn calendar(kind: &InputKind) -> bool {
    matches!(
        kind,
        InputKind::Date | InputKind::Time { .. } | InputKind::DateTime
    )
}
pub(super) fn ready(s: &ShellState, owner: &FormOwner, form: &HomeForm) -> bool {
    form.fields
        .iter()
        .filter(|(_, f)| calendar(&f.kind))
        .all(|(key, _)| {
            s.app_home
                .native
                .entries
                .get(&(owner.clone(), key.clone()))
                .is_some_and(|e| !e.pending && !e.invalid)
        })
}
fn typed(kind: &InputKind, value: &str) -> Result<InputValue, String> {
    Ok(match kind {
        InputKind::Date => InputValue::Date((!value.is_empty()).then(|| value.to_owned())),
        InputKind::Time { .. } => InputValue::Time((!value.is_empty()).then(|| value.to_owned())),
        InputKind::DateTime => InputValue::DateTime(parse_datetime_local(value)?),
        _ => return Err("Not a Home calendar field".into()),
    })
}
fn receive(
    s: &mut ShellState,
    owner: &FormOwner,
    key: &InputKey,
    mount: u64,
    event: &Snapshot,
) -> bool {
    if owner.epoch != s.core.account_epoch
        || event.mount != mount
        || event.value.len() > 128
        || !matches!(event.kind.as_str(), "mounted" | "input" | "closed")
    {
        return false;
    }
    let index = (owner.clone(), key.clone());
    let Some(entry) = s.app_home.native.entries.get_mut(&index) else {
        return false;
    };
    if entry.mount != mount || event.seq != entry.seq + 1 {
        return false;
    }
    let Some(form) = s.app_home.forms.get_mut(owner) else {
        return false;
    };
    let Some(field) = form.fields.get(key) else {
        return false;
    };
    let value = typed(&field.kind, &event.value);
    entry.seq = event.seq;
    entry.pending = false;
    entry.value = event.value.clone();
    entry.invalid = !event.valid || event.bad || value.is_err();
    if entry.invalid {
        let _=form.set_native_error(key,String::new(),"This native date or time is not confirmed. Complete its picker, or explicitly reset the inputs; app actions remain blocked.".into());
    } else if let Ok(value) = value {
        if event.kind == "input" || field.value != value || form.native_error_text(key).is_some() {
            let _ = form.set(key, value);
        }
    }
    let release = event.kind == "closed" && !entry.invalid;
    if event.kind == "closed" {
        entry.pending = true;
    }
    bump(s, owner);
    if release {
        s.app_home.native.entries.remove(&index);
    }
    release
}
#[component]
pub(crate) fn Control(
    state: Signal<ShellState>,
    visit: actions::Key,
    input_key: InputKey,
    kind: InputKind,
    initial: String,
    id: String,
    label: String,
    description: String,
    required: String,
    invalid: String,
    locked: bool,
) -> Element {
    let mount = use_hook(next);
    let host = format!("home-native-calendar-{mount}");
    let native_type = match kind {
        InputKind::Date => "date",
        InputKind::Time { .. } => "time",
        _ => "datetime-local",
    };
    let mounted_host = host.clone();
    rsx! {span{id:"{host}",style:"display:contents","data-home-native-mount":mount,
        "data-native-id":id,"data-label":label,"data-description":description,"data-required":required,"data-invalid":invalid,"data-locked":locked.to_string(),
        onmounted:move |_|{let input_key=input_key.clone();let host=mounted_host.clone();let initial=initial.clone();
            dioxus::core::spawn_forever(async move {mount_control(state,visit,input_key,mount,host,native_type,initial).await;});}
    }}
}
async fn mount_control(
    mut state: Signal<ShellState>,
    visit: actions::Key,
    key: InputKey,
    mount: u64,
    host: String,
    native_type: &str,
    initial: String,
) {
    let captured = {
        let mut s = state.write();
        if !snapshot_current(&s, visit) {
            return;
        }
        let Some(owner) = owner(&s) else { return };
        let pristine = s
            .app_home
            .forms
            .get(&owner)
            .and_then(|f| f.fields.get(&key))
            .is_some_and(|f| !f.dirty);
        let index = (owner.clone(), key.clone());
        let epoch = s.core.account_epoch;
        s.app_home
            .native
            .entries
            .retain(|(owner, _), _| owner.epoch == epoch);
        if !s.app_home.native.entries.contains_key(&index) && s.app_home.native.entries.len() >= 256
        {
            return;
        }
        let e = s
            .app_home
            .native
            .entries
            .entry(index)
            .or_insert_with(|| Entry {
                id: next(),
                mount,
                seq: 0,
                pending: true,
                invalid: false,
                value: initial.clone(),
            });
        let retained = e.invalid;
        e.mount = mount;
        e.seq = 0;
        e.pending = true;
        let config = serde_json::json!({"host":host,"id":e.id,"epoch":owner.epoch,"mount":mount,"type":native_type,"initial":initial,"retained":retained,"pristine":pristine});
        (owner, config)
    };
    let (owner, config) = captured;
    let script = format!(
        "{}\nconst control=homeCalendarNative({},dioxus);if(!control){{dioxus.send({{mount:{mount},seq:1,kind:'closed',value:'',valid:false,bad:true}});await dioxus.recv();}}else{{for(;;){{const ack=await dioxus.recv();control.ack(ack);if(ack.finish)break;}}}}",
        include_str!("native.js"),
        config
    );
    let mut eval = dioxus::document::eval(&script);
    while let Ok(event) = eval.recv::<Snapshot>().await {
        let closed = event.kind == "closed";
        let release = receive(&mut state.write(), &owner, &key, mount, &event);
        let _ = eval.send(
            serde_json::json!({"mount":mount,"seq":event.seq,"release":release,"finish":closed}),
        );
        if closed {
            break;
        }
    }
}
/// Event streams are not final write authority. Re-read/lock the exact current
/// mounted controls before the once-only transport invocation.
pub(crate) async fn lock(
    state: Signal<ShellState>,
    visit: actions::Key,
) -> Option<dioxus::document::Eval> {
    let expected = {
        let s = state.peek();
        if !snapshot_current(&s, visit) || !super::ready(&s) {
            return None;
        }
        let owner = owner(&s)?;
        let form = s.app_home.forms.get(&owner)?;
        let mut out = Vec::new();
        for (key, f) in &form.fields {
            if calendar(&f.kind) {
                let e = s
                    .app_home
                    .native
                    .entries
                    .get(&(owner.clone(), key.clone()))?;
                out.push(Expected {
                    id: e.id,
                    mount: e.mount,
                    seq: e.seq,
                    value: e.value.clone(),
                });
            }
        }
        out
    };
    let script = format!(
        "{}\nconst release=lockHomeCalendars({});dioxus.send(!!release);await dioxus.recv();release?.();",
        include_str!("native.js"),
        serde_json::to_string(&expected).ok()?
    );
    let mut eval = dioxus::document::eval(&script);
    if eval.recv::<bool>().await.ok() == Some(true) {
        Some(eval)
    } else {
        let _ = eval.send(());
        None
    }
}
pub(super) fn reset(s: &mut ShellState, owner: &FormOwner) {
    let ids = s
        .app_home
        .native
        .entries
        .iter()
        .filter(|((o, _), _)| o == owner)
        .map(|(_, e)| e.id)
        .collect::<Vec<_>>();
    s.app_home.native.entries.retain(|(o, _), _| o != owner);
    let script = format!(
        "const book=window.__spHomeCalendarNative;for(const id of {}){{const e=book?.entries.get(id);if(e){{e.stop?.();book.entries.delete(id);}}}}",
        serde_json::to_string(&ids).unwrap()
    );
    dioxus::document::eval(&script);
}
#[cfg(test)]
mod tests;

// Unit-only bridge-state setup; not a DOM/native capture claim. Production
// fixture installation cannot bypass mount snapshots or final native leases.
#[cfg(test)]
pub(super) fn seed(s: &mut ShellState) {
    let owner = owner(s).unwrap();
    let fields = s.app_home.forms[&owner].fields.clone();
    for (key, field) in fields {
        if calendar(&field.kind) {
            let value = match field.value {
                InputValue::Date(v) | InputValue::Time(v) => v.unwrap_or_default(),
                InputValue::DateTime(v) => {
                    super_platinum_core::block_actions::form::datetime_local_value(v)
                }
                _ => unreachable!(),
            };
            s.app_home.native.entries.insert(
                (owner.clone(), key),
                Entry {
                    id: next(),
                    mount: next(),
                    seq: 1,
                    pending: false,
                    invalid: false,
                    value,
                },
            );
        }
    }
}
