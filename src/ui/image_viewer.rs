use std::collections::HashMap;

use iced::advanced::image::{self as core_image, Renderer as _};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Widget, tree};
use iced::advanced::{Renderer as _, Shell, mouse};
use iced::widget::image::Handle as ImageHandle;
use iced::widget::{
    Space, button, column, container, image, opaque, responsive, row, slider, stack, svg, text,
};
use iced::{
    Background, Color, ContentFit, Element, Event, Fill, Font, Length, Point, Radians, Rectangle,
    Size, Vector,
};

use super::{icons, motion, theme};
use crate::app::{FilePreview, ImageViewerImage, ImageViewerState, MediaViewerKind, Message};
use crate::state;

type IcedRenderer = iced::Renderer;
type IcedTheme = iced::Theme;

const MIN_ZOOM: f32 = 1.0;
const MAX_ZOOM: f32 = 5.0;
const CLICK_ZOOM: f32 = 2.0;
const DRAG_THRESHOLD: f32 = 4.0;

pub fn overlay<'a>(
    base: Element<'a, Message>,
    viewer: &'a ImageViewerState,
    file_previews: &'a HashMap<String, FilePreview>,
    avatar_previews: &'a HashMap<String, FilePreview>,
) -> Element<'a, Message> {
    let layers = motion::overlay(viewer.open, move |anim, at| {
        let progress = motion::t(anim, at);
        let alpha = motion::fade(progress);
        let scrim = motion::scrim(progress, Message::ImageViewerClosed);
        let content = responsive(move |size| {
            let gutter = if size.width < 700.0 || size.height < 500.0 {
                8.0
            } else {
                16.0
            };
            let panel = panel(viewer, file_previews, avatar_previews, alpha);
            container(opaque(panel))
                .width(Fill)
                .height(Fill)
                .padding(gutter)
        });
        let content = motion::zoom_y(content.into(), progress, -8.0);
        Element::from(stack![scrim, content].width(Fill).height(Fill))
    })
    .on_finish_maybe((!viewer.open).then_some(Message::ImageViewerDismissed));

    stack![base, layers].into()
}

fn panel<'a>(
    viewer: &'a ImageViewerState,
    file_previews: &'a HashMap<String, FilePreview>,
    avatar_previews: &'a HashMap<String, FilePreview>,
    alpha: f32,
) -> Element<'a, Message> {
    let handle = current_handle(viewer, file_previews);
    let canvas: Element<'a, Message> = match viewer.source.kind {
        MediaViewerKind::Image if handle.is_some() => {
            let handle = handle.expect("checked image handle");
            ControlledViewer::new(handle, viewer.zoom, viewer.offset, alpha, |zoom, offset| {
                Message::ImageViewerTransformed { zoom, offset }
            })
            .into()
        }
        MediaViewerKind::Video
            if viewer
                .video
                .as_ref()
                .and_then(|video| video.player.as_ref())
                .is_some() =>
        {
            let player = viewer
                .video
                .as_ref()
                .and_then(|video| video.player.as_ref())
                .expect("checked video player");
            iced_video_player::VideoPlayer::new(player)
                .width(Fill)
                .height(Fill)
                .content_fit(ContentFit::Contain)
                .on_new_frame(Message::ImageViewerVideoFrame(viewer.generation))
                .on_end_of_stream(Message::ImageViewerVideoEnded(viewer.generation))
                .on_error(move |error| Message::ImageViewerVideoFailed {
                    generation: viewer.generation,
                    error: error.to_string(),
                })
                .into()
        }
        MediaViewerKind::Video if handle.is_some() => image(handle.expect("checked video poster"))
            .width(Fill)
            .height(Fill)
            .content_fit(ContentFit::Contain)
            .opacity(alpha)
            .into(),
        kind => container(
            text(if kind == MediaViewerKind::Video {
                "Loading video…"
            } else {
                "Loading original…"
            })
            .size(theme::TEXT_MD)
            .color(theme::fade(theme::text_3(), alpha)),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into(),
    };

    let image_area = container(canvas)
        .id(iced::widget::Id::new("image-viewer-canvas"))
        .width(Fill)
        .height(Fill)
        .padding([64.0, 20.0]);

    let top = container(
        row![
            metadata(viewer, avatar_previews, alpha),
            icon_button(icons::close(), Message::ImageViewerClosed, alpha, "Close"),
        ]
        .align_y(iced::Alignment::Start),
    )
    .width(Fill)
    .align_top(Fill)
    .padding(16.0);

    let zoom_controls: Element<'a, Message> = container(
        row![
            icon_button(
                icons::minus(),
                Message::ImageViewerZoomChanged((viewer.zoom - 0.25).max(MIN_ZOOM)),
                alpha,
                "Zoom out",
            ),
            slider(
                MIN_ZOOM..=MAX_ZOOM,
                viewer.zoom,
                Message::ImageViewerZoomChanged,
            )
            .step(0.01)
            .width(Length::Fixed(120.0))
            .style(theme::fade_slider(alpha)),
            icon_button(
                icons::plus(),
                Message::ImageViewerZoomChanged((viewer.zoom + 0.25).min(MAX_ZOOM)),
                alpha,
                "Zoom in",
            ),
        ]
        .spacing(2.0)
        .align_y(iced::Alignment::Center),
    )
    .padding(2.0)
    .style(theme::fade_container(theme::image_viewer_controls, alpha))
    .into();
    let controls = match viewer.source.kind {
        MediaViewerKind::Image => zoom_controls,
        MediaViewerKind::Video => video_controls(viewer, alpha),
    };

    let bottom = container(
        row![
            controls,
            Space::new().width(Fill),
            container(icon_button(
                icons::download(),
                Message::ImageViewerDownloadPressed,
                alpha,
                "Download",
            ))
            .padding(2.0)
            .style(theme::fade_container(theme::image_viewer_controls, alpha)),
        ]
        .align_y(iced::Alignment::End),
    )
    .width(Fill)
    .align_bottom(Fill)
    .padding(16.0);

    container(stack![image_area, top, bottom])
        .width(Fill)
        .height(Fill)
        .clip(true)
        .style(theme::fade_container(theme::image_viewer_panel, alpha))
        .into()
}

