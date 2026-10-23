// SPDX-License-Identifier: MIT
//! Live gallery stories. D8.1 fills the buttons row; later IDs append here
//! (D8.30 is the start-menu window that hosts every story).

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use knox_gpui::{Div, FontId, IntoElement, Styled, div, px, text};

use crate::alert::Alert;
use crate::avatar::{Avatar, AvatarGroup};
use crate::badge::Badge;
use crate::button::{Button, ButtonGroup, ButtonVariants, DropdownButton};
use crate::calendar::{Calendar, Date, DatePicker, Time, TimeField};
use crate::checkbox::Checkbox;
use crate::clipboard::Clipboard;
use crate::color_picker::ColorPicker;
use crate::empty::Empty;
use crate::form::{Field, Form};
use crate::icon::{Icon, IconName};
use crate::image::Image;
use crate::input::{FieldState, Input, InputGroup, Textarea};
use crate::kbd::Kbd;
use crate::label::Label;
use crate::link::Link;
use crate::number::NumberInput;
use crate::otp::OtpInput;
use crate::overlay::{AlertDialog, Dialog, FocusTrap, HoverCard, Popover, Positioner, Sheet};
use crate::progress::{Progress, ProgressCircle};
use crate::radio::{Radio, RadioGroup};
use crate::select::{Combobox, SearchableList, Select};
use crate::separator::Separator;
use crate::shimmer::Shimmer;
use crate::size::{Disableable, Selectable, Sizable};
use crate::skeleton::Skeleton;
use crate::slider::Slider;
use crate::spinner::Spinner;
use crate::switch::Switch;
use crate::tag::Tag;
use crate::theme::Theme;
use crate::toggle::{Toggle, ToggleGroup, ToggleVariants};
use crate::tooltip::Tooltip;

/// Snapshot of D8.1 interactive state (controlled; the owner stores values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct D8ButtonsSnapshot {
    pub switch_on: bool,
    pub checked: bool,
    pub radio: u8,
    pub group: u8,
    pub toggle: bool,
    pub toggle_a: bool,
    pub toggle_b: bool,
    pub dropdown_open: bool,
    pub dropdown_choice: u8,
}

/// User actions from the D8.1 gallery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D8ButtonsAction {
    Switch(bool),
    Check(bool),
    Radio(u8),
    Group(u8),
    Toggle(bool),
    ToggleGroup { index: u8, on: bool },
    DropdownToggle,
    DropdownPick(u8),
    Link,
}

/// Every D8.1 public type, for the host/kernel catalog checklist.
pub fn d8_buttons_catalog_types() -> &'static [&'static str] {
    &[
        "Button",
        "ButtonGroup",
        "DropdownButton",
        "Toggle",
        "ToggleGroup",
        "Switch",
        "Checkbox",
        "Radio",
        "RadioGroup",
        "Link",
    ]
}

/// Construct one of each D8.1 control (compile-time checklist).
pub fn d8_buttons_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Button::new("cat-btn")
            .themed(theme)
            .label("Button")
            .into_element(),
        ButtonGroup::new("cat-group")
            .themed(theme)
            .child(Button::new("g1").label("One"))
            .child(Button::new("g2").label("Two"))
            .into_element(),
        DropdownButton::new("cat-drop")
            .themed(theme)
            .button(Button::new("cat-drop-a").themed(theme).label("Save"))
            .items(["Save as", "Export"])
            .into_element(),
        Toggle::new("cat-tog")
            .themed(theme)
            .label("Bold")
            .into_element(),
        ToggleGroup::new("cat-togs")
            .child(Toggle::new("t1").themed(theme).label("A"))
            .child(Toggle::new("t2").themed(theme).label("B"))
            .into_element(),
        Switch::new("cat-sw")
            .themed(theme)
            .label("On")
            .into_element(),
        Checkbox::new("cat-cb")
            .themed(theme)
            .label("Ok")
            .into_element(),
        Radio::new("cat-rd").themed(theme).label("A").into_element(),
        RadioGroup::new("cat-rg")
            .themed(theme)
            .child(Radio::new("r1").label("A"))
            .child(Radio::new("r2").label("B"))
            .into_element(),
        Link::new("cat-link")
            .themed(theme)
            .label("Docs")
            .href("https://knoxos.local")
            .into_element(),
    ]
}

pub(crate) fn heading(theme: &Theme, title: &'static str) -> Div {
    div()
        .flex_col()
        .gap(theme.spacing.xs)
        .mt(theme.spacing.sm)
        .child(
            Label::new(title)
                .themed(theme)
                .font(FontId::UiBold)
                .size(theme.typo.ui),
        )
        .child(Separator::horizontal().themed(theme))
}

fn story(theme: &Theme, name: &'static str, hint: &'static str, body: impl IntoElement) -> Div {
    div()
        .flex_col()
        .gap(theme.spacing.xs)
        .child(
            Label::new(name)
                .themed(theme)
                .secondary(hint)
                .font(FontId::UiBold)
                .size(theme.typo.body),
        )
        .child(body)
}

