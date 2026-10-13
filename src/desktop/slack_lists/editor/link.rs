//! A closed editor must not free the CPU budget while its detector still runs.
use super::CellEditValue;
use serde_json::Value;
use std::sync::{Arc, LazyLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// B18175469: explicit nonempty name, otherwise attachment[originalUrl].title
/// ?? fallback. Capture only at editor admission; this is a draft seed, never a
/// change to the canonical LinkValue used by display toggles or stale checks.
pub(super) fn name_seed(original: &CellEditValue, stored: Option<&Value>) -> String {
    let CellEditValue::Link(Some(link)) = original else {
        return String::new();
    };
    if let Some(name) = link.display_name.as_ref().filter(|name| !name.is_empty()) {
        return name.clone();
    }
    let item = stored.and_then(|stored| match stored {
        Value::Object(_) => Some(stored),
        Value::Array(items) if items.len() == 1 => items.first(),
        _ => None,
    });
    let attachment = item
        .and_then(|item| item.get("attachment"))
        .and_then(|attachments| attachments.get(&link.original_url));
    let title = attachment.and_then(|attachment| {
        attachment
            .get("title")
            .filter(|title| !title.is_null())
            .or_else(|| attachment.get("fallback"))
    });
    title.and_then(Value::as_str).unwrap_or_default().to_owned()
}

static NORMALIZATION: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(1)));

pub(crate) async fn normalization_slot() -> OwnedSemaphorePermit {
    NORMALIZATION
        .clone()
        .acquire_owned()
        .await
        .expect("normalization semaphore is never closed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::slack_lists::LinkValue;

    #[test]
    fn name_seed_preserves_explicit_names_and_nullish_attachment_precedence() {
        let mut value = LinkValue {
            original_url: "https://example.com/spec".into(),
            display_name: None,
            display_as_url: true,
        };
        for (entry, expected) in [
            (json!({"title":"Title","fallback":"Fallback"}), "Title"),
            (json!({"title":"","fallback":"Fallback"}), ""),
            (json!({"title":null,"fallback":"Fallback"}), "Fallback"),
            (json!({"fallback":"Fallback"}), "Fallback"),
            (json!({"title":{"unknown":true},"fallback":"Fallback"}), ""),
            (json!({"title":42}), ""),
            (Value::Null, ""),
        ] {
            let stored = json!({"original_url":value.original_url,"attachment":{
                "https://example.com/spec":entry,"https://other.invalid":{"title":"Wrong URL"}
            }});
            let mut camel = stored.clone();
            camel.as_object_mut().unwrap().remove("original_url");
            camel["originalUrl"] = json!(value.original_url);
            for stored in [
                stored.clone(),
                json!([stored]),
                camel.clone(),
                json!([camel]),
            ] {
                assert_eq!(
                    name_seed(&CellEditValue::Link(Some(value.clone())), Some(&stored)),
                    expected
                );
            }
        }
        let stored = json!({"attachment":{"https://other.invalid":{"title":"Wrong URL"}}});
        assert_eq!(
            name_seed(&CellEditValue::Link(Some(value.clone())), Some(&stored)),
            ""
        );
        for name in ["Explicit", "   "] {
            value.display_name = Some(name.into());
            assert_eq!(
                name_seed(
                    &CellEditValue::Link(Some(value.clone())),
                    Some(&json!({
                        "attachment":{"https://example.com/spec":{"title":"Attachment"}}
                    }))
                ),
                name
            );
        }
    }

    #[tokio::test]
    async fn cancelled_waiter_does_not_release_a_running_normalizers_budget() {
        let semaphore = Arc::new(Semaphore::new(1));
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let (started, start) = tokio::sync::oneshot::channel();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _ = started.send(());
            wait.recv().unwrap();
            drop(_permit);
            let _ = finished.send(());
        });
        start.await.unwrap();
        task.abort();
        assert!(semaphore.try_acquire().is_err());
        release.send(()).unwrap();
        let _ = done.await;
        assert!(semaphore.try_acquire().is_ok());
    }
}
