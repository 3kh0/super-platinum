use super::*;
/// Private text snapshot. Only its viewer/captured holders retain these bounded bytes.
/// Sources hold a weak reference, so repeated refreshes do not accumulate a history.
pub(crate) struct FreshOriginal {
    id: MediaAssetId,
    registry: MediaRegistry,
    _bytes: Arc<[u8]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_snapshot_survives_cache_eviction_without_refetch_and_cleans_its_source() {
        let registry = MediaRegistry::default();
        let old = registry.register_deferred(
            MediaAssetKind::Attachment,
            "https://files.slack.com/notes",
            "text/plain",
        );
        registry.insert(old.clone(), "text/plain", b"old".as_slice());
        let fresh = registry.insert_fresh_original(
            "https://files.slack.com/notes",
            "text/plain",
            Arc::from(b"new".as_slice()),
        );
        let id = fresh.id().clone();
        {
            let mut cache = registry.assets.lock().unwrap();
            let removed = cache.entries.remove(&id).unwrap();
            cache.bytes -= removed.bytes.len();
            cache.evicted.insert(id.clone());
        }
        assert!(registry.is_ready(&id));
        assert_eq!(registry.ready_download_bytes(&id).unwrap().as_ref(), b"new");
        registry.request(&id);
        assert!(registry.sources.read().unwrap()[&id].deferred);
        assert!(
            !registry
                .pending_sources()
                .iter()
                .any(|(current, _)| current == &id)
        );
        drop(fresh);
        assert!(!registry.sources.read().unwrap().contains_key(&id));
        assert!(registry.ready_bytes(&id).is_none());
        assert_eq!(registry.ready_bytes(&old).unwrap().as_ref(), b"old");
        assert_eq!(registry.source_ids.read().unwrap().len(), 1);
    }
}
impl FreshOriginal {
    pub(crate) fn id(&self) -> &MediaAssetId {
        &self.id
    }
}
impl Drop for FreshOriginal {
    fn drop(&mut self) {
        self.registry
            .sources
            .write()
            .expect("media registry poisoned")
            .remove(&self.id);
        self.registry
            .backoff
            .write()
            .expect("media registry poisoned")
            .remove(&self.id);
        let mut cache = self
            .registry
            .assets
            .lock()
            .expect("media registry poisoned");
        if let Some(old) = cache.entries.remove(&self.id) {
            cache.bytes -= old.bytes.len();
        }
        cache.evicted.remove(&self.id);
    }
}
impl MediaRegistry {
    pub(super) fn snapshot(&self, id: &MediaAssetId) -> Option<(String, Arc<[u8]>)> {
        let sources = self.sources.read().expect("media registry poisoned");
        let source = sources.get(id)?;
        let bytes = source.snapshot.as_ref()?.upgrade()?;
        Some((source.mime.clone(), bytes))
    }
    /// Never reuse or mutate a URL-shared source. Deferred prevents a periodic
    /// fetch from silently replacing this exact observed snapshot after eviction.
    pub(crate) fn insert_fresh_original(
        &self,
        url: &str,
        mime: &str,
        bytes: Arc<[u8]>,
    ) -> FreshOriginal {
        let id = MediaAssetId::new(MediaAssetKind::Attachment);
        self.sources
            .write()
            .expect("media registry poisoned")
            .insert(
                id.clone(),
                MediaSource {
                    url: url.into(),
                    original: Some((url.into(), true)),
                    mime: mime.into(),
                    authenticated: true,
                    deferred: true,
                    demand_on_display: false,
                    slot: None,
                    snapshot: Some(Arc::downgrade(&bytes)),
                },
            );
        self.insert(id.clone(), mime, bytes.clone());
        FreshOriginal {
            id,
            registry: self.clone(),
            _bytes: bytes,
        }
    }
}
