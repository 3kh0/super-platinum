use super::*;
use super_platinum_core::slack::api::avatar::AvatarRequest;

pub(crate) async fn avatar(
    state: Signal<ShellState>,
    request: &AvatarRequest,
) -> Option<Result<Value, Error>> {
    if !requested() {
        return None;
    }
    let prepared = (|| {
        let s = state.peek();
        let model = model(&s)?;
        let case = case();
        if !matches!(
            case.as_str(),
            "first-avatar"
                | "first-avatar-crop-retry"
                | "first-avatar-crop-cancel"
                | "first-avatar-upload-back"
                | "first-avatar-upload-retry"
                | "first-avatar-upload-cancel-held"
                | "first-avatar-crop-back"
                | "first-avatar-crop-drag"
                | "first-avatar-crop-drag-retry"
                | "first-avatar-crop-back-held"
                | "first-avatar-crop-cancel-held"
        ) {
            return Err(closed());
        }
        let inspected = request.inspect();
        let url =
            url::Url::parse(inspected["url"].as_str().ok_or_else(closed)?).map_err(|_| closed())?;
        if url.scheme() != "https"
            || url.host_str() != Some("example.test")
            || inspected["tokenPresent"] != true
        {
            return Err(closed());
        }
        let field = |key: &str| {
            inspected["fields"]
                .as_array()
                .and_then(|a| a.iter().find(|v| v[0] == key))
                .and_then(|v| v[1].as_str())
                .ok_or_else(closed)
        };
        let previous = model
            .0
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|v| v["method"] == request.method())
            .count();
        let recovery = matches!(
            case.as_str(),
            "first-avatar-crop-retry" | "first-avatar-crop-cancel" | "first-avatar-crop-drag-retry"
        );
        let wide = case == "first-avatar-crop-drag-retry";
        let source_bytes = if wide {
            include_bytes!("avatar-wide.png").as_slice()
        } else {
            include_bytes!("avatar.png").as_slice()
        };
        let crop_bytes = if wide {
            include_bytes!("avatar-wide-crop.png").as_slice()
        } else {
            include_bytes!("avatar-crop.png").as_slice()
        };
        let (raw, media, applied) = match request.method() {
            "avatars.upload" => {
                if previous
                    >= if matches!(
                        case.as_str(),
                        "first-avatar-upload-retry"
                            | "first-avatar-crop-back-held"
                            | "first-avatar-crop-cancel-held"
                    ) {
                        2
                    } else {
                        1
                    }
                {
                    return Err(closed());
                }
                let image = &inspected["image"];
                if field("_x_reason")? != "workflows_upload_workflow_avatar"
                    || image["filename"]
                        != if wide {
                            "avatar-wide.png"
                        } else {
                            "avatar.png"
                        }
                    || image["mime"] != "image/png"
                    || image["width"] != if wide { 640 } else { 256 }
                    || image["height"] != if wide { 320 } else { 256 }
                    || image["length"].as_u64() != Some(source_bytes.len() as u64)
                    || request
                        .image()
                        .is_none_or(|i| i.bytes().as_ref() != source_bytes)
                {
                    return Err(closed());
                }
                if previous == 0
                    && matches!(
                        case.as_str(),
                        "first-avatar-upload-back" | "first-avatar-upload-retry"
                    )
                {
                    (
                        json!({"ok":false,"error":"invalid_image","opaque_receipt":"original-upload-rejected"}),
                        None,
                        false,
                    )
                } else {
                    let url = "https://example.test/first-publish-upload.png";
                    (
                        json!({"ok":true,"id":"AVATAR_FIRST","url":url,"opaque_receipt":"original-upload"}),
                        Some((url, source_bytes)),
                        true,
                    )
                }
            }
            "avatars.crop" => {
                if previous
                    >= if matches!(
                        case.as_str(),
                        "first-avatar-crop-retry" | "first-avatar-crop-drag-retry"
                    ) {
                        2
                    } else {
                        1
                    }
                {
                    return Err(closed());
                }
                if field("_x_reason")? != "workflows_crop_workflow_avatar"
                    || field("avatar")? != "AVATAR_FIRST"
                    || field("cropbox")? != if wide { "128,64,178" } else { "64,64,128" }
                {
                    return Err(closed());
                }
                if !model
                    .0
                    .lock()
                    .unwrap()
                    .responses
                    .iter()
                    .any(|v| v["result"]["raw"]["id"] == "AVATAR_FIRST")
                {
                    return Err(closed());
                }
                if recovery && previous == 0 {
                    (
                        json!({"ok":false,"error":"invalid_crop","opaque_receipt":"original-crop-rejected"}),
                        None,
                        false,
                    )
                } else {
                    let url = "https://example.test/first-publish-crop.png";
                    (
                        json!({"ok":true,"url":url,"opaque_receipt":"original-crop"}),
                        Some((url, crop_bytes)),
                        true,
                    )
                }
            }
            _ => return Err(closed()),
        };
        let n = {
            let mut m = model.0.lock().unwrap();
            if !m.armed {
                return Err(closed());
            }
            let n = m.requests.len() + 1;
            m.requests.push(json!({"serial":n,"phase":"avatar","method":request.method(),"request":inspected,"applied":applied}));
            n
        };
        if let Some((url, bytes)) = media {
            let id = s.media.register_image(
                super_platinum_core::MediaAssetKind::Attachment,
                url,
                "image/png",
            );
            s.media.insert(id, "image/png", Arc::<[u8]>::from(bytes));
        }
        let hold = (case == "first-avatar-upload-cancel-held"
            && request.method() == "avatars.upload")
            || (matches!(
                case.as_str(),
                "first-avatar-crop-back-held" | "first-avatar-crop-cancel-held"
            ) && request.method() == "avatars.crop");
        Ok((model, n, hold, raw))
    })();
    Some(match prepared {
        Ok((m, n, hold, raw)) => deliver(m, n, hold, Ok(raw)).await,
        Err(e) => Err(e),
    })
}
