/// Response — The result of adding a widget to a `Ui`.
///
/// Inspired by egui's `Response`: every widget returns a `Response` that tells
/// the caller whether the widget was clicked, hovered, dragged, or had its
/// value changed. This enables the immediate-mode pattern:
///
/// ```ignore
/// if ui.button("Click me").clicked() {
///     // handle click
/// }
/// ```
use super::framebuffer::Rect;
use super::id::Id;

/// The result of adding a widget to a [`super::ui::Ui`].
#[derive(Clone, Debug)]
pub struct Response {
    /// The widget's unique ID.
    pub id: Id,

    /// The screen-space rectangle the widget occupies.
    pub rect: Rect,

    /// The rectangle used for interaction (may be clipped).
    pub interact_rect: Rect,

    // ── Interaction state ────────────────────────────────
    /// Widget was enabled and could be interacted with.
    pub enabled: bool,

    /// The pointer is hovering over this widget.
    pub hovered: bool,

    /// The widget was clicked (mouse down + up inside) this frame.
    pub clicked: bool,

    /// The widget was double-clicked this frame.
    pub double_clicked: bool,

    /// The widget was right-clicked (secondary button) this frame.
    pub secondary_clicked: bool,

    /// The widget is currently being pressed (pointer down on it).
    pub is_pointer_button_down_on: bool,

    /// A drag started on this widget this frame.
    pub drag_started: bool,

    /// The widget is currently being dragged.
    pub dragged: bool,

    /// A drag on this widget ended this frame.
    pub drag_stopped: bool,

    /// How far the pointer has dragged since last frame (only meaningful if `dragged`).
    pub drag_delta_x: i32,
    pub drag_delta_y: i32,

    /// The underlying value was modified (slider moved, text edited, checkbox toggled, etc.).
    pub changed: bool,

    /// Whether this widget has keyboard focus.
    pub has_focus: bool,

    /// Whether this widget gained keyboard focus this frame.
    pub gained_focus: bool,

    /// Whether this widget lost keyboard focus this frame.
    pub lost_focus: bool,
}

impl Response {
    /// Create a default non-interacted response.
    pub fn none(id: Id, rect: Rect) -> Self {
        Self {
            id,
            rect,
            interact_rect: rect,
            enabled: true,
            hovered: false,
            clicked: false,
            double_clicked: false,
            secondary_clicked: false,
            is_pointer_button_down_on: false,
            drag_started: false,
            dragged: false,
            drag_stopped: false,
            drag_delta_x: 0,
            drag_delta_y: 0,
            changed: false,
            has_focus: false,
            gained_focus: false,
            lost_focus: false,
        }
    }

    /// Was the widget clicked (primary button)?
    #[inline]
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    /// Was the widget double-clicked?
    #[inline]
    pub fn double_clicked(&self) -> bool {
        self.double_clicked
    }

    /// Was the widget right-clicked?
    #[inline]
    pub fn secondary_clicked(&self) -> bool {
        self.secondary_clicked
    }

    /// Is the pointer hovering over this widget?
    #[inline]
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Is the widget currently being dragged?
    #[inline]
    pub fn dragged(&self) -> bool {
        self.dragged
    }

    /// Was the underlying data changed?
    #[inline]
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Does this widget have keyboard focus?
    #[inline]
    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Did this widget lose keyboard focus this frame?
    #[inline]
    pub fn lost_focus(&self) -> bool {
        self.lost_focus
    }

    /// Union two responses (e.g. from a compound widget).
    pub fn union(self, other: Response) -> Response {
        Response {
            id: self.id,
            rect: rect_union(self.rect, other.rect),
            interact_rect: rect_union(self.interact_rect, other.interact_rect),
            enabled: self.enabled && other.enabled,
            hovered: self.hovered || other.hovered,
            clicked: self.clicked || other.clicked,
            double_clicked: self.double_clicked || other.double_clicked,
            secondary_clicked: self.secondary_clicked || other.secondary_clicked,
            is_pointer_button_down_on: self.is_pointer_button_down_on
                || other.is_pointer_button_down_on,
            drag_started: self.drag_started || other.drag_started,
            dragged: self.dragged || other.dragged,
            drag_stopped: self.drag_stopped || other.drag_stopped,
            drag_delta_x: self.drag_delta_x + other.drag_delta_x,
            drag_delta_y: self.drag_delta_y + other.drag_delta_y,
            changed: self.changed || other.changed,
            has_focus: self.has_focus || other.has_focus,
            gained_focus: self.gained_focus || other.gained_focus,
            lost_focus: self.lost_focus || other.lost_focus,
        }
    }
}

/// Compute the bounding union of two `Rect`s.
fn rect_union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width as i32).max(b.x + b.width as i32);
    let bottom = (a.y + a.height as i32).max(b.y + b.height as i32);
    Rect::new(x, y, (right - x).max(0) as u32, (bottom - y).max(0) as u32)
}

/// Wrapper for a closure result + its response, like egui's `InnerResponse`.
pub struct InnerResponse<R> {
    /// The return value of the closure.
    pub inner: R,
    /// The response of the surrounding container.
    pub response: Response,
}
