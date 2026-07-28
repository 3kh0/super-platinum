use std::collections::HashMap;
use std::f32::consts::TAU;
use std::time::Duration;

use iced::widget::image::Handle as ImageHandle;
use iced::widget::text_editor::Content;
use iced::widget::{Column, Row, button, container, image, mouse_area, stack, svg, text};
use iced::{Alignment, Color, ContentFit, Element, Fill, Font, Length, Point};
use unicode_segmentation::UnicodeSegmentation;

use super::{blocks, composer, icons, profile, selectable, theme};
use crate::app::{
    ComposerAttachment, ComposerTarget, FilePreview, ImageFetchAuth, ImageViewerSource,
    MediaViewerKind, Message, ProfileHoverState, TextSelection, TextSelectionSurface,
};
use crate::slack::models::Message as SlackMessage;
use crate::state::{self, Workspace};

mod attachments;
mod body;
mod row;

pub use attachments::avatar_with_size;
pub(super) use body::emoji_inline;
pub use body::{empty_placeholder, inline_line, selectable_copy_text};
pub use row::row;

use attachments::*;
use body::*;
use row::*;

#[cfg(test)]
mod tests;
