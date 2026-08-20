//! Google Material Design icon path data for the desktop shell.
//! Rendered as inline SVG so the WebView does not need external assets.

const VIEW: &str = "0 0 24 24";

fn svg_data_uri(path: &str) -> String {
    // currentColor via CSS fill on the img is unreliable for data URIs; use a
    // neutral light fill that matches dark-theme chrome icons.
    let raw = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{VIEW}" fill="#c8c9cf"><path d="{path}"/></svg>"##
    );
    format!("data:image/svg+xml;utf8,{}", urlencoding_lite(&raw))
}

fn urlencoding_lite(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            b'"' => out.push_str("%22"),
            b'#' => out.push_str("%23"),
            b'<' => out.push_str("%3C"),
            b'>' => out.push_str("%3E"),
            b'{' | b'}' | b'|' | b'\\' | b'^' | b'`' => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

fn svg_markup(path: &str, class: &str) -> String {
    format!(
        r##"<svg class="{class}" xmlns="http://www.w3.org/2000/svg" viewBox="{VIEW}" aria-hidden="true"><path fill="currentColor" d="{path}"/></svg>"##
    )
}

pub const HOME: &str = "M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z";
pub const DMS: &str = "M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm-8 3.3c1.49 0 2.7 1.21 2.7 2.7s-1.21 2.7-2.7 2.7S9.3 9.49 9.3 8s1.21-2.7 2.7-2.7zM18 16H6v-.9c0-2 4-3.1 6-3.1s6 1.1 6 3.1v.9z";
pub const BELL: &str = "M12 22c1.1 0 2-.9 2-2h-4c0 1.1.89 2 2 2zm6-6v-5c0-3.07-1.64-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.63 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z";
pub const SEARCH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
pub const UNREADS: &str = "M19 3h-4.2A3 3 0 0 0 12 1a3 3 0 0 0-2.8 2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7h-2v7H5V5h4.2A3 3 0 0 0 12 7a3 3 0 0 0 2.8-2H19v4h2V5a2 2 0 0 0-2-2zm-7 2a1 1 0 1 1 0-2 1 1 0 0 1 0 2z";
pub const REPLY: &str = "M10 9V5l-7 7 7 7v-4.1c5 0 8.5 1.6 11 5.1-1-5-4-10-11-11z";
#[allow(dead_code)]
pub const TAG: &str =
    "M20 10V8h-4V4h-2v4h-4V4H8v4H4v2h4v4H4v2h4v4h2v-4h4v4h2v-4h4v-2h-4v-4h4zm-6 4h-4v-4h4v4z";
pub const LOCK: &str = "M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1 1.71 0 3.1 1.39 3.1 3.1v2z";
pub const PLUS: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
// Filled triangle — matches the pre-migration send control in the composer.
pub const SEND: &str = "M8 5v14l11-7z";
pub const COMPOSE: &str = "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z";
pub const MESSAGE: &str =
    "M4 3h16c1.1 0 2 .9 2 2v11c0 1.1-.9 2-2 2H7l-5 4V5c0-1.1.9-2 2-2zm0 2v12.83L6.3 16H20V5H4z";
pub const CLOCK: &str = "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm1-13h-2v6l5.25 3.15 1-1.64-4.25-2.51V7z";
pub const USER_ADD: &str = "M15 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm-9-2V7H4v3H1v2h3v3h2v-3h3v-2H6zm9 4c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z";
pub const MORE: &str = "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";
#[allow(dead_code)]
pub const AT: &str = "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10h5v-2h-5c-4.34 0-8-3.66-8-8s3.66-8 8-8 8 3.66 8 8v1.43c0 .79-.71 1.57-1.5 1.57s-1.5-.78-1.5-1.57V12c0-2.76-2.24-5-5-5s-5 2.24-5 5 2.24 5 5 5c1.38 0 2.64-.56 3.54-1.47.65.89 1.77 1.47 2.96 1.47 1.97 0 3.5-1.6 3.5-3.57V12c0-5.52-4.48-10-10-10zm0 13c-1.66 0-3-1.34-3-3s1.34-3 3-3 3 1.34 3 3-1.34 3-3 3z";

pub fn home_uri() -> String {
    svg_data_uri(HOME)
}
pub fn dms_uri() -> String {
    svg_data_uri(DMS)
}
pub fn bell_uri() -> String {
    svg_data_uri(BELL)
}
pub fn search_uri() -> String {
    svg_data_uri(SEARCH)
}
pub fn unreads_uri() -> String {
    svg_data_uri(UNREADS)
}
pub fn reply_uri() -> String {
    svg_data_uri(REPLY)
}
#[allow(dead_code)]
pub fn tag_uri() -> String {
    svg_data_uri(TAG)
}
pub fn lock_uri() -> String {
    svg_data_uri(LOCK)
}
pub fn plus_uri() -> String {
    svg_data_uri(PLUS)
}
pub fn send_uri() -> String {
    svg_data_uri(SEND)
}
pub fn compose_uri() -> String {
    svg_data_uri(COMPOSE)
}
pub fn message_uri() -> String {
    svg_data_uri(MESSAGE)
}
pub fn clock_uri() -> String {
    svg_data_uri(CLOCK)
}
pub fn user_add_uri() -> String {
    svg_data_uri(USER_ADD)
}
pub fn more_uri() -> String {
    svg_data_uri(MORE)
}
#[allow(dead_code)]
pub fn at_uri() -> String {
    svg_data_uri(AT)
}

/// Inline SVG markup for cases where currentColor theming is needed.
#[allow(dead_code)]
pub fn icon_html(path: &str) -> String {
    svg_markup(path, "mat-icon")
}

/// The Super Platinum mark: a faceted rhombus with the "S" cut out of it.
///
/// Geometry matches `assets/icons/mark-flat.svg`, so the in-app mark and the
/// bundled app icon stay in sync. Filled with a platinum gradient rather than
/// the 12-facet chrome of the full icon, which turns to mud below ~64px.
const MARK: &str = "M498.87,112.09Q505.92,105.00 513.10,111.96L904.58,491.15Q911.76,498.11 904.92,505.40L523.76,911.71Q516.92,919.00 509.97,911.81L119.19,507.61Q112.24,500.42 119.29,493.33L498.87,112.09ZM482.97,312.94Q491.45,307.63 500.47,311.95L881.32,494.36Q890.34,498.68 881.18,502.69L769.82,551.41Q760.66,555.42 751.57,551.25L385.91,383.59Q376.82,379.42 385.29,374.11L482.97,312.94ZM248.29,450.93Q257.55,447.16 266.51,451.61L627.81,631.44Q636.76,635.89 628.58,641.65L543.63,701.35Q535.45,707.11 526.59,702.45L144.25,501.60Q135.39,496.95 144.66,493.17L248.29,450.93Z";

/// The app icon, inline: platinum mark on the same dark plate the bundled icon
/// uses. The plate is what makes this readable on both themes — bare platinum
/// disappears into a light background.
pub fn brand_mark_html() -> String {
    format!(
        r##"<svg class="brand-mark-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" aria-hidden="true"><defs><linearGradient id="spp" x1="512" y1="0" x2="512" y2="1024" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#20242E"/><stop offset="1" stop-color="#0D1014"/></linearGradient><linearGradient id="spm" x1="0" y1="0" x2="0.35" y2="1"><stop offset="0" stop-color="#FDFDFE"/><stop offset="0.45" stop-color="#D2D5DB"/><stop offset="0.7" stop-color="#9BA1AC"/><stop offset="1" stop-color="#C9CDD4"/></linearGradient></defs><rect width="1024" height="1024" rx="228" ry="228" fill="url(#spp)"/><path fill="url(#spm)" fill-rule="evenodd" d="{MARK}"/></svg>"##
    )
}
