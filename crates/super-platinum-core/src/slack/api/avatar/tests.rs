use super::*;
use ::image::ImageEncoder;

pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    ::image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &vec![51; (width * height * 4) as usize],
            width,
            height,
            ::image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    bytes
}
fn parsed(width: u32, height: u32) -> Result<Image, Error> {
    Image::parse("original.png".into(), "image/png", png(width, height))
}
#[test]
fn actual_decode_preserves_original_file_and_inclusive_dimensions() {
    for (w, h) in [(128, 128), (2000, 128), (128, 2000), (2000, 2000)] {
        let bytes = png(w, h);
        let image = Image::parse("R\u{e9}vision.png".into(), "image/png", bytes.clone()).unwrap();
        assert_eq!(&*image.bytes(), bytes);
        assert_eq!(image.width(), w);
        assert_eq!(image.height(), h);
        assert_eq!(image.filename(), "R\u{e9}vision.png");
        assert_eq!(image.mime(), "image/png");
    }
}
#[test]
fn validates_source_dimension_size_and_mime_limits() {
    assert!(matches!(parsed(127,128),Err(Error::Api(s)) if s=="too_small"));
    assert!(matches!(parsed(128,2001),Err(Error::Api(s)) if s=="too_large"));
    assert!(
        matches!(Image::parse("x.png".into(),"image/webp",png(128,128)),Err(Error::Api(s)) if s=="invalid_type")
    );
    assert!(Image::parse("x.png".into(), "image/jpeg", png(128, 128)).is_err());
    let mut bytes = png(128, 128);
    bytes.resize(MAX_BYTES, 0);
    assert!(Image::parse("x.png".into(), "image/png", bytes.clone()).is_ok());
    bytes.push(0);
    assert!(
        matches!(Image::parse("x.png".into(),"image/png",bytes),Err(Error::Api(s)) if s=="max_size")
    );
}
#[test]
fn accepts_real_jpeg_and_gif_without_reencoding() {
    for (format, mime, name) in [
        (::image::ImageFormat::Jpeg, "image/jpeg", "x.jpg"),
        (::image::ImageFormat::Gif, "image/gif", "x.gif"),
    ] {
        let image = ::image::DynamicImage::new_rgb8(128, 128);
        let mut buffer = std::io::Cursor::new(Vec::new());
        image.write_to(&mut buffer, format).unwrap();
        let bytes = buffer.into_inner();
        assert_eq!(
            &*Image::parse(name.into(), mime, bytes.clone())
                .unwrap()
                .bytes(),
            bytes
        );
    }
}
#[test]
fn corrupt_raster_and_multipart_name_injection_are_refused() {
    let mut bytes = png(128, 128);
    bytes.truncate(40);
    assert!(Image::parse("x.png".into(), "image/png", bytes).is_err());
    for name in ["", "x\r\nX-Secret: injected.png", "../x.png", "C:\\x.png"] {
        assert!(Image::parse(name.into(), "image/png", png(128, 128)).is_err());
    }
}
#[test]
fn crop_uses_full_image_coordinates_and_js_half_rounding() {
    assert_eq!(
        Crop::new(1.5, 2.49, 128.5, 512, 512).unwrap().cropbox(),
        "2,2,129"
    );
    for (x, y, size) in [
        (f64::NAN, 0., 128.),
        (-0.1, 0., 128.),
        (0., 0., 127.),
        (500., 0., 128.),
        (0., 0., f64::INFINITY),
    ] {
        assert!(Crop::new(x, y, size, 512, 512).is_err());
    }
    assert_eq!(
        Crop::new(0., 0., 2000., 2000, 2000).unwrap().cropbox(),
        "0,0,2000"
    );
}
#[test]
fn usable_replies_retain_exact_opaque_envelopes() {
    let raw = json!({"ok":true,"id":"AV-original","url":"https://files.slack.com/avatar?opaque=one","opaque":{"keep":[1,"two"]}});
    let upload = UploadReply::parse(&raw).unwrap();
    assert_eq!(upload.raw(), &raw);
    assert_eq!(upload.id(), "AV-original");
    let crop = CropReply::parse(&raw).unwrap();
    assert_eq!(crop.raw(), &raw);
    assert_eq!(crop.url(), upload.url());
}
#[test]
fn malformed_or_unsafe_success_is_not_invented_as_usable_avatar() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"id":"AV","url":""}),
        json!({"ok":1,"id":"AV","url":"https://x.slack.com/a"}),
        json!({"ok":true,"id":"AV","url":"javascript:alert(1)"}),
        json!({"ok":true,"id":"AV","url":"https://user:secret@x.slack.com/a"}),
        json!({"ok":true,"error":"broken","id":"AV","url":"https://x.slack.com/a"}),
    ] {
        assert!(UploadReply::parse(&raw).is_err());
        assert!(CropReply::parse(&raw).is_err());
    }
    assert!(UploadReply::parse(&json!({"ok":true,"url":"https://files.slack.com/a"})).is_err());
}
#[test]
fn debug_omits_original_filename_bytes_url_and_raw_credentials() {
    let image = Image::parse("sensitive-file-name.png".into(), "image/png", png(128, 128)).unwrap();
    assert!(!format!("{image:?}").contains("sensitive"));
    let raw = json!({"ok":true,"id":"secret-id","url":"https://files.slack.com/a?token=secret","token":"secret"});
    assert!(!format!("{:?}", UploadReply::parse(&raw).unwrap()).contains("secret"));
}
