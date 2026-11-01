use super::*;
use serde_json::json;
const ID: &str = "F12345678";
fn fresh() -> Value {
    json!({"ok":true,"file":{"id":ID,"mode":"hosted","filetype":"pdf","mimetype":"application/pdf","url_private_download":"https://files.slack.com/files-pri/T1-F12345678/download/report.pdf","is_external":false,"file_access":null,"is_hidden_by_limit":false,"restriction_type":0,"updated":1}})
}
#[test]
fn isolated_pdf_fresh_visibility_literal_content_and_precheck_never_mint_decoded_readiness() {
    let raw = fresh();
    let p = Projection::from_fresh(&raw, ID).unwrap();
    assert_eq!(
        p.url(),
        raw["file"]["url_private_download"].as_str().unwrap()
    );
    for access in [Value::Null, json!("visible")] {
        for tombstone in [Value::Null, json!(false)] {
            let mut v = raw.clone();
            v["file"]["file_access"] = access.clone();
            v["file"]["is_tombstoned"] = tombstone;
            assert!(Projection::from_fresh(&v, ID).is_ok());
        }
    }
    for key in [
        "mode",
        "filetype",
        "mimetype",
        "url_private_download",
        "is_external",
        "file_access",
        "is_hidden_by_limit",
        "restriction_type",
        "updated",
    ] {
        let mut v = raw.clone();
        v["file"].as_object_mut().unwrap().remove(key);
        assert!(Projection::from_fresh(&v, ID).is_err(), "missing {key}");
    }
    let mut known = raw.clone();
    known["file"].as_object_mut().unwrap().remove("updated");
    known["file"]["isUnknown"] = json!(false);
    known["file"]["isNonExistent"] = json!(false);
    assert!(Projection::from_fresh(&known, ID).is_ok());
    for key in ["converted_pdf", "app_id", "external_id", "external_type"] {
        for empty in [Value::Null, json!("")] {
            let mut v = raw.clone();
            v["file"][key] = empty;
            assert!(Projection::from_fresh(&v, ID).is_ok());
        }
        for invalid in [json!("not-empty"), json!(false), json!(0), json!([])] {
            let mut v = raw.clone();
            v["file"][key] = invalid;
            assert!(Projection::from_fresh(&v, ID).is_err(), "routing {key}");
        }
    }
    for key in [
        "is_deleted",
        "is_tombstoned",
        "is_tombstone",
        "isUnknown",
        "isNonExistent",
        "deleted",
        "hidden",
        "is_hidden",
        "is_dlp_tombstoned",
        "is_dsa_restricted",
    ] {
        for denied in [json!(true), json!("false"), json!(0)] {
            let mut v = raw.clone();
            v["file"][key] = denied;
            assert!(Projection::from_fresh(&v, ID).is_err(), "deny {key}");
        }
    }
    for (key, value) in [
        ("file_access", json!("not_visible")),
        ("file_access", json!(false)),
        ("is_hidden_by_limit", json!(true)),
        ("is_hidden_by_limit", Value::Null),
        ("restriction_type", json!(1)),
        ("restriction_type", json!(0.0)),
        ("restriction_type", Value::Null),
        ("is_external", json!(true)),
        ("is_external", Value::Null),
        ("mode", json!("not_found")),
        ("mode", json!("tombstone")),
        ("mode", json!("external")),
        ("filetype", json!("docx")),
        ("mimetype", json!("application/octet-stream")),
        ("updated", json!("1")),
        ("updated", json!(-1)),
        ("isMigratedFromLegacyPost", json!(false)),
        ("docsId", Value::Null),
    ] {
        let mut v = raw.clone();
        v["file"][key] = value;
        assert!(Projection::from_fresh(&v, ID).is_err(), "{key}");
    }
    assert!(Projection::from_fresh(&raw, "F87654321").is_err());
    for url in [
        "https://files.slack.com.evil/x.pdf",
        "http://files.slack.com/x.pdf",
        "https://app.slack.com/x.pdf",
        "https://u@files.slack.com/x.pdf",
        "https://files.slack.com/x.pdf#fragment",
        "https://files.slack.com:443/x.pdf",
        "https://FILES.slack.com/x.pdf",
    ] {
        let mut v = raw.clone();
        v["file"]["url_private_download"] = json!(url);
        assert!(Projection::from_fresh(&v, ID).is_err(), "url {url}");
    }
    let mut missing_download = raw.clone();
    missing_download["file"]
        .as_object_mut()
        .unwrap()
        .remove("url_private_download");
    missing_download["file"]["url_private"] = raw["file"]["url_private_download"].clone();
    assert!(
        Projection::from_fresh(&missing_download, ID).is_err(),
        "no private/full-url fallback"
    );
    for error in [Value::Null, json!("file_deleted")] {
        let mut v = raw.clone();
        v["error"] = error;
        assert!(Projection::from_fresh(&v, ID).is_err());
    }
    let bytes = b"%PDF-1.7\nnot parsed objects or page data\n%%EOF\n";
    assert!(
        p.validate_bytes(bytes).is_ok(),
        "container precheck deliberately does not decode PDF"
    );
    for bytes in [
        b"".as_slice(),
        b"<html>not PDF</html>",
        b"%PDF-1.7\ntruncated",
        b"%PDF-1.8\nunsupported\n%%EOF",
        b"%PDF-1.7\nbody\n%%EOFhidden",
    ] {
        assert!(p.validate_bytes(bytes).is_err());
    }
    assert!(p.validate_bytes(&vec![b' '; MAX_BYTES + 1]).is_err());
    assert_eq!(MAX_PAGES, 200);
    assert_eq!(
        raw,
        fresh(),
        "classification does not mutate original input"
    );
}
