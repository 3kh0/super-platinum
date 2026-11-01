use super::*;
fn fresh() -> Value {
    json!({"ok":true,"file":{"id":"F12345678","mode":"hosted","filetype":"png","mimetype":"image/png","thumb_360":"https://files.slack.com/image.png","file_access":null,"is_hidden_by_limit":false,"is_tombstoned":false,"restriction_type":0,"is_external":false,"external_type":null,"app_id":null,"updated":1}})
}
#[test]
fn isolated_image_fresh_fields_never_default_inherited_denials() {
    let raw = fresh();
    assert!(Projection::from_fresh(&raw, "F12345678").is_ok());
    for key in [
        "file_access",
        "is_hidden_by_limit",
        "is_tombstoned",
        "restriction_type",
        "is_external",
        "external_type",
        "app_id",
        "mode",
        "filetype",
        "mimetype",
        "thumb_360",
        "updated",
    ] {
        let mut missing = raw.clone();
        missing["file"].as_object_mut().unwrap().remove(key);
        assert!(
            Projection::from_fresh(&missing, "F12345678").is_err(),
            "{key}"
        );
    }
    for (key, value) in [
        ("file_access", json!("access_denied")),
        ("is_hidden_by_limit", json!(true)),
        ("is_tombstoned", json!(true)),
        ("restriction_type", json!(1)),
        ("is_external", json!(true)),
        ("app_id", json!("A1")),
        ("is_deleted", json!(true)),
        ("isNonExistent", json!(true)),
        ("mimetype", json!("image/svg+xml")),
        ("thumb_360", json!("https://files.slack.com.evil.test/x")),
    ] {
        let mut deny = raw.clone();
        deny["file"][key] = value;
        assert!(Projection::from_fresh(&deny, "F12345678").is_err(), "{key}");
    }
    assert!(Projection::from_fresh(&raw, "FOTHER123").is_err());
    let mut contradictory = raw.clone();
    contradictory["error"] = json!("not_allowed");
    assert!(Projection::from_fresh(&contradictory, "F12345678").is_err());
    contradictory = raw.clone();
    contradictory["file"]["docsId"] = json!("FOTHER123");
    assert!(Projection::from_fresh(&contradictory, "F12345678").is_err());
    let mut explicit = raw.clone();
    explicit["file"].as_object_mut().unwrap().remove("updated");
    explicit["file"]["isUnknown"] = json!(false);
    explicit["file"]["isNonExistent"] = json!(false);
    assert!(Projection::from_fresh(&explicit, "F12345678").is_ok());
}
#[test]
fn isolated_image_content_is_static_decoded_bounded_and_reencoded() {
    let projection = Projection::from_fresh(&fresh(), "F12345678").unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 3)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let clean = projection.rasterize(png.get_ref()).unwrap();
    assert_eq!(image::load_from_memory(&clean).unwrap().height(), 3);
    assert!(projection.rasterize(b"<svg/>").is_err());
    assert!(projection.rasterize(&png.get_ref()[..12]).is_err());
    let mut animated = png.into_inner();
    animated.splice(8..8, [0, 0, 0, 0, b'a', b'c', b'T', b'L', 0, 0, 0, 0]);
    assert!(projection.rasterize(&animated).is_err());
    let mut jpeg = fresh();
    jpeg["file"]["filetype"] = json!("jpg");
    jpeg["file"]["mimetype"] = json!("image/jpeg");
    let projection = Projection::from_fresh(&jpeg, "F12345678").unwrap();
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    assert!(
        projection
            .rasterize(bytes.get_ref())
            .unwrap()
            .starts_with(b"\x89PNG")
    );
}