/// D8.1 gallery rows: buttons, toggles, and binary inputs.
pub fn d8_buttons_gallery(
    theme: &Theme,
    snap: D8ButtonsSnapshot,
    on: Arc<dyn Fn(D8ButtonsAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let save_label = match snap.dropdown_choice {
        1 => "Export",
        2 => "Save as",
        _ => "Save",
    };

    let variants = div()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new("g-primary")
                        .themed(theme)
                        .primary()
                        .label("Primary"),
                )
                .child(
                    Button::new("g-secondary")
                        .themed(theme)
                        .secondary()
                        .label("Secondary"),
                )
                .child(
                    Button::new("g-danger")
                        .themed(theme)
                        .danger()
                        .label("Danger"),
                ),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(Button::new("g-ghost").themed(theme).ghost().label("Ghost"))
                .child(
                    Button::new("g-disabled")
                        .themed(theme)
                        .label("Disabled")
                        .disabled(true),
                )
                .child(
                    Button::new("g-outline")
                        .themed(theme)
                        .primary()
                        .outline()
                        .label("Outline"),
                ),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new("g-search")
                        .themed(theme)
                        .ghost()
                        .icon(IconName::Search)
                        .label("Search"),
                )
                .child(
                    Button::new("g-plus")
                        .themed(theme)
                        .primary()
                        .icon(IconName::Plus)
                        .compact(),
                )
                .child(
                    Button::new("g-save")
                        .themed(theme)
                        .secondary()
                        .icon(IconName::Save)
                        .label("Save")
                        .loading(true),
                ),
        );

    let click = fire.clone();
    let group = ButtonGroup::new("g-group")
        .themed(theme)
        .child(Button::new("g-one").label("One").selected(snap.group == 0))
        .child(Button::new("g-two").label("Two").selected(snap.group == 1))
        .child(
            Button::new("g-three")
                .label("Three")
                .selected(snap.group == 2),
        )
        .on_click(move |i| click(D8ButtonsAction::Group(i as u8)));

    let click = fire.clone();
    let pick = fire.clone();
    let dropdown = DropdownButton::new("g-drop")
        .themed(theme)
        .primary()
        .button(
            Button::new("g-drop-a")
                .themed(theme)
                .primary()
                .label(save_label),
        )
        .items(["Save", "Export", "Save as"])
        .open(snap.dropdown_open)
        .on_toggle(move || click(D8ButtonsAction::DropdownToggle))
        .on_item(move |i| pick(D8ButtonsAction::DropdownPick(i as u8)));

    let click = fire.clone();
    let toggles = ToggleGroup::new("g-toggles")
        .child(
            Toggle::new("g-bold")
                .themed(theme)
                .outline()
                .label("Bold")
                .checked(snap.toggle)
                .on_change({
                    let click = click.clone();
                    move |on| click(D8ButtonsAction::Toggle(on))
                }),
        )
        .child(
            Toggle::new("g-italic")
                .themed(theme)
                .ghost()
                .label("A")
                .checked(snap.toggle_a)
                .on_change({
                    let click = click.clone();
                    move |on| click(D8ButtonsAction::ToggleGroup { index: 0, on })
                }),
        )
        .child(
            Toggle::new("g-under")
                .themed(theme)
                .ghost()
                .label("B")
                .checked(snap.toggle_b)
                .on_change(move |on| click(D8ButtonsAction::ToggleGroup { index: 1, on })),
        );

    let click = fire.clone();
    let switch = Switch::new("g-switch")
        .themed(theme)
        .label("Switch")
        .checked(snap.switch_on)
        .on_change({
            let click = click.clone();
            move |on| click(D8ButtonsAction::Switch(on))
        });
    let check = Checkbox::new("g-check")
        .themed(theme)
        .label("Check")
        .checked(snap.checked)
        .on_change({
            let click = click.clone();
            move |on| click(D8ButtonsAction::Check(on))
        });
    let radios = RadioGroup::new("g-radios")
        .themed(theme)
        .selected(Some(snap.radio as usize))
        .child(Radio::new("g-ra").themed(theme).label("A"))
        .child(Radio::new("g-rb").themed(theme).label("B"))
        .child(Radio::new("g-rc").themed(theme).label("C"))
        .on_change({
            let click = click.clone();
            move |i| click(D8ButtonsAction::Radio(i as u8))
        });
    let link = Link::new("g-link")
        .themed(theme)
        .label("Docs")
        .href("https://knoxos.local/docs")
        .on_click(move || click(D8ButtonsAction::Link));

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Buttons"))
        .child(variants)
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(group)
                .child(dropdown),
        )
        .child(heading(theme, "Toggles"))
        .child(toggles)
        .child(heading(theme, "Switch / Check / Radio / Link"))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(switch)
                .child(check)
                .child(radios)
                .child(link),
        )
}

/// Every D8.2 public type, for the host/kernel catalog checklist.
pub fn d8_display_catalog_types() -> &'static [&'static str] {
    &[
        "Label",
        "Kbd",
        "Separator",
        "Badge",
        "Tag",
        "Avatar",
        "AvatarGroup",
        "Icon",
        "Image",
    ]
}

