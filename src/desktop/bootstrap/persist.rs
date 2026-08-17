use dioxus::prelude::{ReadableExt, Signal};

use crate::state::ShellState;

pub async fn persist_settings(state: Signal<ShellState>) {
    let settings = state.read().core.settings.clone();
    match tokio::task::spawn_blocking(move || super_platinum_core::config::save_settings(&settings))
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("super-platinum: could not save settings: {error}"),
        Err(error) => eprintln!("super-platinum: settings writer stopped: {error}"),
    }
}
