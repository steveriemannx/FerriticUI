//! Interaction tests for the generated static UI: hit testing, message
//! emission and focus order, without a window or GPU.

use ferritic::prelude::*;
use ferritic::ui;

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Clicked,
}

ui! {
    msg = Msg;
    struct TestUi {
        Column(padding: 10.0, spacing: 0.0) {
            button: Button<Msg>(text: "Click me", on_click: Msg::Clicked),
        }
    }
}

fn prepare(ui: &mut TestUi, text: &mut dyn TextEngine, size: Size) {
    let _ = ui.measure(text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));
}

#[test]
fn clicking_the_button_emits_its_message() {
    let theme = Theme::light();
    let mut ui = TestUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    prepare(&mut ui, &mut text, Size::new(200.0, 120.0));

    let mut cx = EventCtx::default();
    // The button sits at the root Column's padding: (10, 10).
    let position = Point::new(20.0, 20.0);

    let pressed = ui.event(
        &mut cx,
        &Event::PointerDown(position, PointerButton::Primary),
        &mut text,
    );
    assert_eq!(pressed, None);

    let released = ui.event(
        &mut cx,
        &Event::PointerUp(position, PointerButton::Primary),
        &mut text,
    );
    assert_eq!(released, Some(Msg::Clicked));
}

#[test]
fn clicking_outside_the_button_emits_nothing() {
    let theme = Theme::light();
    let mut ui = TestUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    prepare(&mut ui, &mut text, Size::new(200.0, 120.0));

    let mut cx = EventCtx::default();
    let position = Point::new(180.0, 100.0);
    ui.event(
        &mut cx,
        &Event::PointerDown(position, PointerButton::Primary),
        &mut text,
    );
    let released = ui.event(
        &mut cx,
        &Event::PointerUp(position, PointerButton::Primary),
        &mut text,
    );
    assert_eq!(released, None);
}

#[test]
fn tab_moves_focus_to_the_button() {
    let theme = Theme::light();
    let mut ui = TestUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    prepare(&mut ui, &mut text, Size::new(200.0, 120.0));

    let mut cx = EventCtx::default();
    assert_eq!(cx.focus, None);
    ui.event(
        &mut cx,
        &Event::Key {
            key: Key::Tab,
            pressed: true,
            repeat: false,
            shift: false,
            ctrl: false,
        },
        &mut text,
    );
    // The first focusable node (the only button) becomes focused.
    assert!(cx.focus.is_some());
}


#[derive(Clone, Debug, PartialEq)]
enum CheckMsg {
    Toggled,
}

ui! {
    msg = CheckMsg;
    struct CheckUi {
        Column(padding: 10.0, spacing: 0.0) {
            check: Checkbox<CheckMsg>(text: "Enable", on_change: CheckMsg::Toggled),
        }
    }
}