/// Construct one of each D8.2 control (compile-time checklist).
pub fn d8_display_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Label::new("Label").themed(theme).into_element(),
        Kbd::parse("cmd-k").unwrap().themed(theme).into_element(),
        Separator::horizontal().themed(theme).into_element(),
        Badge::new()
            .themed(theme)
            .count(3)
            .child(Icon::new(IconName::Bell).themed(theme))
            .into_element(),
        Tag::primary().themed(theme).label("Tag").into_element(),
        Avatar::new()
            .themed(theme)
            .name("Ada Lovelace")
            .into_element(),
        AvatarGroup::new()
            .themed(theme)
            .child(Avatar::new().name("Ada Lovelace"))
            .child(Avatar::new().name("Alan Turing"))
            .into_element(),
        div().child(Icon::new(IconName::Search).themed(theme)),
        div().child(Image::demo_gradient(32, 20).themed(theme)),
    ]
}

/// D8.2 gallery rows: text, chrome, and media.
pub fn d8_display_gallery(theme: &Theme) -> Div {
    let labels = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Label::new("Label").themed(theme))
        .child(
            Label::new("Search")
                .themed(theme)
                .secondary("docs")
                .highlights("doc"),
        )
        .child(Label::new("Secret").themed(theme).muted(true))
        .child(Label::new("Hidden").themed(theme).masked(true));

    let keys = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Kbd::parse("cmd-k").unwrap().themed(theme))
        .child(Kbd::parse("ctrl-shift-s").unwrap().themed(theme).outline())
        .child(Kbd::parse("esc").unwrap().themed(theme));

    let rules = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .w(px(160))
                .child(Separator::horizontal().themed(theme).label("or")),
        )
        .child(
            div()
                .h(px(24))
                .child(Separator::vertical_dashed().themed(theme)),
        );

    let chips = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            Badge::new()
                .themed(theme)
                .count(3)
                .child(Icon::new(IconName::Bell).themed(theme)),
        )
        .child(
            Badge::new()
                .themed(theme)
                .count(128)
                .max(99)
                .child(Icon::new(IconName::Inbox).themed(theme)),
        )
        .child(
            Badge::new()
                .themed(theme)
                .dot()
                .child(Icon::new(IconName::User).themed(theme)),
        )
        .child(
            Badge::new()
                .themed(theme)
                .icon(IconName::Check)
                .child(Avatar::new().themed(theme).name("Ada Lovelace").small()),
        );

    let tags = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Tag::primary().themed(theme).label("New"))
        .child(Tag::success().themed(theme).label("Live"))
        .child(Tag::warning().themed(theme).label("Warn"))
        .child(Tag::info().themed(theme).label("Info"))
        .child(Tag::danger().themed(theme).outline().label("Danger"))
        .child(Tag::secondary().themed(theme).label("Meta"));

    let people = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Avatar::new().themed(theme).name("Ada Lovelace"))
        .child(Avatar::new().themed(theme).name("Alan Turing").large())
        .child(Avatar::new().themed(theme).small())
        .child(
            AvatarGroup::new()
                .themed(theme)
                .limit(3)
                .child(Avatar::new().name("Ada Lovelace"))
                .child(Avatar::new().name("Alan Turing"))
                .child(Avatar::new().name("Grace Hopper"))
                .child(Avatar::new().name("Edsger Dijkstra")),
        );

    let media = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Icon::new(IconName::Search).themed(theme))
        .child(
            Icon::new(IconName::Check)
                .themed(theme)
                .color(theme.colors.primary),
        )
        .child(Icon::new(IconName::Folder).themed(theme))
        .child(Icon::new(IconName::Settings).themed(theme))
        .child(
            Image::demo_gradient(48, 28)
                .themed(theme)
                .rounded(theme.radius.md),
        );

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Display"))
        .child(story(
            theme,
            "Label",
            "secondary · highlight · muted · mask",
            labels,
        ))
        .child(story(theme, "Kbd", "one keycap per modifier", keys))
        .child(story(theme, "Separator", "labeled · dashed", rules))
        .child(story(
            theme,
            "Badge",
            "count · overflow · dot · icon",
            chips,
        ))
        .child(story(theme, "Tag", "semantic chips", tags))
        .child(story(
            theme,
            "Avatar",
            "initials · sizes · overflow group",
            people,
        ))
        .child(story(theme, "Icon / Image", "lucide · gradient", media))
}

/// Snapshot of D8.3 interactive state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct D8FeedbackSnapshot {
    pub alert_open: bool,
    pub copied: bool,
    pub phase: f32,
    pub reduced_motion: bool,
}

impl Default for D8FeedbackSnapshot {
    fn default() -> Self {
        Self {
            alert_open: true,
            copied: false,
            phase: 0.0,
            reduced_motion: false,
        }
    }
}

/// User actions from the D8.3 gallery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D8FeedbackAction {
    DismissAlert,
    Copy,
}

/// Every D8.3 public type, for the host/kernel catalog checklist.
pub fn d8_feedback_catalog_types() -> &'static [&'static str] {
    &[
        "Alert",
        "Empty",
        "Skeleton",
        "Shimmer",
        "Spinner",
        "Progress",
        "ProgressCircle",
        "Tooltip",
        "Clipboard",
    ]
}

/// Construct one of each D8.3 control (compile-time checklist).
pub fn d8_feedback_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Alert::info("Saved")
            .themed(theme)
            .title("Alert")
            .into_element(),
        Empty::new()
            .themed(theme)
            .title("Empty")
            .description("Nothing here")
            .into_element(),
        Skeleton::new().themed(theme).into_element(),
        div().child(Shimmer::new("Shimmer").themed(theme)),
        div().child(Spinner::new().themed(theme)),
        Progress::new().themed(theme).value(64.0).into_element(),
        div().child(ProgressCircle::new().themed(theme).value(64.0)),
        div().child(
            Tooltip::new("Hint")
                .themed(theme)
                .child(Label::new("Hover").themed(theme)),
        ),
        Clipboard::new().themed(theme).value("knox").into_element(),
    ]
}

