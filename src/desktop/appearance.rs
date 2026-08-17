use std::path::PathBuf;

use dioxus::prelude::{ReadableExt, Signal, WritableExt};

use crate::state::ShellState;

pub async fn import_background(mut state: Signal<ShellState>, path: PathBuf) {
    match super_platinum_core::appearance::import_background(path).await {
        Ok(background) => {
            let old = state.read().core.settings.background.clone();
            {
                let mut shell = state.write();
                shell.core.settings.background = Some(background);
                shell.load_managed_background();
            }
            if let Some(old) = old.as_ref() {
                super_platinum_core::appearance::remove_managed_background(old);
            }
            crate::bootstrap::persist_settings(state).await;
        }
        Err(error) => state.write().toast = Some(error),
    }
}

pub async fn clear_background(mut state: Signal<ShellState>) {
    let old = state.write().core.settings.background.take();
    state.write().background_uri = None;
    if let Some(old) = old.as_ref() {
        super_platinum_core::appearance::remove_managed_background(old);
    }
    crate::bootstrap::persist_settings(state).await;
}