#[test]
fn clicking_a_checkbox_toggles_it_and_emits_its_message() {
    let theme = Theme::light();
    let mut ui = CheckUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(200.0, 120.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    assert!(!ui.check.checked);
    let mut cx = EventCtx::default();
    let position = Point::new(20.0, 20.0);
    ui.event(
        &mut cx,
        &Event::PointerDown(position, PointerButton::Primary),
        &mut text,
    );
    let message = ui.event(
        &mut cx,
        &Event::PointerUp(position, PointerButton::Primary),
        &mut text,
    );
    assert_eq!(message, Some(CheckMsg::Toggled));
    assert!(ui.check.checked);
}


ui! {
    msg = Msg;
    struct ScrollTest {
        scroll: Scroll(padding: 2.0, spacing: 2.0) {
            a: Button<Msg>(text: "A", on_click: Msg::Clicked),
            b: Button<Msg>(text: "B", on_click: Msg::Clicked),
            c: Button<Msg>(text: "C", on_click: Msg::Clicked),
            d: Button<Msg>(text: "D", on_click: Msg::Clicked),
            e: Button<Msg>(text: "E", on_click: Msg::Clicked),
            f: Button<Msg>(text: "F", on_click: Msg::Clicked),
        }
    }
}

#[test]
fn wheel_scrolls_the_scroll_container_and_clamps() {
    let theme = Theme::light();
    let mut ui = ScrollTest::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(200.0, 100.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    assert!(ui.scroll.content_height > ui.scroll.viewport_height);
    assert_eq!(ui.scroll.offset, 0.0);

    let mut cx = EventCtx {
        pointer: Point::new(100.0, 50.0),
        ..EventCtx::default()
    };
    ui.event(&mut cx, &Event::Wheel(-1.0), &mut text);
    assert!(ui.scroll.offset > 0.0);
    assert!(cx.needs_redraw);

    ui.event(&mut cx, &Event::Wheel(-1000.0), &mut text);
    assert_eq!(ui.scroll.offset, ui.scroll.content_height - ui.scroll.viewport_height);

    ui.event(&mut cx, &Event::Wheel(1000.0), &mut text);
    assert_eq!(ui.scroll.offset, 0.0);
}


#[derive(Clone, Debug, PartialEq)]
enum TextMsg {
    Changed,
}

ui! {
    msg = TextMsg;
    struct TextUi {
        Column(padding: 10.0, spacing: 6.0) {
            input: TextInput<TextMsg>(placeholder: "Type", on_change: TextMsg::Changed),
        }
    }
}

ui! {
    msg = TextMsg;
    struct TwoInputsUi {
        Column(padding: 10.0, spacing: 6.0) {
            first: TextInput<TextMsg>(placeholder: "First", on_change: TextMsg::Changed),
            second: TextInput<TextMsg>(placeholder: "Second", on_change: TextMsg::Changed),
        }
    }
}

fn key_event(key: Key, shift: bool, ctrl: bool) -> Event {
    Event::Key {
        key,
        pressed: true,
        repeat: false,
        shift,
        ctrl,
    }
}

fn click(ui: &mut TextUi, cx: &mut EventCtx, text: &mut dyn TextEngine) {
    let position = Point::new(30.0, 25.0);
    ui.event(cx, &Event::PointerDown(position, PointerButton::Primary), text);
    ui.event(cx, &Event::PointerUp(position, PointerButton::Primary), text);
}

#[test]
fn text_input_focuses_on_click_and_edits_with_keys() {
    let theme = Theme::light();
    let mut ui = TextUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(200.0, 80.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    let mut cx = EventCtx::default();
    click(&mut ui, &mut cx, &mut text);
    assert!(cx.focus.is_some());
    assert!(ui.input.focused);
    assert!(cx.ime_allowed);

    let changed = ui.event(&mut cx, &key_event(Key::Character('a'), false, false), &mut text);
    assert_eq!(changed, Some(TextMsg::Changed));
    ui.event(&mut cx, &key_event(Key::Character('b'), false, false), &mut text);
    assert_eq!(ui.input.text, "ab");

    ui.event(&mut cx, &key_event(Key::Backspace, false, false), &mut text);
    assert_eq!(ui.input.text, "a");

    // Ctrl+A selects all and Ctrl+C copies the selection.
    ui.event(&mut cx, &key_event(Key::Character('a'), false, true), &mut text);
    ui.event(&mut cx, &key_event(Key::Character('c'), false, true), &mut text);
    assert_eq!(cx.clipboard_set.as_deref(), Some("a"));

    // Ctrl+V replaces the selection.
    cx.clipboard_get = Some("Z".to_string());
    ui.event(&mut cx, &key_event(Key::Character('v'), false, true), &mut text);
    assert_eq!(ui.input.text, "Z");
}

#[test]
fn text_input_commits_ime_text() {
    let theme = Theme::light();
    let mut ui = TextUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(200.0, 80.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    let mut cx = EventCtx::default();
    click(&mut ui, &mut cx, &mut text);
    ui.event(&mut cx, &Event::ImePreedit("ni".to_string()), &mut text);
    assert_eq!(ui.input.preedit, "ni");
    let changed = ui.event(&mut cx, &Event::ImeCommit("你好".to_string()), &mut text);
    assert_eq!(changed, Some(TextMsg::Changed));
    assert_eq!(ui.input.text, "你好");
    assert!(ui.input.preedit.is_empty());
}

#[test]
fn tab_moves_focus_between_text_inputs() {
    let theme = Theme::light();
    let mut ui = TwoInputsUi::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(220.0, 120.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    let mut cx = EventCtx::default();
    let position = Point::new(30.0, 25.0);
    ui.event(&mut cx, &Event::PointerDown(position, PointerButton::Primary), &mut text);
    ui.event(&mut cx, &Event::PointerUp(position, PointerButton::Primary), &mut text);
    assert!(ui.first.focused);
    assert!(!ui.second.focused);

    ui.event(&mut cx, &key_event(Key::Tab, false, false), &mut text);
    assert!(!ui.first.focused);
    assert!(ui.second.focused);
}


#[derive(Clone, Debug, PartialEq)]
enum ListMsg {
    Selected,
}

ui! {
    msg = ListMsg;
    struct ListTest {
        list: List<ListMsg>(row_height: 20.0, on_select: ListMsg::Selected)
    }
}

fn list_ui(text: &mut dyn TextEngine) -> (ListTest, EventCtx) {
    let theme = Theme::light();
    let mut ui = ListTest::new(&theme);
    ui.list
        .set_items((0..100).map(|index| format!("Item {index}")).collect());
    let size = Size::new(200.0, 100.0);
    let _ = ui.measure(text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));
    (ui, EventCtx::default())
}

#[test]
fn list_click_selects_a_row_and_emits_its_message() {
    let mut text = ferritic::SystemText::new().expect("a system font");
    let (mut ui, mut cx) = list_ui(&mut text);

    // Row 2 starts at y = 40, is 20 pixels tall.
    let position = Point::new(50.0, 50.0);
    ui.event(&mut cx, &Event::PointerDown(position, PointerButton::Primary), &mut text);
    let message = ui.event(&mut cx, &Event::PointerUp(position, PointerButton::Primary), &mut text);
    assert_eq!(message, Some(ListMsg::Selected));
    assert_eq!(ui.list.selected, Some(2));
    assert!(cx.focus.is_some());
}

#[test]
fn list_wheel_and_keyboard_navigate_and_clamp() {
    let mut text = ferritic::SystemText::new().expect("a system font");
    let (mut ui, mut cx) = list_ui(&mut text);

    let position = Point::new(50.0, 50.0);
    ui.event(&mut cx, &Event::PointerDown(position, PointerButton::Primary), &mut text);
    ui.event(&mut cx, &Event::PointerUp(position, PointerButton::Primary), &mut text);

    ui.event(&mut cx, &Event::Wheel(-1.0), &mut text);
    assert!(ui.list.offset > 0.0);

    let down = key_event(Key::ArrowDown, false, false);
    let message = ui.event(&mut cx, &down, &mut text);
    assert_eq!(message, Some(ListMsg::Selected));
    assert_eq!(ui.list.selected, Some(3));

    ui.event(&mut cx, &key_event(Key::End, false, false), &mut text);
    assert_eq!(ui.list.selected, Some(99));
    assert!(ui.list.offset > 0.0);

    ui.event(&mut cx, &key_event(Key::Home, false, false), &mut text);
    assert_eq!(ui.list.selected, Some(0));
    assert_eq!(ui.list.offset, 0.0);

    ui.event(&mut cx, &key_event(Key::PageDown, false, false), &mut text);
    assert!(ui.list.offset >= 100.0);
}


#[derive(Clone, Debug, PartialEq)]
enum SliderMsg {
    Changed,
}

ui! {
    msg = SliderMsg;
    struct SliderTest {
        slider: Slider<SliderMsg>(on_change: SliderMsg::Changed)
    }
}

#[test]
fn slider_drag_and_keys_change_its_value() {
    let theme = Theme::light();
    let mut ui = SliderTest::new(&theme);
    let mut text = ferritic::SystemText::new().expect("a system font");
    let size = Size::new(200.0, 40.0);
    let _ = ui.measure(&mut text, Constraints::loose(size));
    ui.arrange(Rect::from_size(size));

    // The track spans thumb_radius..width-thumb_radius: 9..191.
    let mut cx = EventCtx::default();
    let down = Point::new(9.0 + 182.0 * 0.75, 20.0);
    ui.event(&mut cx, &Event::PointerDown(down, PointerButton::Primary), &mut text);
    assert!((ui.slider.value - 0.75).abs() < 0.02);
    assert!(cx.focus.is_some());

    let moved = Point::new(9.0 + 182.0 * 0.25, 20.0);
    let message = ui.event(&mut cx, &Event::PointerMoved(moved), &mut text);
    assert_eq!(message, Some(SliderMsg::Changed));
    assert!((ui.slider.value - 0.25).abs() < 0.02);
    ui.event(&mut cx, &Event::PointerUp(moved, PointerButton::Primary), &mut text);

    let before = ui.slider.value;
    ui.event(&mut cx, &key_event(Key::ArrowRight, false, false), &mut text);
    assert!(ui.slider.value > before);
    ui.event(&mut cx, &key_event(Key::End, false, false), &mut text);
    assert_eq!(ui.slider.value, 1.0);
    ui.event(&mut cx, &key_event(Key::Home, false, false), &mut text);
    assert_eq!(ui.slider.value, 0.0);
}
