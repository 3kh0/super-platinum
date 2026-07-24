use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Shell, Widget, mouse, overlay, renderer};
use iced::{Element, Event, Length, Padding, Point, Rectangle, Size, Vector};

use crate::app::Message;

type Renderer = iced::Renderer;
type Theme = iced::Theme;

pub struct DragCapture<'a> {
    content: Element<'a, Message, Theme, Renderer>,
    padding: Padding,
    on_drag: Box<dyn Fn(Point) -> Message + 'a>,
    on_scroll: Box<dyn Fn(i32) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    dragging: bool,
}

pub fn drag_capture<'a>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    padding: Padding,
    on_drag: impl Fn(Point) -> Message + 'a,
    on_scroll: impl Fn(i32) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer> {
    Element::new(DragCapture {
        content: content.into(),
        padding,
        on_drag: Box::new(on_drag),
        on_scroll: Box::new(on_scroll),
    })
}

impl Widget<Message, Theme, Renderer> for DragCapture<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );

        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.dragging = cursor.is_over(bounds);
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                // Inside the bounds the editor handles the drag itself.
                if cursor.position_in(bounds).is_some() {
                    return;
                }
                let Some(position) = cursor.position() else {
                    return;
                };

                if position.y < bounds.y {
                    shell.publish((self.on_scroll)(-1));
                } else if position.y > bounds.y + bounds.height {
                    shell.publish((self.on_scroll)(1));
                }

                shell.publish((self.on_drag)(clamp_into_text(
                    position,
                    bounds,
                    self.padding,
                )));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

fn clamp_into_text(position: Point, bounds: Rectangle, padding: Padding) -> Point {
    let local = position - bounds.position() - Vector::new(padding.left, padding.top);
    let width = (bounds.width - padding.x()).max(0.0);
    let height = (bounds.height - padding.y()).max(0.0);

    Point::new(local.x.clamp(0.0, width), local.y.clamp(0.0, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PADDING: Padding = Padding {
        top: 4.0,
        right: 8.0,
        bottom: 4.0,
        left: 8.0,
    };

    fn bounds() -> Rectangle {
        Rectangle {
            x: 100.0,
            y: 200.0,
            width: 316.0,
            height: 48.0,
        }
    }

    #[test]
    fn clamps_above_and_left_to_origin() {
        let point = clamp_into_text(Point::new(0.0, 0.0), bounds(), PADDING);
        assert_eq!(point, Point::new(0.0, 0.0));
    }

    #[test]
    fn clamps_below_and_right_to_text_extent() {
        let point = clamp_into_text(Point::new(9_999.0, 9_999.0), bounds(), PADDING);
        assert_eq!(point, Point::new(300.0, 40.0));
    }

    #[test]
    fn subtracts_padding_inside_bounds() {
        let point = clamp_into_text(Point::new(150.0, 220.0), bounds(), PADDING);
        assert_eq!(point, Point::new(42.0, 16.0));
    }
}