fn video_controls<'a>(viewer: &ImageViewerState, alpha: f32) -> Element<'a, Message> {
    let (duration, position, playing, volume, muted) = viewer
        .video
        .as_ref()
        .map(|video| {
            (
                video.duration,
                video.position,
                video.playing,
                video.volume,
                video.muted,
            )
        })
        .unwrap_or((0.0, 0.0, false, 1.0, false));
    let slider_duration = duration.max(0.01);
    let play_label = if playing { "Pause" } else { "Play" };
    let volume_label = if muted { "Unmute" } else { "Mute" };
    container(
        row![
            icon_button(
                if playing {
                    icons::pause()
                } else {
                    icons::play()
                },
                Message::ImageViewerVideoPlayPause,
                alpha,
                play_label,
            ),
            text(format_media_time(position))
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_2(), alpha)),
            slider(
                0.0..=slider_duration,
                position.clamp(0.0, slider_duration),
                Message::ImageViewerVideoSeekChanged,
            )
            .on_release(Message::ImageViewerVideoSeekReleased)
            .step(0.01)
            .width(Length::Fixed(140.0))
            .style(theme::fade_slider(alpha)),
            text(format_media_time(duration))
                .size(theme::TEXT_SM)
                .color(theme::fade(theme::text_3(), alpha)),
            icon_button(
                if muted {
                    icons::volume_off()
                } else {
                    icons::volume()
                },
                Message::ImageViewerVideoMuteToggled,
                alpha,
                volume_label,
            ),
            slider(0.0..=1.0, volume, Message::ImageViewerVideoVolumeChanged,)
                .on_release(Message::ImageViewerVideoVolumeReleased)
                .step(0.01)
                .width(Length::Fixed(60.0))
                .style(theme::fade_slider(alpha)),
        ]
        .spacing(theme::SPACE_XS)
        .align_y(iced::Alignment::Center),
    )
    .padding(2.0)
    .style(theme::fade_container(theme::image_viewer_controls, alpha))
    .into()
}

fn format_media_time(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    format!("{minutes}:{seconds:02}")
}