/// D8.3 gallery rows: alerts, empty, motion, progress, tooltip, copy.
pub fn d8_feedback_gallery(
    theme: &Theme,
    snap: D8FeedbackSnapshot,
    on: Arc<dyn Fn(D8FeedbackAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let alert = Alert::info("Gallery controls are Knox GPUI views.")
        .themed(theme)
        .title("Knox UI")
        .visible(snap.alert_open)
        .on_close({
            let click = fire.clone();
            move || click(D8FeedbackAction::DismissAlert)
        });

    let empty = Empty::new()
        .themed(theme)
        .icon(IconName::Inbox)
        .title("No items")
        .description("Try another filter");

    let motion = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            Skeleton::new()
                .themed(theme)
                .w(px(96))
                .phase(snap.phase)
                .reduced_motion(snap.reduced_motion),
        )
        .child(
            Shimmer::new("Shimmer")
                .themed(theme)
                .phase(snap.phase)
                .reduced_motion(snap.reduced_motion),
        )
        .child(
            Spinner::new()
                .themed(theme)
                .phase(snap.phase)
                .reduced_motion(snap.reduced_motion),
        );

    let meters = div()
        .flex()
        .gap_2()
        .items_center()
        .child(Progress::new().themed(theme).value(64.0).w(px(120)))
        .child(ProgressCircle::new().themed(theme).value(64.0));

    let tips = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            Tooltip::new("Open palette")
                .themed(theme)
                .key_binding(Kbd::parse("cmd-k").unwrap().themed(theme))
                .child(Label::new("Hover me").themed(theme)),
        )
        .child(
            Clipboard::new()
                .themed(theme)
                .value("knox-gpui")
                .copied(snap.copied)
                .on_copy({
                    let click = fire.clone();
                    move |_| click(D8FeedbackAction::Copy)
                }),
        );

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Feedback"))
        .child(alert)
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(empty)
                .child(motion),
        )
        .child(meters)
        .child(tips)
}

/// Snapshot of D8.4 interactive state (gap-buffer fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D8InputSnapshot {
    pub line: FieldState,
    pub area: FieldState,
    pub group: FieldState,
}

impl Default for D8InputSnapshot {
    fn default() -> Self {
        Self {
            line: FieldState::empty(),
            area: FieldState::empty(),
            group: FieldState::from("docs"),
        }
    }
}

/// User actions from the D8.4 gallery row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum D8InputAction {
    Line(FieldState),
    Area(FieldState),
    Group(FieldState),
    Copy(String),
}

/// Every D8.4 public type, for the host/kernel catalog checklist.
pub fn d8_input_catalog_types() -> &'static [&'static str] {
    &["Input", "Textarea", "InputGroup"]
}

/// Construct one of each D8.4 control (compile-time checklist).
pub fn d8_input_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        div().child(
            Input::new("cat-in")
                .themed(theme)
                .placeholder("Name")
                .w(px(160)),
        ),
        div().child(
            Textarea::new("cat-area")
                .themed(theme)
                .placeholder("Notes")
                .rows(2)
                .w(px(160)),
        ),
        InputGroup::new("cat-ig")
            .themed(theme)
            .prefix("@")
            .suffix(".os")
            .field(Input::new("cat-ig-in").themed(theme).value("docs"))
            .into_element(),
    ]
}

/// D8.4 gallery rows: input, textarea, grouped field.
pub fn d8_input_gallery(
    theme: &Theme,
    snap: D8InputSnapshot,
    on: Arc<dyn Fn(D8InputAction) + Send + Sync>,
    paste: Arc<dyn Fn() -> Option<String> + Send + Sync>,
) -> Div {
    let fire = on;
    let paste_line = paste.clone();
    let paste_area = paste.clone();
    let paste_group = paste;

    let line = Input::new("g-in")
        .themed(theme)
        .placeholder("Type, select, copy/paste")
        .w(px(240))
        .state(snap.line)
        .on_change({
            let click = fire.clone();
            move |s| click(D8InputAction::Line(s))
        })
        .on_copy({
            let click = fire.clone();
            move |s| click(D8InputAction::Copy(String::from(s)))
        })
        .paste_from(move || paste_line());

    let area = Textarea::new("g-area")
        .themed(theme)
        .placeholder("Multiline")
        .rows(3)
        .w(px(240))
        .state(snap.area)
        .on_change({
            let click = fire.clone();
            move |s| click(D8InputAction::Area(s))
        })
        .on_copy({
            let click = fire.clone();
            move |s| click(D8InputAction::Copy(String::from(s)))
        })
        .paste_from(move || paste_area());

    let group = InputGroup::new("g-group")
        .themed(theme)
        .prefix("@")
        .suffix(".os")
        .field(
            Input::new("g-group-in")
                .themed(theme)
                .placeholder("host")
                .w(px(120)),
        )
        .state(snap.group)
        .on_change({
            let click = fire.clone();
            move |s| click(D8InputAction::Group(s))
        })
        .on_copy({
            let click = fire.clone();
            move |s| click(D8InputAction::Copy(String::from(s)))
        })
        .paste_from(move || paste_group());

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Input"))
        .child(div().flex().gap_2().items_center().child(line).child(group))
        .child(area)
}

