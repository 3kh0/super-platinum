use super::*;
use crate::slack::client::RequestBody;
fn workspace() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        enterprise_id: None,
        user_id: "U1".into(),
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    }
}
fn raw() -> Value {
    json!({"avatar_hash":"hash1","image_48":"https://fixture.slack.com/avatar48.png","image_original":"https://fixture.slack.com/original.png"})
}
#[test]
fn crop_is_square_clamped_rounded_and_nonfinite_is_refused() {
    let c = SquareCrop::normalize(198.5, -1.0, 72.5, 200, 100).unwrap();
    assert_eq!((c.x(), c.y(), c.size()), (127, 0, 73));
    let c = SquareCrop::normalize(5.0, 5.0, 500.0, 200, 100).unwrap();
    assert_eq!((c.x(), c.y(), c.size()), (5, 0, 100));
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(SquareCrop::normalize(bad, 1.0, 4.0, 100, 100).is_none());
    }
    assert!(SquareCrop::normalize(1.0, 1.0, 0.0, 100, 100).is_none());
}
#[test]
fn actual_photo_permission_and_owner_not_name_permission_grant_writes() {
    let w = workspace();
    let b = json!({"ok":true,"team":{"id":"T1"},"self":{"id":"U1"}});
    let p = json!({"ok":true,"profile":raw()});
    let req = permissions_request(&SlackClient::default(), &w, &b).unwrap();
    let RequestBody::Json(args) = req.body else {
        panic!()
    };
    assert_eq!(args["permissions"], json!(["EDIT_PROFILE_PHOTO"]));
    assert_eq!(args["as_admin"], true);
    assert_eq!(args["user_id"], "U1");
    assert!(
        capability(
            &w,
            &b,
            &json!({"ok":true,"allowed":["EDIT_PROFILE_PHOTO"]}),
            &p
        )
        .unwrap()
        .permission
        .allowed()
    );
    assert!(
        !capability(&w, &b, &json!({"ok":true,"allowed":["UPDATE_NAME"]}), &p)
            .unwrap()
            .permission
            .allowed()
    );
    assert!(
        !capability(&w, &b, &json!({"ok":true}), &p)
            .unwrap()
            .permission
            .allowed()
    );
    let mut wrong = b.clone();
    wrong["self"]["id"] = json!("U2");
    assert!(
        capability(
            &w,
            &wrong,
            &json!({"ok":true,"allowed":["EDIT_PROFILE_PHOTO"]}),
            &p
        )
        .is_err()
    );
}
#[test]
fn own_photo_response_identity_matches_actual_request_and_refuses_explicit_conflicts() {
    let w = workspace();
    let boot = json!({"ok":true,"team":{"id":"T1"},"self":{"id":"U1"}});
    let permissions = json!({"ok":true,"allowed":["EDIT_PROFILE_PHOTO"]});
    let absent = json!({"ok":true,"profile":raw()});
    assert!(decode_own_photo(&w, &absent).is_ok());
    assert!(capability(&w, &boot, &permissions, &absent).is_ok());
    for identity in [
        json!("U1"),
        json!("U2"),
        Value::Null,
        json!(1),
        json!(["U1"]),
    ] {
        let mut response = absent.clone();
        response["user_id"] = identity.clone();
        let expected = identity == json!("U1");
        assert_eq!(decode_own_photo(&w, &response).is_ok(), expected);
        assert_eq!(
            capability(&w, &boot, &permissions, &response).is_ok(),
            expected
        );
    }
}
#[test]
fn enterprise_photo_uses_only_known_internal_workspace_permissions() {
    let mut w = workspace();
    w.team_id = "E1".into();
    w.enterprise_id = Some("E1".into());
    let b = json!({"ok":true,"team":{"id":"E1"},"self":{"id":"U1"},"workspaces":[{"id":"T1"},{"id":"T2","allow_external_users":true}]});
    let req = permissions_request(&SlackClient::default(), &w, &b).unwrap();
    let RequestBody::Json(args) = req.body else {
        panic!()
    };
    assert_eq!(args["team_ids"], json!(["T1"]));
    assert!(args.get("as_admin").is_none());
    let c = capability(
        &w,
        &b,
        &json!({"ok":true,"allowed_per_team":{"T2":["EDIT_PROFILE_PHOTO"],"T1":[]}}),
        &json!({"ok":true,"profile":raw()}),
    )
    .unwrap();
    assert!(!c.permission.allowed());
}
#[test]
fn exact_staged_set_and_delete_have_no_profile_or_foreign_user_fields() {
    let p = PreparedPhoto::parse(
        &json!({"ok":true,"id":"123456","url":"https://fixture.slack.com/prepared.png"}),
    )
    .unwrap();
    let c = SlackClient::default();
    let w = workspace();
    let r = set(
        &c,
        &w,
        &p,
        SquareCrop::normalize(10.0, 11.0, 72.0, 100, 100).unwrap(),
    )
    .request;
    let RequestBody::Form(fields) = r.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "synthetic".into()),
            ("id".into(), "123456".into()),
            ("crop_x".into(), "10".into()),
            ("crop_y".into(), "11".into()),
            ("crop_w".into(), "72".into()),
            ("_x_reason".into(), "edit-profile-set-photo".into())
        ]
    );
    let RequestBody::Form(fields) = remove(&c, &w).request.body else {
        panic!()
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(
        fields[1],
        ("_x_reason".into(), "edit_profile_delete_photo".into())
    );
    for url in [
        "http://fixture.slack.com/x",
        "https://slack.com.evil.test/x",
        "https://user@fixture.slack.com/x",
        "https://fixture.slack.com:444/x",
    ] {
        assert!(PreparedPhoto::parse(&json!({"ok":true,"id":"1","url":url})).is_err());
    }
}
#[test]
fn photo_adoption_preserves_every_unrelated_field_and_requires_full_read() {
    let mut p:UserProfile=serde_json::from_value(json!({"real_name":"Keep","title":"Keep title","status_text":"Keep status","status_emoji":":book:","fields":{"X":{"value":"keep"}},"future":{"exact":[1,null,true]},"avatar_hash":"before","image_48":"old","image_1024":"old1024"})).unwrap();
    let before = serde_json::to_value(&p).unwrap();
    let photo = Photo::from_profile(&raw()).unwrap();
    photo.adopt(&mut p);
    assert_eq!(
        Photo::from_profile(&serde_json::to_value(&p).unwrap()).unwrap(),
        photo
    );
    let after = serde_json::to_value(p).unwrap();
    for key in [
        "real_name",
        "title",
        "status_text",
        "status_emoji",
        "fields",
        "future",
    ] {
        assert_eq!(before[key], after[key]);
    }
    assert!(after.get("image_1024").is_none());
    assert_eq!(after["avatar_hash"], "hash1");
    assert!(Photo::from_profile(&json!({"image_original":null})).is_err());
    let mut bad = raw();
    bad["image_original"] = json!(true);
    assert!(Photo::from_profile(&bad).is_err());
}
#[test]
fn supported_large_png_is_not_rejected_at_sixteen_megabytes() {
    // Valid uncompressed PNG, >16MiB, still within captured4000x4000 geometry.
    let width = 2100;
    let height = 2100;
    let pixels = vec![127u8; (width * height * 4) as usize];
    let mut bytes = Vec::new();
    use ::image::ImageEncoder;
    ::image::codecs::png::PngEncoder::new_with_quality(
        &mut bytes,
        ::image::codecs::png::CompressionType::Uncompressed,
        ::image::codecs::png::FilterType::NoFilter,
    )
    .write_image(&pixels, width, height, ::image::ExtendedColorType::Rgba8)
    .unwrap();
    assert!(bytes.len() > 16 * 1024 * 1024);
    let photo = Image::parse(bytes).unwrap();
    assert_eq!((photo.width(), photo.height()), (width, height));
    let preview = ::image::load_from_memory(&photo.preview_bytes()).unwrap();
    assert!(preview.width() <= 480 && preview.height() <= 480);
    assert!(photo.preview_bytes().len() < 2 * 1024 * 1024);
    assert!(Image::parse(b"not an image".to_vec()).is_err());
}
#[test]
fn gif_preview_is_a_bounded_first_frame_while_upload_bytes_and_dimensions_stay_exact() {
    let mut bytes = Vec::new();
    ::image::codecs::gif::GifEncoder::new(&mut bytes)
        .encode(
            &vec![211; 64 * 48 * 4],
            64,
            48,
            ::image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    let photo = Image::parse(bytes.clone()).unwrap();
    assert_eq!(photo.bytes().as_ref(), bytes);
    assert_eq!(photo.mime(), "image/gif");
    assert_eq!((photo.width(), photo.height()), (64, 48));
    assert_eq!(
        crate::detect_image_mime(&photo.preview_bytes()),
        Some("image/png")
    );
    let preview = ::image::load_from_memory(&photo.preview_bytes()).unwrap();
    assert_eq!((preview.width(), preview.height()), (64, 48));
    let mut header_only = b"GIF89a".to_vec();
    header_only.extend_from_slice(&[64, 0, 48, 0, 0, 0, 0]);
    assert!(
        Image::parse(header_only).is_err(),
        "valid dimensions without a decodable frame are not a preview"
    );
}