fn metadata<'a>(
    viewer: &'a ImageViewerState,
    avatar_previews: &'a HashMap<String, FilePreview>,
    alpha: f32,
) -> Element<'a, Message> {
    let initial = viewer
        .source
        .author_name
        .chars()
        .find(|c| !c.is_whitespace())
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_owned());
    let avatar: Element<'a, Message> = viewer
        .source
        .avatar_key
        .as_deref()
        .and_then(|key| avatar_previews.get(key))
        .and_then(preview_handle)
        .map(|handle| {
            image(handle)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .content_fit(ContentFit::Cover)
                .border_radius(8.0)
                .opacity(alpha)
                .into()
        })
        .unwrap_or_else(|| {
            container(
                text(initial)
                    .size(theme::TEXT_MD)
                    .color(theme::fade(theme::text_1(), alpha))
                    .font(Font {
                        weight: iced::font::Weight::Bold,
                        ..Font::default()
                    }),
            )
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .center_x(Length::Fixed(32.0))
            .center_y(Length::Fixed(32.0))
            .style(theme::fade_container(theme::avatar_placeholder, alpha))
            .into()
        });
    let when = if viewer.source.timestamp.is_empty() {
        "unknown time".to_owned()
    } else {
        state::format_relative_ts(&viewer.source.timestamp)
    };
    let detail = format!(
        "{when} in {} - {}",
        viewer.source.conversation, viewer.source.filename
    );
    container(
        row![
            avatar,
            column![
                text(viewer.source.author_name.clone())
                    .size(theme::TEXT_MD)
                    .color(theme::fade(theme::text_1(), alpha))
                    .font(Font {
                        weight: iced::font::Weight::Semibold,
                        ..Font::default()
                    }),
                text(detail)
                    .size(theme::TEXT_SM)
                    .color(theme::fade(theme::text_3(), alpha)),
            ]
            .width(Fill)
            .spacing(2.0),
        ]
        .spacing(theme::SPACE_SM)
        .align_y(iced::Alignment::Center),
    )
    .width(Fill)
    .into()
}

fn icon_button<'a>(
    handle: iced::widget::svg::Handle,
    message: Message,
    alpha: f32,
    label: &'static str,
) -> Element<'a, Message> {
    let icon = svg(handle)
        .width(Length::Fixed(15.0))
        .height(Length::Fixed(15.0))
        .style(theme::sidebar_icon(theme::fade(theme::text_1(), alpha)));
    container(
        button(icon)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .padding(7.0)
            .style(theme::fade_button(theme::image_viewer_button, alpha))
            .on_press(message),
    )
    .id(iced::widget::Id::from(format!(
        "image-viewer-{}",
        label.to_ascii_lowercase().replace(' ', "-")
    )))
    .into()
}

fn current_handle(
    viewer: &ImageViewerState,
    file_previews: &HashMap<String, FilePreview>,
) -> Option<ImageHandle> {
    match &viewer.image {
        ImageViewerImage::Loaded(handle) => Some(handle.clone()),
        ImageViewerImage::Loading | ImageViewerImage::Failed => file_previews
            .get(&viewer.source.preview_key)
            .and_then(preview_handle),
    }
}

fn preview_handle(preview: &FilePreview) -> Option<ImageHandle> {
    match preview {
        FilePreview::Loaded(handle) => Some(handle.clone()),
        FilePreview::Animated { frames, .. } => frames.first().cloned(),
        FilePreview::Loading | FilePreview::Failed => None,
    }
}

pub struct ControlledViewer<'a, Message> {
    handle: ImageHandle,
    zoom: f32,
    offset: Vector,
    opacity: f32,
    on_change: Box<dyn Fn(f32, Vector) -> Message + 'a>,
}

impl<'a, Message> ControlledViewer<'a, Message> {
    pub fn new(
        handle: ImageHandle,
        zoom: f32,
        offset: Vector,
        opacity: f32,
        on_change: impl Fn(f32, Vector) -> Message + 'a,
    ) -> Self {
        Self {
            handle,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            offset,
            opacity: opacity.clamp(0.0, 1.0),
            on_change: Box::new(on_change),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ViewerInteraction {
    press_origin: Option<Point>,
    starting_offset: Vector,
    dragged: bool,
}

impl<Message> Widget<Message, IcedTheme, IcedRenderer> for ControlledViewer<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ViewerInteraction>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ViewerInteraction::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut tree::Tree,
        _renderer: &IcedRenderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut tree::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &IcedRenderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                let y = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
                };
                if y == 0.0 {
                    return;
                }
                let zoom = if y > 0.0 {
                    self.zoom * 1.10
                } else {
                    self.zoom / 1.10
                }
                .clamp(MIN_ZOOM, MAX_ZOOM);
                let offset = zoom_around(self.offset, self.zoom, zoom, position, bounds.center());
                shell.publish((self.on_change)(
                    zoom,
                    clamp_offset(renderer, &self.handle, bounds, zoom, offset),
                ));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                let state = tree.state.downcast_mut::<ViewerInteraction>();
                state.press_origin = Some(position);
                state.starting_offset = self.offset;
                state.dragged = false;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let state = tree.state.downcast_mut::<ViewerInteraction>();
                let Some(origin) = state.press_origin else {
                    return;
                };
                let delta = *position - origin;
                if delta.x.hypot(delta.y) >= DRAG_THRESHOLD {
                    state.dragged = true;
                }
                if state.dragged && self.zoom > MIN_ZOOM {
                    let offset = clamp_offset(
                        renderer,
                        &self.handle,
                        bounds,
                        self.zoom,
                        state.starting_offset + delta,
                    );
                    shell.publish((self.on_change)(self.zoom, offset));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let state = tree.state.downcast_mut::<ViewerInteraction>();
                let Some(origin) = state.press_origin.take() else {
                    return;
                };
                if !state.dragged && cursor.is_over(bounds) {
                    let zoom = if self.zoom <= MIN_ZOOM {
                        CLICK_ZOOM
                    } else {
                        MIN_ZOOM
                    };
                    let offset = if zoom <= MIN_ZOOM {
                        Vector::ZERO
                    } else {
                        zoom_around(self.offset, self.zoom, zoom, origin, bounds.center())
                    };
                    shell.publish((self.on_change)(
                        zoom,
                        clamp_offset(renderer, &self.handle, bounds, zoom, offset),
                    ));
                }
                state.dragged = false;
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &tree::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &IcedRenderer,
    ) -> mouse::Interaction {
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }
        let state = tree.state.downcast_ref::<ViewerInteraction>();
        if state.press_origin.is_some() && state.dragged {
            mouse::Interaction::Grabbing
        } else if self.zoom > MIN_ZOOM {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::ZoomIn
        }
    }