/// Snapshot of D8.5 interactive state.
#[derive(Clone, Debug, PartialEq)]
pub struct D8FormsSnapshot {
    pub number: i64,
    pub number_text: FieldState,
    pub otp: String,
    pub slider: f32,
    pub color: knox_gpui::Hsla,
    pub name: FieldState,
}

impl Default for D8FormsSnapshot {
    fn default() -> Self {
        Self {
            number: 4,
            number_text: FieldState::from("4"),
            otp: String::new(),
            slider: 40.0,
            color: knox_gpui::Hsla::from_rgb(21, 153, 148),
            name: FieldState::empty(),
        }
    }
}

/// User actions from the D8.5 gallery row.
#[derive(Clone, Debug, PartialEq)]
pub enum D8FormsAction {
    Number(i64),
    NumberText(FieldState),
    Otp(String),
    Slider(f32),
    Color(knox_gpui::Hsla),
    Name(FieldState),
}

/// Every D8.5 public type, for the host/kernel catalog checklist.
pub fn d8_forms_catalog_types() -> &'static [&'static str] {
    &[
        "NumberInput",
        "OtpInput",
        "Slider",
        "ColorPicker",
        "Form",
        "Field",
    ]
}

/// Construct one of each D8.5 control (compile-time checklist).
pub fn d8_forms_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        NumberInput::new("cat-num")
            .themed(theme)
            .value(1)
            .into_element(),
        div().child(OtpInput::new().themed(theme).length(4)),
        div().child(Slider::new().themed(theme).value(40.0)),
        ColorPicker::new().themed(theme).into_element(),
        Form::new().themed(theme).into_element(),
        Field::new("Field")
            .themed(theme)
            .child(Input::new("cat-field").themed(theme))
            .into_element(),
    ]
}

/// D8.5 gallery rows: numeric, otp, slider, color, form field.
pub fn d8_forms_gallery(
    theme: &Theme,
    snap: D8FormsSnapshot,
    on: Arc<dyn Fn(D8FormsAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let number_text = if snap.number_text.is_empty() {
        FieldState::from(alloc::format!("{}", snap.number))
    } else {
        snap.number_text
    };
    let number = NumberInput::new("g-num")
        .themed(theme)
        .value(snap.number)
        .text(number_text)
        .min(0)
        .max(99)
        .w(px(72))
        .on_change({
            let click = fire.clone();
            move |n| click(D8FormsAction::Number(n))
        })
        .on_text({
            let click = fire.clone();
            move |s| click(D8FormsAction::NumberText(s))
        });
    let otp = OtpInput::new()
        .themed(theme)
        .length(4)
        .value(snap.otp)
        .on_change({
            let click = fire.clone();
            move |s| click(D8FormsAction::Otp(String::from(s)))
        });
    let slider = Slider::new()
        .themed(theme)
        .value(snap.slider)
        .w(px(160))
        .on_change({
            let click = fire.clone();
            move |v| click(D8FormsAction::Slider(v))
        });
    let color = ColorPicker::new()
        .themed(theme)
        .color(snap.color)
        .on_change({
            let click = fire.clone();
            move |c| click(D8FormsAction::Color(c))
        });
    let error = if snap.name.is_empty() { "Required" } else { "" };
    let form = Form::new().themed(theme).child(
        Field::new("Name")
            .themed(theme)
            .required(true)
            .error(error)
            .child(
                Input::new("g-name")
                    .themed(theme)
                    .placeholder("Ada")
                    .w(px(160))
                    .state(snap.name)
                    .on_change({
                        let click = fire.clone();
                        move |s| click(D8FormsAction::Name(s))
                    }),
            ),
    );

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Forms"))
        .child(div().flex().gap_2().items_center().child(number).child(otp))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(slider)
                .child(color),
        )
        .child(form)
}

const PEOPLE: [&str; 4] = ["Ada", "Alan", "Grace", "Edsger"];

/// Snapshot of D8.6 interactive state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct D8SelectSnapshot {
    pub select_open: bool,
    pub select_idx: usize,
    pub combo_open: bool,
    pub combo_query: FieldState,
    pub combo_idx: Option<usize>,
    pub combo_hi: usize,
    pub list_query: FieldState,
    pub list_idx: Option<usize>,
    pub list_hi: usize,
}

impl Default for D8SelectSnapshot {
    fn default() -> Self {
        Self {
            select_open: false,
            select_idx: 0,
            combo_open: true,
            combo_query: FieldState::empty(),
            combo_idx: None,
            combo_hi: 0,
            list_query: FieldState::from("a"),
            list_idx: Some(0),
            list_hi: 0,
        }
    }
}

/// User actions from the D8.6 gallery row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum D8SelectAction {
    SelectToggle,
    Select(usize),
    ComboToggle,
    ComboQuery(FieldState),
    Combo(usize),
    ComboHighlight(usize),
    ListQuery(FieldState),
    List(usize),
    ListHighlight(usize),
}

