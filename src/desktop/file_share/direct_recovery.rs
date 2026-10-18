//! Exact returned-open storage custody, independent of the visible dialog.
use super::*;
use super_platinum_core::file_sharing::store::direct_open::{Record, Returned};

#[derive(Clone)]
pub(super) struct Held {
    identity: Arc<()>,
    storage: recovery::Storage,
    record: Record,
    raw: Option<Value>,
}
pub(super) fn reserve(
    custody: &Arc<std::sync::Mutex<recovery::Custody>>,
    storage: &recovery::Storage,
    record: &Record,
) -> Result<(), Refusal> {
    let mut held = custody.lock().map_err(|_| Refusal::Storage)?;
    if held.direct_results.len() >= 32 || held.direct_results.contains_key(record.id()) {
        return Err(Refusal::Capacity);
    }
    held.direct_results.insert(
        record.id().into(),
        Held {
            identity: Arc::new(()),
            storage: storage.clone(),
            record: record.clone(),
            raw: None,
        },
    );
    Ok(())
}
pub(super) async fn returned(
    custody: &Arc<std::sync::Mutex<recovery::Custody>>,
    record: &Record,
    result: Result<Value, Error>,
) -> Result<Record, Refusal> {
    let raw = match result {
        Ok(raw) if admission::bounded(&raw, 512 * 1024, 32) => raw,
        _ => {
            custody
                .lock()
                .map_err(|_| Refusal::Storage)?
                .direct_results
                .remove(record.id());
            return Err(Refusal::RecipientResolution);
        }
    };
    let captured = {
        let mut held = custody.lock().map_err(|_| Refusal::Storage)?;
        let entry = held
            .direct_results
            .get_mut(record.id())
            .ok_or(Refusal::Storage)?;
        if &entry.record != record || entry.raw.is_some() {
            return Err(Refusal::Changed);
        }
        entry.raw = Some(raw);
        entry.clone()
    };
    match persist(custody, captured).await? {
        Returned::Active(record) => Ok(record),
        Returned::Completed => Err(Refusal::Changed),
    }
}
async fn persist(
    custody: &Arc<std::sync::Mutex<recovery::Custody>>,
    held: Held,
) -> Result<Returned, Refusal> {
    let original = held.record.clone();
    let raw = held.raw.clone().ok_or(Refusal::Storage)?;
    let expected = original.clone();
    let saved = held
        .storage
        .run(move |j| j.settle_direct_returned(&expected, raw))
        .await?;
    let mut guard = custody.lock().map_err(|_| Refusal::Storage)?;
    if guard.direct_results.get(original.id()).is_some_and(|v| {
        Arc::ptr_eq(&v.identity, &held.identity) && v.record == original && v.raw == held.raw
    }) {
        guard.direct_results.remove(original.id());
    }
    Ok(saved)
}
pub(crate) fn retry_returned(mut state: Signal<ShellState>, id: String) {
    let captured = (|| {
        let shell = state.peek();
        let context = admission::Context::capture(&shell)?;
        let custody = shell.file_share.custody.clone();
        let held = custody.lock().ok()?.direct_results.get(&id)?.clone();
        (held.storage.key == context.key && held.raw.is_some()).then_some((context, custody, held))
    })();
    let Some((context, custody, held)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let result = persist(&custody, held).await;
        let mut shell = state.write();
        if context.current(&shell) {
            shell.show_toast(match result {
                Ok(Returned::Active(_))=>"The DM result is saved. Review the saved message to continue.",
                Ok(Returned::Completed)=>"The original share was already completed. This late result is saved with its history.",
                Err(_)=>"The DM result is still retained here. Review this copy before saving or discarding it.",
            });
        }
    });
}
pub(crate) fn retry_available(shell: &ShellState, id: &str) -> bool {
    let Some(context) = admission::Context::capture(shell) else {
        return false;
    };
    shell.file_share.custody.lock().ok().is_some_and(|h| {
        h.direct_results
            .get(id)
            .is_some_and(|v| v.storage.key == context.key && v.raw.is_some())
    })
}
mod retained;
pub(crate) use retained::{RetainedResult, discard as discard_retained, list as retained_results};

/// A complete literal im.list response is a read, not an open-result receipt.
/// Fresh conversations.info still verifies this unique candidate afterwards.
pub(super) fn candidate(raw: &Value, peer: &str) -> Result<String, Refusal> {
    if !admission::bounded(raw, 4 * 1024 * 1024, 32)
        || raw["ok"] != true
        || raw.get("error").is_some_and(|v| !v.is_null())
    {
        return Err(Refusal::RecipientResolution);
    }
    let rows = raw["ims"]
        .as_array()
        .filter(|v| v.len() <= 10_000)
        .ok_or(Refusal::RecipientResolution)?;
    let mut found = None;
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().ok_or(Refusal::RecipientResolution)?;
        if !ids.insert(id) {
            return Err(Refusal::RecipientResolution);
        }
        if row["user"] != peer {
            continue;
        }
        if row["is_im"] != true || row["is_open"] != true {
            return Err(Refusal::RecipientResolution);
        }
        let id = new_direct::opened_id(&serde_json::json!({"ok":true,"channel":row}))?;
        if found.replace(id).is_some() {
            return Err(Refusal::RecipientResolution);
        }
    }
    found.ok_or(Refusal::RecipientResolution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row(id: &str, user: &str) -> Value {
        json!({"id":id,"user":user,"is_im":true,"is_open":true})
    }
    #[test]
    fn readonly_lookup_admits_one_exact_open_peer_and_refuses_absent_or_ambiguous() {
        assert_eq!(
            candidate(
                &json!({"ok":true,"ims":[row("D00000001","U00000002")]}),
                "U00000002"
            )
            .unwrap(),
            "D00000001"
        );
        assert!(candidate(&json!({"ok":true,"ims":[]}), "U00000002").is_err());
        assert!(candidate(&json!({"ok":true,"ims":[row("D00000001","U00000002"),row("D00000002","U00000002")]}),"U00000002").is_err());
        assert!(
            candidate(
                &json!({"ok":true,"ims":[row("D00000001","U00000003")]}),
                "U00000002"
            )
            .is_err()
        );
    }
    #[test]
    fn lookup_cannot_default_closed_or_missing_peer_shape_to_an_open_dm() {
        let mut value = row("D00000001", "U00000002");
        value.as_object_mut().unwrap().remove("is_open");
        assert!(candidate(&json!({"ok":true,"ims":[value]}), "U00000002").is_err());
        assert!(
            candidate(
                &json!({"ok":false,"ims":[row("D00000001","U00000002")]}),
                "U00000002"
            )
            .is_err()
        );
        assert!(
            candidate(
                &json!({"ok":true,"ims":[row("C00000001","U00000002")]}),
                "U00000002"
            )
            .is_err()
        );
    }
}
