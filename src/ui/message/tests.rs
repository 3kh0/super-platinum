use serde_json::json;

use super::{animated_frame_index, file_preview_dimensions};
use crate::slack::models::File;
use std::time::Duration;

#[test]
fn square_file_preview_uses_square_viewer() {
    let mut file = File::default();
    file.extra.insert("original_w".into(), json!(1024));
    file.extra.insert("original_h".into(), json!(1024));
    assert_eq!(file_preview_dimensions(&file), (320.0, 320.0));
}

#[test]
fn animated_frame_selection_honors_each_delay_and_loops() {
    let delays = [Duration::from_millis(20), Duration::from_millis(50)];
    let total = Duration::from_millis(70);

    assert_eq!(
        animated_frame_index(2, &delays, total, Duration::ZERO),
        Some(0)
    );
    assert_eq!(
        animated_frame_index(2, &delays, total, Duration::from_millis(19)),
        Some(0)
    );
    assert_eq!(
        animated_frame_index(2, &delays, total, Duration::from_millis(20)),
        Some(1)
    );
    assert_eq!(
        animated_frame_index(2, &delays, total, Duration::from_millis(70)),
        Some(0)
    );
}