/// Every D8.6 public type, for the host/kernel catalog checklist.
pub fn d8_select_catalog_types() -> &'static [&'static str] {
    &["Select", "Combobox", "SearchableList"]
}

/// Construct one of each D8.6 control (compile-time checklist).
pub fn d8_select_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Select::new("cat-sel")
            .themed(theme)
            .items(PEOPLE)
            .into_element(),
        Combobox::new("cat-combo")
            .themed(theme)
            .items(PEOPLE)
            .into_element(),
        SearchableList::new("cat-list")
            .themed(theme)
            .items(PEOPLE)
            .into_element(),
    ]
}

/// D8.6 gallery rows: select, combobox, searchable list.
pub fn d8_select_gallery(
    theme: &Theme,
    snap: D8SelectSnapshot,
    on: Arc<dyn Fn(D8SelectAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let select = Select::new("g-sel")
        .themed(theme)
        .items(PEOPLE)
        .selected(Some(snap.select_idx))
        .open(snap.select_open)
        .on_toggle({
            let click = fire.clone();
            move || click(D8SelectAction::SelectToggle)
        })
        .on_select({
            let click = fire.clone();
            move |i| click(D8SelectAction::Select(i))
        });
    let combo = Combobox::new("g-combo")
        .themed(theme)
        .items(PEOPLE)
        .query(snap.combo_query)
        .selected(snap.combo_idx)
        .open(snap.combo_open)
        .highlight(snap.combo_hi)
        .on_query({
            let click = fire.clone();
            move |s| click(D8SelectAction::ComboQuery(s))
        })
        .on_toggle({
            let click = fire.clone();
            move || click(D8SelectAction::ComboToggle)
        })
        .on_select({
            let click = fire.clone();
            move |i| click(D8SelectAction::Combo(i))
        })
        .on_highlight({
            let click = fire.clone();
            move |i| click(D8SelectAction::ComboHighlight(i))
        });
    let list = SearchableList::new("g-list")
        .themed(theme)
        .items(PEOPLE)
        .query(snap.list_query)
        .selected(snap.list_idx)
        .highlight(snap.list_hi)
        .on_query({
            let click = fire.clone();
            move |s| click(D8SelectAction::ListQuery(s))
        })
        .on_select({
            let click = fire.clone();
            move |i| click(D8SelectAction::List(i))
        })
        .on_highlight({
            let click = fire.clone();
            move |i| click(D8SelectAction::ListHighlight(i))
        });

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Select"))
        .child(
            div()
                .flex()
                .gap_2()
                .items_start()
                .child(select)
                .child(combo)
                .child(list),
        )
}

/// Snapshot of D8.7 interactive state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D8DateSnapshot {
    pub view: Date,
    pub selected: Date,
    pub open: bool,
    pub time: Time,
    pub segment: u8,
}

impl Default for D8DateSnapshot {
    fn default() -> Self {
        Self {
            view: Date::new(2026, 10, 1),
            selected: Date::new(2026, 10, 22),
            open: true,
            time: Time::new(9, 30),
            segment: 0,
        }
    }
}

/// User actions from the D8.7 gallery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D8DateAction {
    View(Date),
    Select(Date),
    Toggle,
    Time(Time),
    Segment(u8),
}

/// Every D8.7 public type, for the host/kernel catalog checklist.
pub fn d8_date_catalog_types() -> &'static [&'static str] {
    &["Calendar", "DatePicker", "TimeField"]
}

/// Construct one of each D8.7 control (compile-time checklist).
pub fn d8_date_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Calendar::new().themed(theme).into_element(),
        DatePicker::new().themed(theme).into_element(),
        div().child(TimeField::new().themed(theme)),
    ]
}

/// D8.7 gallery rows: calendar, date picker, time field.
pub fn d8_date_gallery(
    theme: &Theme,
    snap: D8DateSnapshot,
    on: Arc<dyn Fn(D8DateAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let cal = Calendar::new()
        .themed(theme)
        .view(snap.view)
        .selected(snap.selected)
        .on_select({
            let click = fire.clone();
            move |d| click(D8DateAction::Select(d))
        })
        .on_view({
            let click = fire.clone();
            move |d| click(D8DateAction::View(d))
        });
    let picker = DatePicker::new()
        .themed(theme)
        .selected(snap.selected)
        .view(snap.view)
        .open(snap.open)
        .on_toggle({
            let click = fire.clone();
            move || click(D8DateAction::Toggle)
        })
        .on_select({
            let click = fire.clone();
            move |d| click(D8DateAction::Select(d))
        })
        .on_view({
            let click = fire.clone();
            move |d| click(D8DateAction::View(d))
        });
    let time = TimeField::new()
        .themed(theme)
        .value(snap.time)
        .segment(snap.segment)
        .on_change({
            let click = fire.clone();
            move |t| click(D8DateAction::Time(t))
        })
        .on_segment({
            let click = fire.clone();
            move |s| click(D8DateAction::Segment(s))
        });

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Date / Time"))
        .child(
            div()
                .flex()
                .gap_2()
                .items_start()
                .child(cal)
                .child(div().flex_col().gap_2().child(picker).child(time)),
        )
}

/// Snapshot of D8.8 overlay state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct D8OverlaySnapshot {
    pub dialog: bool,
    pub alert: bool,
    pub sheet: bool,
    pub popover: bool,
    pub hover: bool,
}

