//! Shared image loader. Source selection and request/publication are separate
//! from passive opaque-ID protocol access. Blocking writes own serialization.
use super::*;
use std::future::Future;
use super_platinum_core::slack::Error;
use tokio::sync::OwnedMutexGuard;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Network,
    CacheOnly,
}
#[derive(Clone)]
pub(crate) struct Fetch {
    pub(crate) transport: Arc<Transport>,
    pub(crate) url: String,
    pub(crate) mime: String,
    pub(crate) authenticated: bool,
    user_agent: Arc<str>,
}
pub(super) async fn fetch(request: Fetch) -> Result<Vec<u8>, Error> {
    if request.authenticated {
        request
            .transport
            .get_bytes(&request.url, &request.user_agent)
            .await
    } else if request.mime == "image/gif" {
        request
            .transport
            .get_public_gif_bytes(&request.url, &request.user_agent)
            .await
    } else {
        request
            .transport
            .get_public_bytes(&request.url, &request.user_agent)
            .await
    }
}
#[derive(Clone, Default)]
pub(crate) struct Hooks {
    #[cfg(test)]
    pub(crate) on_disk_lookup: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    pub(crate) after_disk_read: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    pub(crate) after_write_admitted: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl MediaRegistry {
    pub(super) async fn load_pending_with<C, F, Fut>(
        &self,
        transport: Arc<Transport>,
        current: C,
        mode: Mode,
        fetch: F,
        hooks: Hooks,
    ) where
        C: Fn() -> bool,
        F: Fn(Fetch) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<u8>, Error>> + Send + 'static,
    {
        if !current() {
            return;
        }
        let guard = tokio::select! {
            biased;
            _ = self.scope.retired() => return,
            guard = self.load_lock.clone().lock_owned() => guard,
        };
        if !current() {
            return;
        }
        let cache_only = mode == Mode::CacheOnly || transport.health() == Health::Offline;
        let pending = if cache_only {
            self.pending_cache_sources()
        } else {
            self.pending_sources()
        };
        if pending.is_empty() {
            return;
        }
        let Some(pending) = self
            .hydrate_from_store_with(pending, &current, hooks.clone())
            .await
        else {
            return;
        };
        if !current() || cache_only || transport.health() == Health::Offline || pending.is_empty() {
            return;
        }
        let user_agent: Arc<str> = super_platinum_core::slack::xparams::Identity::from_capture()
            .user_agent
            .into();
        let permits = Arc::new(tokio::sync::Semaphore::new(12));
        let mut tasks = tokio::task::JoinSet::new();
        for (id, source) in pending {
            if !current() {
                return;
            }
            let scope = self.scope.clone();
            let permits = permits.clone();
            let request = Fetch {
                transport: transport.clone(),
                url: source.url.clone(),
                mime: source.mime.clone(),
                authenticated: source.authenticated,
                user_agent: user_agent.clone(),
            };
            let fetch = fetch.clone();
            tasks.spawn(async move {
                let permit = tokio::select! {
                    biased;
                    _ = scope.retired() => return None,
                    permit = permits.acquire_owned() => permit.ok()?,
                };
                // This check is the captured request's admission boundary.
                // Retirement cannot unsend an admitted HTTP request.
                if !scope.active() || request.transport.health() == Health::Offline {
                    return None;
                }
                let result = tokio::select! {
                    biased;
                    _ = scope.retired() => return None,
                    result = fetch(request) => result,
                };
                drop(permit);
                Some((id, source, result))
            });
        }
        let mut persist = Vec::new();
        loop {
            let next = tokio::select! {
                biased;
                _ = self.scope.retired() => return,
                next = tasks.join_next() => next,
            };
            let Some(next) = next else {
                break;
            };
            if !current() {
                return;
            }
            let Ok(Some((id, source, result))) = next else {
                continue;
            };
            match result {
                Ok(bytes) => {
                    let mime = detect_image_mime(&bytes).unwrap_or(&source.mime);
                    if let Some(slot) = source.slot.clone() {
                        persist.push((slot, source.url.clone(), bytes.clone()));
                    }
                    self.backoff
                        .write()
                        .expect("media registry poisoned")
                        .remove(&id);
                    self.insert_asset(id, mime.to_owned(), bytes.into(), false);
                }
                Err(error) => {
                    let host = url::Url::parse(&source.url)
                        .ok()
                        .and_then(|url| url.host_str().map(str::to_owned))
                        .unwrap_or_else(|| "invalid-host".into());
                    let category = if transport.health() == Health::Offline {
                        "offline"
                    } else if is_permanent(&error) {
                        "unavailable"
                    } else {
                        "retryable transport failure"
                    };
                    eprintln!("super-platinum: media {} from {host}: {category}", id.uri());
                    if transport.health() != Health::Offline {
                        self.record_failure(&id, is_permanent(&error));
                    }
                }
            }
        }
        if !current() {
            return;
        }
        self.persist(persist, guard, hooks).await;
    }
    async fn persist(
        &self,
        entries: Vec<(String, String, Vec<u8>)>,
        guard: OwnedMutexGuard<()>,
        hooks: Hooks,
    ) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        let scope = self.scope.clone();
        let limit = self.current_limit();
        // The worker, not this cancellation-prone awaiter, owns the lock. An
        // admitted write may finish after retirement, before any successor write.
        let _ = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            for (slot, url, bytes) in entries {
                if !scope.active() {
                    return;
                }
                #[cfg(test)]
                if let Some(hook) = &hooks.after_write_admitted {
                    hook();
                }
                if let Err(error) = store.store(&slot, &url, &bytes) {
                    if scope.active() {
                        eprintln!("super-platinum: could not cache a picture: {error}");
                    }
                }
            }
            if scope.active() {
                store.prune_to(limit);
            }
        })
        .await;
    }
}
#[cfg(test)]
mod tests;
