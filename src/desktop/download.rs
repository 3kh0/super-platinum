//! Native Save dialog plus a bounded original-file transfer; previews remain cached.
use crate::state::ShellState;
use dioxus::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use super_platinum_core::{
    MediaAssetId,
    file_download::{DownloadError, OriginalSource, SaveMode, StagedFile},
    slack::models::File,
    state::file_download_name,
};
mod cooldown;
#[cfg(feature = "original-download-native-fixture")]
pub(crate) mod fixture;
mod owner;

#[component]
pub fn DownloadButton(
    state: Signal<ShellState>,
    id: MediaAssetId,
    name: String,
    ready: bool,
) -> Element {
    let mut saving = use_signal(|| false);
    let canceled = use_hook(|| Arc::new(AtomicBool::new(false)));
    let on_drop = canceled.clone();
    use_drop(move || on_drop.store(true, Ordering::Release));
    let available = {
        let shell = state.read();
        (ready && shell.media.ready_download_bytes(&id).is_some())
            || (shell.media.original_download_url(&id).is_some()
                && shell.core.session.is_some()
                && shell.core.transport.is_some())
    };
    #[cfg(feature = "original-download-native-fixture")]
    let available = available || fixture::available(&state.read(), &id);
    rsx! {
        button {class:"primary",style:"min-width:96px;",disabled:!available&&!saving(),"aria-label":if saving(){format!("Cancel download {name}")}else{format!("Download {name}")},
            onclick:move |_| {
                if saving() {canceled.store(true,Ordering::Release);return;}
                let captured={let shell=state.peek();owner::Owner::capture(&shell,id.clone())};
                let Some((owner,bytes))=captured else {return;};
                canceled.store(false,Ordering::Release);
                saving.set(true);
                let canceled=canceled.clone();
                let filename=file_download_name(&File{name:Some(name.clone()),..Default::default()});
                dioxus::core::spawn_forever(async move {
                    save(state,filename,owner,bytes,canceled).await;
                    if let Ok(mut current)=saving.try_write(){*current=false;}
                });
            },
            if saving(){"Cancel"}else{"Download"}
        }
    }
}

async fn save(
    mut state: Signal<ShellState>,
    filename: String,
    owner: owner::Owner,
    bytes: Option<Arc<[u8]>>,
    canceled: Arc<AtomicBool>,
) {
    let owner_state = state;
    let current = || {
        let valid = !canceled.load(Ordering::Acquire)
            && owner_state.try_peek().is_ok_and(|s| owner.current(&s));
        if !valid {
            canceled.store(true, Ordering::Release);
        }
        valid
    };
    if !current() {
        return;
    }
    if bytes.is_none() && !cooldown::available(&owner.key) {
        state.write().show_toast(
            "Wait before downloading another original file. The server asked us to pause.",
        );
        return;
    }
    let title = if bytes.is_some() {
        "Save file"
    } else {
        "Save original file - choose a new filename"
    };
    #[cfg(feature = "original-download-native-fixture")]
    let destination = if let Some(input) = &owner.fixture {
        if input.dialog_diagnostic() {
            if input.begin_dialog() {
                let result =
                    choose_destination_in(title, &filename, Some(input.dialog_directory())).await;
                input.dialog_returned(result.is_none());
            }
            // Diagnostic cancellation only: even an unexpected real selection
            // never enters transfer. No returned path is substituted or saved.
            return;
        }
        input.destination()
    } else {
        choose_destination(title, &filename).await
    };
    #[cfg(not(feature = "original-download-native-fixture"))]
    let destination = choose_destination(title, &filename).await;
    let Some(destination) = destination else {
        return;
    };
    if !current() {
        return;
    }
    let result = transfer(&owner, bytes, destination, &current, canceled.clone()).await;
    #[cfg(feature = "original-download-native-fixture")]
    if let Some(input) = &owner.fixture {
        input.completed(&result);
    }
    // A late known limit belongs to its captured owner even after account retirement.
    cooldown::record(&owner.key, &result);
    if !current() {
        return;
    }
    match result {
        Ok(()) => state.write().show_toast(format!("Saved {filename}")),
        Err(DownloadError::Canceled) => {}
        Err(DownloadError::Network(error)) => state.write().report_failure(
            &error,
            "Reconnect to download this file.",
            || match &error {
                super_platinum_core::slack::Error::RateLimited { .. } => {
                    "The server asked us to wait before downloading. Try again later.".into()
                }
                _ => {
                    "The original file could not be downloaded. Check access and try again.".into()
                }
            },
        ),
        Err(DownloadError::DestinationExists | DownloadError::DestinationChanged) => {
            state.write().show_toast(
                "The existing file was preserved. Download again and choose a new filename.",
            )
        }
        Err(error) => state.write().show_toast(error.to_string()),
    }
}
async fn choose_destination(title: &str, filename: &str) -> Option<std::path::PathBuf> {
    choose_destination_in(title, filename, None).await
}
async fn choose_destination_in(
    title: &str,
    filename: &str,
    directory: Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(filename);
    let dialog = if let Some(directory) = directory {
        dialog.set_directory(directory)
    } else {
        dialog
    };
    dialog
        .save_file()
        .await
        .map(|file| file.path().to_path_buf())
}
async fn transfer(
    owner: &owner::Owner,
    bytes: Option<Arc<[u8]>>,
    path: std::path::PathBuf,
    current: impl Fn() -> bool,
    canceled: Arc<AtomicBool>,
) -> Result<(), DownloadError> {
    if !current() {
        return Err(DownloadError::Canceled);
    }
    let mode = if bytes.is_some() {
        SaveMode::ConfirmedCachedOverwrite
    } else {
        SaveMode::NewFile
    };
    let mut staged = StagedFile::create(path, mode).await?;
    if let Some(bytes) = bytes {
        staged.cached(&bytes, &current).await?;
    } else {
        if !cooldown::available(&owner.key) {
            return Err(DownloadError::Network(
                super_platinum_core::slack::Error::RateLimited {
                    retry_after_secs: None,
                },
            ));
        }
        #[cfg(feature = "original-download-native-fixture")]
        let fixture_pair = owner.fixture.as_ref().map(|input| input.network());
        let source = OriginalSource::parse(
            owner
                .source
                .as_deref()
                .ok_or(DownloadError::InvalidSource)?,
        );
        #[cfg(feature = "original-download-native-fixture")]
        let source = match &fixture_pair {
            Some((source, _)) => source.clone(),
            None => source?,
        };
        #[cfg(not(feature = "original-download-native-fixture"))]
        let source = source?;
        #[cfg(feature = "original-download-native-fixture")]
        let transport = fixture_pair
            .as_ref()
            .map(|(_, transport)| transport)
            .or(owner.transport.as_ref());
        #[cfg(not(feature = "original-download-native-fixture"))]
        let transport = owner.transport.as_ref();
        let transport = transport.ok_or(DownloadError::InvalidSource)?;
        let user_agent = super_platinum_core::slack::xparams::Identity::from_capture().user_agent;
        transport
            .download_original(&source, &user_agent, staged.writer(), || {
                current() && cooldown::available(&owner.key)
            })
            .await?;
    }
    staged.commit(current, canceled).await
}

#[cfg(test)]
mod tests;
