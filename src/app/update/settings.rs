use super::*;

pub(super) fn apply_settings(app: &mut App) -> bool {
    ui::theme::apply(&app.settings);
    if let Err(e) = config::save_settings(&app.settings) {
        app.toast(format!("could not save settings: {e}"));
        false
    } else {
        true
    }
}

pub(super) async fn import_background(path: PathBuf) -> Result<config::BackgroundSettings, String> {
    tokio::task::spawn_blocking(move || import_background_sync(&path))
        .await
        .map_err(|error| format!("could not import background: {error}"))?
}

pub(in crate::app) fn import_background_sync(
    path: &Path,
) -> Result<config::BackgroundSettings, String> {
    const MAX_BYTES: u64 = 25 * 1024 * 1024;
    const MAX_DIMENSION: u32 = 8192;

    let metadata =
        std::fs::metadata(path).map_err(|error| format!("could not read background: {error}"))?;
    if metadata.len() > MAX_BYTES {
        return Err("background must be 25 MB or smaller".to_owned());
    }

    let reader = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|error| format!("could not read background: {error}"))?;
    let format = reader
        .format()
        .ok_or_else(|| "background format could not be detected".to_owned())?;
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::WebP => "webp",
        _ => return Err("choose a PNG, JPEG, or WebP image".to_owned()),
    };
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| format!("could not decode background: {error}"))?;
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err("background dimensions must not exceed 8192 × 8192".to_owned());
    }
    let mut decoder = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|error| format!("could not read background: {error}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(300 * 1024 * 1024);
    decoder.limits(limits);
    decoder
        .decode()
        .map_err(|error| format!("could not decode background: {error}"))?;

    let directory = config::background_dir()
        .map_err(|error| format!("could not prepare background storage: {error}"))?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not prepare background storage: {error}"))?;
    let file_name = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let destination = directory.join(&file_name);
    let temporary = directory.join(format!("{file_name}.tmp"));
    std::fs::copy(path, &temporary)
        .map_err(|error| format!("could not copy background: {error}"))?;
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("could not finish background import: {error}"));
    }

    Ok(config::BackgroundSettings {
        file_name,
        fit: config::BackgroundFit::Cover,
        dim: 0.45,
        surface_opacity: 0.88,
    })
}

pub(super) fn remove_managed_background(background: &config::BackgroundSettings) {
    if let Some(path) = config::background_path(background)
        && let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "could not remove managed background");
    }
}
