//! Purpose-bound Search PDF presentation. Decoding is not admission authority.
use dioxus::{core::Task, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use super_platinum_core::MediaAssetId;

/// Result of this renderer's first decode; `native` is an IO discriminator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub pages: usize,
    pub native: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Config {
    pub id: String,
    pub nonce: String,
    pub opaque: String,
    pub source: String,
    pub isolated: bool,
}
impl Config {
    fn new(id: &MediaAssetId, nonce: String) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mount = NEXT.fetch_add(1, Ordering::Relaxed);
        Self {
            id: format!("pdf-isolated-{mount}"),
            source: format!("/pdf/search-data/{nonce}/{}", id.opaque()),
            opaque: id.opaque().into(),
            nonce,
            isolated: true,
        }
    }
    fn valid(&self) -> bool {
        !self.nonce.is_empty()
            && self.nonce.len() <= 128
            && self
                .nonce
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    kind: String,
    id: String,
    nonce: String,
    pages: Option<usize>,
}
enum Bridge {
    Native(dioxus::document::Eval),
    #[cfg(test)]
    Model(super::test_io::ModelBridge),
}
impl Bridge {
    fn native(&self) -> bool {
        matches!(self, Self::Native(_))
    }
    async fn recv(&mut self) -> Result<Event, ()> {
        match self {
            Self::Native(e) => e.recv().await.map_err(|_| ()),
            #[cfg(test)]
            Self::Model(m) => serde_json::from_value(m.recv().await.ok_or(())?).map_err(|_| ()),
        }
    }
    fn send(&self, value: serde_json::Value) -> Result<(), ()> {
        match self {
            Self::Native(e) => e.send(value).map_err(|_| ()),
            #[cfg(test)]
            Self::Model(m) => m.send(value),
        }
    }
}

#[component]
pub fn IsolatedPdf(
    id: MediaAssetId,
    nonce: String,
    admit: Callback<Decoded, bool>,
    failed: EventHandler<()>,
) -> Element {
    let config = use_hook(|| Config::new(&id, nonce.clone()));
    let current = use_hook(|| Rc::new(Cell::new(true)));
    let mounted = use_hook(|| Rc::new(Cell::new(false)));
    let task = use_hook(|| Rc::new(Cell::new(None::<Task>)));
    // A component occurrence belongs to one id/nonce. Reusing its props never
    // converts a pending first decode into a callback for a different owner.
    if config.opaque != id.opaque() || config.nonce != nonce {
        current.set(false);
        if let Some(task) = task.take() {
            task.cancel();
        }
    }
    let cleanup = config.id.clone();
    let alive = current.clone();
    let running = task.clone();
    #[cfg(test)]
    let io = dioxus::core::try_consume_context::<super::test_io::Io>();
    use_drop(move || {
        alive.set(false);
        if let Some(task) = running.take() {
            task.cancel();
        }
        #[cfg(not(test))]
        if !crate::fixture::is_fixture() {
            document::eval(&format!(
                "window.superPlatinumPdf?.get({})?.destroy();",
                serde_json::to_string(&cleanup).unwrap()
            ));
        }
        #[cfg(test)]
        let _ = cleanup;
    });
    if !current.get() {
        return rsx! {};
    }
    let dom_id = config.id.clone();
    let element = super::render::view(
        dom_id,
        EventHandler::new(move |_| {
            if mounted.replace(true) || !current.get() {
                return;
            }
            if !config.valid() {
                failed.call(());
                return;
            }
            let config = config.clone();
            let current = current.clone();
            #[cfg(test)]
            let io = io.clone();
            task.set(Some(spawn(async move {
                #[cfg(test)]
                {
                    let Some(io) = io else {
                        if current.get() {
                            failed.call(());
                        }
                        return;
                    };
                    let bridge = Bridge::Model(io.open(config.clone()));
                    drive(config, current, admit, failed, bridge).await;
                }
                #[cfg(not(test))]
                {
                    if crate::fixture::is_fixture() {
                        if current.get() {
                            failed.call(());
                        }
                        return;
                    }
                    let eval = document::eval(&format!(
                        "{}\nawait runPdfPreview(await dioxus.recv(),globalThis,dioxus);",
                        include_str!("bridge.js")
                    ));
                    let bridge = Bridge::Native(eval);
                    if bridge
                        .send(serde_json::to_value(&config).expect("owned PDF config"))
                        .is_err()
                    {
                        if current.get() {
                            failed.call(());
                        }
                        return;
                    }
                    drive(config, current, admit, failed, bridge).await;
                }
            })));
        }),
    );
    #[cfg(test)]
    super::test_io::record(&element);
    element
}

async fn drive(
    config: Config,
    current: Rc<Cell<bool>>,
    admit: Callback<Decoded, bool>,
    failed: EventHandler<()>,
    mut bridge: Bridge,
) {
    let native = bridge.native();
    let mut decoded = false;
    loop {
        let event = bridge.recv().await;
        if !current.get() {
            return;
        }
        let Ok(event) = event else {
            failed.call(());
            return;
        };
        if event.id != config.id || event.nonce != config.nonce {
            failed.call(());
            return;
        }
        match event.kind.as_str() {
            "decoded" if !decoded && event.pages.is_some_and(|p| (1..=200).contains(&p)) => {
                decoded = true;
                let pages = event.pages.unwrap();
                let accepted = admit.call(Decoded { pages, native });
                if !current.get() {
                    return;
                }
                if bridge.send(serde_json::json!({"kind":if accepted {"admit"} else {"deny"},"id":config.id,"nonce":config.nonce,"pages":pages})).is_err() {
                    failed.call(()); return;
                }
                if !accepted {
                    return;
                }
            }
            "closed" if event.pages.is_none() => {
                failed.call(());
                return;
            }
            "failed" if event.pages.is_none() => {
                failed.call(());
                return;
            }
            _ => {
                failed.call(());
                return;
            }
        }
    }
}
