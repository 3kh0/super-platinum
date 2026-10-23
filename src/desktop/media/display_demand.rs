//! Register a finite emoji catalog without downloading every unused entry.
use super::*;

impl MediaRegistry {
    pub(crate) fn register_emoji_on_display(&self, name: &str, url: &str) -> MediaAssetId {
        let id = self.register_source_inner(
            MediaAssetKind::Emoji,
            url,
            "image/png".into(),
            is_slack_hosted(url),
            Some(format!("emoji/{name}")),
            true,
        );
        let mut sources = self.sources.write().expect("media registry poisoned");
        if let Some(source) = sources.get_mut(&id) {
            if source.deferred {
                source.demand_on_display = true;
            }
        }
        id
    }

    pub(super) fn request_on_display(&self, id: &MediaAssetId) {
        let mut sources = self.sources.write().expect("media registry poisoned");
        let Some(source) = sources.get_mut(id) else {
            return;
        };
        if !source.deferred || !source.demand_on_display {
            return;
        }
        source.deferred = false;
        source.demand_on_display = false;
        drop(sources);
        // The existing async sweep does the IO. Repeated image requests must
        // never reset an HTTP failure's backoff or retry a permanent failure.
        self.dirty.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(registry: &MediaRegistry, id: &MediaAssetId) {
        let response = registry.respond(Request::builder().uri(id.uri()).body(vec![]).unwrap());
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn unused_catalog_is_deferred_and_only_displayed_asset_is_requested() {
        let media = MediaRegistry::default();
        let reached = media.register_emoji_on_display("T1/first", "https://example.test/first.png");
        let unused =
            media.register_emoji_on_display("T1/second", "https://example.test/second.png");
        assert!(!media.has_pending());
        assert_eq!(
            media.sources.read().unwrap()[&reached].slot.as_deref(),
            Some("emoji/T1/first")
        );
        get(&media, &reached);
        assert!(media.has_pending());
        assert!(!media.sources.read().unwrap()[&reached].deferred);
        assert!(media.sources.read().unwrap()[&unused].deferred);
    }

    #[test]
    fn repeated_display_preserves_abandoned_backoff_and_other_deferred_sources() {
        let media = MediaRegistry::default();
        let emoji = media.register_emoji_on_display("T1/first", "https://example.test/first.png");
        get(&media, &emoji);
        media
            .backoff
            .write()
            .unwrap()
            .insert(emoji.clone(), Backoff::Abandoned);
        get(&media, &emoji);
        assert!(!media.has_pending());
        let original = media.register_deferred(
            MediaAssetKind::Attachment,
            "https://example.test/movie",
            "video/mp4",
        );
        get(&media, &original);
        assert!(media.sources.read().unwrap()[&original].deferred);
    }

    #[test]
    fn eager_registration_still_wins_and_non_get_does_not_promote() {
        let media = MediaRegistry::default();
        let emoji = media.register_emoji_on_display("T1/first", "https://example.test/first.png");
        let response = media.respond(
            Request::builder()
                .method("HEAD")
                .uri(emoji.uri())
                .body(vec![])
                .unwrap(),
        );
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(!media.has_pending());
        assert_eq!(
            media.register_emoji("T1/first", "https://example.test/first.png"),
            emoji
        );
        assert!(media.has_pending());
    }
}
