use serde_json::json;

use super::file_preview_dimensions;
use crate::slack::models::File;

#[test]
fn square_file_preview_uses_square_viewer() {
    let mut file = File::default();
    file.extra.insert("original_w".into(), json!(1024));
    file.extra.insert("original_h".into(), json!(1024));
    assert_eq!(file_preview_dimensions(&file), (320.0, 320.0));
}
