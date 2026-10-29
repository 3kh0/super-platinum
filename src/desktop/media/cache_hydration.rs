//! One local lookup per actual picture demand, independent of HTTP backoff.
use super::loading::Hooks;
use super::*;
use super_platinum_core::CachedImage;

#[derive(Clone, Default)]
pub(super) struct Demand {
    revision: Arc<()>,
    attempted: bool,
}
impl Demand {
    pub(super) fn rearm(&mut self) {
        *self = Self::default();
    }
}
impl MediaRegistry {
    pub(crate) fn has_pending_for(&self, health: Health) -> bool {
        if health == Health::Offline {
            self.has_cache_pending()
        } else {
            self.has_pending()
        }
    }
    fn cache_candidate(
        &self,
        id: &MediaAssetId,
        source: &MediaSource,
        assets: &AssetCache,
    ) -> bool {
        self.eligible(source)
            && !source.deferred
            && source.slot.is_some()
            && source
                .disk_demand
                .as_ref()
                .is_some_and(|demand| !demand.attempted)
            && assets.wanted(id)
    }
    pub(super) fn has_cache_pending(&self) -> bool {
        if self.store.is_none() {
            return false;
        }
        let assets = self.assets.lock().expect("media registry poisoned");
        self.sources
            .read()
            .expect("media registry poisoned")
            .iter()
            .any(|(id, source)| self.cache_candidate(id, source, &assets))
    }
    pub(super) fn pending_cache_sources(&self) -> Vec<(MediaAssetId, MediaSource)> {
        if self.store.is_none() {
            return Vec::new();
        }
        let assets = self.assets.lock().expect("media registry poisoned");
        self.sources
            .read()
            .expect("media registry poisoned")
            .iter()
            .filter(|(id, source)| self.cache_candidate(id, source, &assets))
            .map(|(id, source)| (id.clone(), source.clone()))
            .collect()
    }
    // Call only with the asset lock held: demand visibility and its revision
    // change together, in the same assets -> sources order as completion.
    pub(super) fn rearm_cache_demand(&self, id: &MediaAssetId) {
        if let Some(demand) = self
            .sources
            .write()
            .expect("media registry poisoned")
            .get_mut(id)
            .and_then(|source| source.disk_demand.as_mut())
        {
            demand.rearm();
        }
    }
    // Preserve the unchanged original standalone regression entry.
    #[cfg(test)]
    pub(super) async fn hydrate_from_store(
        &self,
        pending: Vec<(MediaAssetId, MediaSource)>,
    ) -> Vec<(MediaAssetId, MediaSource)> {
        self.hydrate_from_store_with(pending, &|| self.scope.active(), Hooks::default())
            .await
            .unwrap_or_default()
    }
    pub(super) async fn hydrate_from_store_with<C: Fn() -> bool>(
        &self,
        pending: Vec<(MediaAssetId, MediaSource)>,
        current: &C,
        hooks: Hooks,
    ) -> Option<Vec<(MediaAssetId, MediaSource)>> {
        let Some(store) = self.store.clone() else {
            return Some(pending);
        };
        let lookups = pending
            .iter()
            .filter_map(|(id, source)| {
                Some((
                    id.clone(),
                    source.slot.clone()?,
                    source.url.clone(),
                    source.disk_demand.as_ref()?.revision.clone(),
                ))
            })
            .collect::<Vec<_>>();
        if lookups.is_empty() {
            return Some(pending);
        }
        let reads = lookups.clone();
        let scope = self.scope.clone();
        let read = tokio::task::spawn_blocking(move || {
            let mut hits: HashMap<MediaAssetId, CachedImage> = HashMap::new();
            for (id, slot, url, _) in reads {
                if !scope.active() {
                    break;
                }
                let image = store.load(&slot, &url);
                #[cfg(test)]
                if let Some(hook) = &hooks.on_disk_lookup {
                    hook();
                }
                if let Some(image) = image {
                    hits.insert(id, image);
                }
            }
            #[cfg(test)]
            if let Some(hook) = hooks.after_disk_read {
                hook();
            }
            hits
        });
        let mut hits = tokio::select! {
            biased;
            _ = self.scope.retired() => return None,
            result = read => result.unwrap_or_default(),
        };
        if !current() {
            return None;
        }
        let mut satisfied = HashSet::new();
        // A failed worker also consumes this demand's attempt. It must not
        // become a once-per-second disk loop; explicit renewed demand can retry.
        let mut assets = self.assets.lock().expect("media registry poisoned");
        let mut sources = self.sources.write().expect("media registry poisoned");
        for (id, slot, url, revision) in lookups {
            if !current() {
                return None;
            }
            let Some(source) = sources.get_mut(&id) else {
                continue;
            };
            if !self.eligible(source) {
                continue;
            }
            let Some(demand) = source.disk_demand.as_mut() else {
                continue;
            };
            if !Arc::ptr_eq(&demand.revision, &revision) {
                continue;
            }
            demand.attempted = true;
            let Some(image) = hits.remove(&id) else {
                continue;
            };
            if image.current {
                satisfied.insert(id.clone());
            }
            assets.insert(
                id,
                MediaAsset {
                    mime: image.mime,
                    bytes: image.bytes.into(),
                    provisional: !image.current,
                    last_used: 0,
                    revision: 0,
                    image_failed: false,
                    picture: Some((slot, url)),
                },
            );
            self.dirty.store(true, Ordering::Release);
        }
        Some(
            pending
                .into_iter()
                .filter(|(id, _)| !satisfied.contains(id))
                .collect(),
        )
    }
}