/// User actions from the D8.8 gallery row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum D8OverlayAction {
    Dialog(bool),
    Alert(bool),
    Sheet(bool),
    Popover(bool),
    Hover(bool),
}

/// Every D8.8 public type, for the host/kernel catalog checklist.
pub fn d8_overlay_catalog_types() -> &'static [&'static str] {
    &[
        "Dialog",
        "AlertDialog",
        "Sheet",
        "Popover",
        "HoverCard",
        "FocusTrap",
        "Positioner",
    ]
}

/// Construct one of each D8.8 control (compile-time checklist).
pub fn d8_overlay_construct_all(theme: &Theme) -> Vec<Div> {
    alloc::vec![
        Dialog::new().themed(theme).into_element(),
        AlertDialog::new().themed(theme).into_element(),
        Sheet::new().themed(theme).into_element(),
        Popover::new().themed(theme).into_element(),
        HoverCard::new().themed(theme).into_element(),
        div().child(FocusTrap::child(text("trap"))),
        Positioner::new().child(text("pos")).into_element(),
    ]
}

/// D8.8 gallery rows: dialogs, sheet, popover, hover card.
pub fn d8_overlay_gallery(
    theme: &Theme,
    snap: D8OverlaySnapshot,
    on: Arc<dyn Fn(D8OverlayAction) + Send + Sync>,
) -> Div {
    let fire = on;
    let buttons = div()
        .flex()
        .gap_2()
        .items_center()
        .child(
            Button::new("ov-dlg")
                .themed(theme)
                .label("Dialog")
                .on_click({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Dialog(true))
                }),
        )
        .child(
            Button::new("ov-alert")
                .themed(theme)
                .secondary()
                .label("Alert")
                .on_click({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Alert(true))
                }),
        )
        .child(
            Button::new("ov-sheet")
                .themed(theme)
                .ghost()
                .label("Sheet")
                .on_click({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Sheet(true))
                }),
        )
        .child(
            Button::new("ov-pop")
                .themed(theme)
                .ghost()
                .label("Popover")
                .on_click({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Popover(!snap.popover))
                }),
        )
        .child(
            Button::new("ov-hover")
                .themed(theme)
                .ghost()
                .label("Hover")
                .on_click({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Hover(!snap.hover))
                }),
        );

    div()
        .flex_col()
        .gap(theme.spacing.md)
        .child(heading(theme, "Overlays"))
        .child(buttons)
        .child(
            Dialog::new()
                .themed(theme)
                .title("Dialog")
                .open(snap.dialog)
                .child(
                    Label::new("Esc dismisses")
                        .themed(theme)
                        .muted(true)
                        .size(theme.typo.caption),
                )
                .on_dismiss({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Dialog(false))
                }),
        )
        .child(
            AlertDialog::new()
                .themed(theme)
                .title("Alert")
                .message("Confirm this action?")
                .open(snap.alert)
                .on_dismiss({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Alert(false))
                })
                .on_confirm({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Alert(false))
                }),
        )
        .child(
            Sheet::new()
                .themed(theme)
                .title("Sheet")
                .open(snap.sheet)
                .child(
                    Label::new("Side panel")
                        .themed(theme)
                        .muted(true)
                        .size(theme.typo.caption),
                )
                .on_dismiss({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Sheet(false))
                }),
        )
        .child(
            Popover::new()
                .themed(theme)
                .open(snap.popover)
                .child(Label::new("Popover").themed(theme).size(theme.typo.caption))
                .on_dismiss({
                    let click = fire.clone();
                    move || click(D8OverlayAction::Popover(false))
                }),
        )
        .child(
            HoverCard::new().themed(theme).open(snap.hover).child(
                Label::new("Hover card")
                    .themed(theme)
                    .size(theme.typo.caption),
            ),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        D8ButtonsSnapshot, D8DateSnapshot, D8FeedbackSnapshot, D8FormsSnapshot, D8InputSnapshot,
        D8OverlaySnapshot, D8SelectSnapshot, d8_buttons_catalog_types, d8_buttons_construct_all,
        d8_buttons_gallery, d8_date_catalog_types, d8_date_construct_all, d8_date_gallery,
        d8_display_catalog_types, d8_display_construct_all, d8_display_gallery,
        d8_feedback_catalog_types, d8_feedback_construct_all, d8_feedback_gallery,
        d8_forms_catalog_types, d8_forms_construct_all, d8_forms_gallery, d8_input_catalog_types,
        d8_input_construct_all, d8_input_gallery, d8_overlay_catalog_types,
        d8_overlay_construct_all, d8_overlay_gallery, d8_select_catalog_types,
        d8_select_construct_all, d8_select_gallery,
    };
    use crate::theme::Theme;

    #[test]
    fn catalog_lists_every_d81_type() {
        let names = d8_buttons_catalog_types();
        for need in [
            "Button",
            "ButtonGroup",
            "DropdownButton",
            "Toggle",
            "ToggleGroup",
            "Switch",
            "Checkbox",
            "Radio",
            "RadioGroup",
            "Link",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn catalog_constructs_every_d81_type() {
        let theme = Theme::aurora_dark();
        let built = d8_buttons_construct_all(&theme);
        assert_eq!(built.len(), d8_buttons_catalog_types().len());
        let _ = d8_buttons_gallery(
            &theme,
            D8ButtonsSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }

    #[test]
    fn catalog_lists_every_d82_type() {
        let names = d8_display_catalog_types();
        for need in [
            "Label",
            "Kbd",
            "Separator",
            "Badge",
            "Tag",
            "Avatar",
            "AvatarGroup",
            "Icon",
            "Image",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 9);
    }

    #[test]
    fn catalog_constructs_every_d82_type() {
        let theme = Theme::aurora_dark();
        let built = d8_display_construct_all(&theme);
        assert_eq!(built.len(), d8_display_catalog_types().len());
        let _ = d8_display_gallery(&theme);
    }

    #[test]
    fn display_gallery_paints_ink() {
        use knox_gpui::{
            HostTextSystem, PaintEnv, Point, RasterBuffer, Scene, Size, SpriteAtlas, WindowInput,
            paint_element_with_input,
        };

        let theme = Theme::aurora_dark();
        let mut root = d8_display_gallery(&theme);
        let mut atlas = SpriteAtlas::new();
        let sys = HostTextSystem;
        let mut env = PaintEnv {
            atlas: &mut atlas,
            text: Some(&sys),
        };
        let mut scene = Scene::new();
        let mut input = WindowInput::new();
        paint_element_with_input(
            &mut root,
            Point::ZERO,
            Size::wh(640, 480),
            &mut scene,
            &mut input,
            &mut env,
        );
        let mut buf = RasterBuffer::new(640, 480);
        buf.draw_with_atlas(&scene, Some(&atlas));
        let ink = (0..480)
            .flat_map(|y| (0..640).map(move |x| (x, y)))
            .filter(|&(x, y)| buf.pixel(x, y).map(|p| p.a > 0).unwrap_or(false))
            .count();
        assert!(ink > 200, "gallery should paint, got {ink} inked pixels");
    }

    #[test]
    fn catalog_lists_every_d83_type() {
        let names = d8_feedback_catalog_types();
        for need in [
            "Alert",
            "Empty",
            "Skeleton",
            "Shimmer",
            "Spinner",
            "Progress",
            "ProgressCircle",
            "Tooltip",
            "Clipboard",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 9);
    }

    #[test]
    fn catalog_constructs_every_d83_type() {
        let theme = Theme::aurora_dark();
        let built = d8_feedback_construct_all(&theme);
        assert_eq!(built.len(), d8_feedback_catalog_types().len());
        let _ = d8_feedback_gallery(
            &theme,
            D8FeedbackSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }

    #[test]
    fn catalog_lists_every_d84_type() {
        let names = d8_input_catalog_types();
        for need in ["Input", "Textarea", "InputGroup"] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn catalog_constructs_every_d84_type() {
        let theme = Theme::aurora_dark();
        let built = d8_input_construct_all(&theme);
        assert_eq!(built.len(), d8_input_catalog_types().len());
        let _ = d8_input_gallery(
            &theme,
            D8InputSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
            alloc::sync::Arc::new(|| None),
        );
    }

    #[test]
    fn catalog_lists_every_d85_type() {
        let names = d8_forms_catalog_types();
        for need in [
            "NumberInput",
            "OtpInput",
            "Slider",
            "ColorPicker",
            "Form",
            "Field",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 6);
    }

    #[test]
    fn catalog_constructs_every_d85_type() {
        let theme = Theme::aurora_dark();
        let built = d8_forms_construct_all(&theme);
        assert_eq!(built.len(), d8_forms_catalog_types().len());
        let _ = d8_forms_gallery(
            &theme,
            D8FormsSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }

    #[test]
    fn catalog_lists_every_d86_type() {
        let names = d8_select_catalog_types();
        for need in ["Select", "Combobox", "SearchableList"] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn catalog_constructs_every_d86_type() {
        let theme = Theme::aurora_dark();
        let built = d8_select_construct_all(&theme);
        assert_eq!(built.len(), d8_select_catalog_types().len());
        let _ = d8_select_gallery(
            &theme,
            D8SelectSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }

    #[test]
    fn catalog_lists_every_d87_type() {
        let names = d8_date_catalog_types();
        for need in ["Calendar", "DatePicker", "TimeField"] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn catalog_constructs_every_d87_type() {
        let theme = Theme::aurora_dark();
        let built = d8_date_construct_all(&theme);
        assert_eq!(built.len(), d8_date_catalog_types().len());
        let _ = d8_date_gallery(
            &theme,
            D8DateSnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }

    #[test]
    fn catalog_lists_every_d88_type() {
        let names = d8_overlay_catalog_types();
        for need in [
            "Dialog",
            "AlertDialog",
            "Sheet",
            "Popover",
            "HoverCard",
            "FocusTrap",
            "Positioner",
        ] {
            assert!(names.contains(&need), "missing {need}");
        }
        assert_eq!(names.len(), 7);
    }

    #[test]
    fn catalog_constructs_every_d88_type() {
        let theme = Theme::aurora_dark();
        let built = d8_overlay_construct_all(&theme);
        assert_eq!(built.len(), d8_overlay_catalog_types().len());
        let _ = d8_overlay_gallery(
            &theme,
            D8OverlaySnapshot::default(),
            alloc::sync::Arc::new(|_| {}),
        );
    }
}