    fn draw(
        &self,
        _tree: &tree::Tree,
        renderer: &mut IcedRenderer,
        _theme: &IcedTheme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let size = scaled_image_size(renderer, &self.handle, bounds.size(), self.zoom);
        let offset = clamp_offset(renderer, &self.handle, bounds, self.zoom, self.offset);
        let image_bounds = Rectangle::new(
            bounds.center() - Vector::new(size.width / 2.0, size.height / 2.0) + offset,
            size,
        );
        renderer.with_layer(bounds, |renderer| {
            renderer.draw_image(
                core_image::Image {
                    handle: self.handle.clone(),
                    filter_method: core_image::FilterMethod::Linear,
                    rotation: Radians(0.0),
                    border_radius: iced::border::Radius::default(),
                    opacity: self.opacity,
                },
                image_bounds,
                *viewport,
            );
        });
    }
}

impl<'a, Message: 'a> From<ControlledViewer<'a, Message>>
    for Element<'a, Message, IcedTheme, IcedRenderer>
{
    fn from(viewer: ControlledViewer<'a, Message>) -> Self {
        Element::new(viewer)
    }
}

fn scaled_image_size(
    renderer: &IcedRenderer,
    handle: &ImageHandle,
    bounds: Size,
    zoom: f32,
) -> Size {
    let raw = renderer.measure_image(handle).unwrap_or_default();
    let raw = Size::new(raw.width as f32, raw.height as f32);
    if raw.width <= 0.0 || raw.height <= 0.0 {
        return Size::ZERO;
    }
    let fit = (bounds.width / raw.width)
        .min(bounds.height / raw.height)
        .min(1.0);
    raw * (fit * zoom)
}

fn clamp_offset(
    renderer: &IcedRenderer,
    handle: &ImageHandle,
    bounds: Rectangle,
    zoom: f32,
    offset: Vector,
) -> Vector {
    let size = scaled_image_size(renderer, handle, bounds.size(), zoom);
    let x = ((size.width - bounds.width) / 2.0).max(0.0);
    let y = ((size.height - bounds.height) / 2.0).max(0.0);
    Vector::new(offset.x.clamp(-x, x), offset.y.clamp(-y, y))
}

fn zoom_around(
    offset: Vector,
    old_zoom: f32,
    new_zoom: f32,
    point: Point,
    center: Point,
) -> Vector {
    if new_zoom <= MIN_ZOOM || old_zoom <= 0.0 {
        return Vector::ZERO;
    }
    let ratio = new_zoom / old_zoom;
    let from_center = point - center;
    offset - (from_center - offset) * (ratio - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_around_center_preserves_offset() {
        assert_eq!(
            zoom_around(
                Vector::new(12.0, -4.0),
                2.0,
                3.0,
                Point::new(50.0, 50.0),
                Point::new(50.0, 50.0),
            ),
            Vector::new(18.0, -6.0),
        );
    }

    #[test]
    fn returning_to_fit_resets_offset() {
        assert_eq!(
            zoom_around(
                Vector::new(20.0, 20.0),
                2.0,
                1.0,
                Point::new(80.0, 80.0),
                Point::new(50.0, 50.0),
            ),
            Vector::ZERO,
        );
    }
}
