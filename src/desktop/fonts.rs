//! Embedded Geist fonts: no network or filesystem work on the rendering path.

pub fn serve_asset(
    request: dioxus::desktop::AssetRequest,
    responder: dioxus::desktop::wry::RequestAsyncResponder,
) {
    let bytes: Option<&'static [u8]> = match request.uri().path() {
        "/fonts/Geist.woff2" => Some(include_bytes!("../../assets/fonts/Geist.woff2")),
        "/fonts/Geist-Italic.woff2" => {
            Some(include_bytes!("../../assets/fonts/Geist-Italic.woff2"))
        }
        "/fonts/GeistMono.woff2" => Some(include_bytes!("../../assets/fonts/GeistMono.woff2")),
        _ => None,
    };
    let response = dioxus::desktop::wry::http::Response::builder()
        .status(if bytes.is_some() { 200 } else { 404 })
        .header("Content-Type", "font/woff2")
        .header("Cache-Control", "no-cache")
        .body(std::borrow::Cow::Borrowed(bytes.unwrap_or(b"")))
        .expect("static font response is valid");
    responder.respond(response);
}
