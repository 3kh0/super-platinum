use crate::state::ShellState;
use dioxus::prelude::*;
use super_platinum_core::{MediaAssetId, slack::models::File, state::file_download_name};

#[component]
pub fn DownloadButton(
    state: Signal<ShellState>,
    id: MediaAssetId,
    name: String,
    ready: bool,
) -> Element {
    let mut saving = use_signal(|| false);
    rsx! {
        button {class:"primary",disabled:!ready || saving(),"aria-label":"Download {name}",
            onclick:move |_|{
                if saving() {return;}
                let bytes=state.peek().media.ready_bytes(&id);
                let Some(bytes)=bytes else {return;};
                saving.set(true);
                let filename=file_download_name(&File{name:Some(name.clone()),..Default::default()});
                dioxus::core::spawn_forever(async move {
                    save(state,filename,bytes).await;
                    if let Ok(mut current)=saving.try_write(){*current=false;}
                });
            },
            if saving(){"Saving…"}else{"Download"}
        }
    }
}

async fn save(mut state: Signal<ShellState>, filename: String, bytes: std::sync::Arc<[u8]>) {
    let epoch = state.peek().core.account_epoch;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("Save file")
        .set_file_name(&filename)
        .save_file()
        .await
    else {
        return;
    };
    // Keep filesystem work off the UI thread and avoid another copy of a large
    // original. The native dialog owns destination and overwrite confirmation.
    let path = file.path().to_path_buf();
    let result = tokio::task::spawn_blocking(move || std::fs::write(path, bytes.as_ref())).await;
    if state.peek().core.account_epoch != epoch {
        return;
    }
    match result {
        Ok(Ok(())) => state.write().show_toast(format!("Saved {filename}")),
        _ => state
            .write()
            .show_toast("The file could not be saved. Check the destination and try again."),
    }
}
