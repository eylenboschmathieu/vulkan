use std::collections::HashSet;

/// Mouse buttons the UI distinguishes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MouseButton {
    Primary   = 0,
    Secondary = 1,
    Middle    = 2,
}

/// Keys the UI distinguishes for navigation and text editing. Printable
/// character input is carried separately via [`UiInput::text`], since it
/// depends on keyboard layout and modifier state (e.g. Shift+A -> 'A').
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Tab, Enter, Escape, Backspace, Delete, Space,
    ArrowLeft, ArrowRight, ArrowUp, ArrowDown,
    Home, End,
    Shift, Control, Alt,
}

/// The input state the UI needs to hit-test, drag sliders, fire
/// button/checkbox callbacks, and drive text fields: cursor position, mouse
/// button states, keyboard key states, and any text typed this frame.
/// Decoupled from the host's own input system — the host is responsible for
/// translating its input into this each frame.
#[derive(Default)]
pub struct UiInput {
    cursor: (f32, f32),

    mouse_held:     [bool; 3],
    mouse_pressed:  [bool; 3],
    mouse_released: [bool; 3],

    keys_held:     HashSet<Key>,
    keys_pressed:  HashSet<Key>,
    keys_released: HashSet<Key>,

    /// Text typed this frame, after keyboard-layout and modifier resolution
    /// (e.g. Shift+A -> "A").
    text: String,

    /// Opaque, host-defined name of a key pressed this frame (e.g. "KeyW",
    /// "F1", "ArrowUp"), set by the host while [`crate::Ui`] is in
    /// key-capture mode (see [`crate::Ui::start_key_capture`]). Unlike
    /// [`Key`], which only covers UI-navigation keys, this can represent any
    /// key the host's input system knows about. `None` if no key was pressed
    /// this frame.
    captured_key: Option<String>,

    /// Scroll-wheel delta for this frame, in wheel "lines" — typically ±1.0
    /// per discrete notch from `MouseScrollDelta::LineDelta`, or a
    /// fractional value for smooth/trackpad scrolling. Routed to the nearest
    /// scroll-enabled ancestor of the hovered node by
    /// [`crate::Ui::handle_input`], which converts it to pixels (using the
    /// panel's [`crate::Scroll::scrollbar`] step size along that slider's
    /// axis, so wheel-scrolling matches its step buttons) before adding it
    /// to that panel's offset.
    scroll: (f32, f32),
}

impl UiInput {
    pub fn new(cursor: (f32, f32)) -> Self {
        Self { cursor, ..Default::default() }
    }

    /// Sets the held/pressed/released state of a mouse button for this frame.
    pub fn with_mouse_button(mut self, button: MouseButton, held: bool, pressed: bool, released: bool) -> Self {
        let i = button as usize;
        self.mouse_held[i]     = held;
        self.mouse_pressed[i]  = pressed;
        self.mouse_released[i] = released;
        self
    }

    /// Sets the held/pressed/released state of a key for this frame.
    pub fn with_key(mut self, key: Key, held: bool, pressed: bool, released: bool) -> Self {
        if held     { self.keys_held.insert(key); }
        if pressed  { self.keys_pressed.insert(key); }
        if released { self.keys_released.insert(key); }
        self
    }

    /// Sets the text typed this frame.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Sets the host-defined name of a key pressed this frame, for
    /// key-capture mode (see [`UiInput::captured_key`]).
    pub fn with_captured_key(mut self, key: impl Into<String>) -> Self {
        self.captured_key = Some(key.into());
        self
    }

    /// Sets the scroll-wheel delta for this frame, in wheel lines.
    pub fn with_scroll_delta(mut self, delta: (f32, f32)) -> Self {
        self.scroll = delta;
        self
    }

    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    pub fn button_held(&self, button: MouseButton) -> bool {
        self.mouse_held[button as usize]
    }

    pub fn button_pressed(&self, button: MouseButton) -> bool {
        self.mouse_pressed[button as usize]
    }

    pub fn button_released(&self, button: MouseButton) -> bool {
        self.mouse_released[button as usize]
    }

    pub fn primary_held(&self) -> bool {
        self.button_held(MouseButton::Primary)
    }

    pub fn primary_pressed(&self) -> bool {
        self.button_pressed(MouseButton::Primary)
    }

    pub fn primary_released(&self) -> bool {
        self.button_released(MouseButton::Primary)
    }

    /// True if any mouse button changed state (pressed or released) this frame.
    pub fn any_click(&self) -> bool {
        self.mouse_pressed.iter().any(|&p| p) || self.mouse_released.iter().any(|&r| r)
    }

    pub fn key_held(&self, key: Key) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn key_pressed(&self, key: Key) -> bool {
        self.keys_pressed.contains(&key)
    }

    pub fn key_released(&self, key: Key) -> bool {
        self.keys_released.contains(&key)
    }

    /// Text typed this frame, if any.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The host-defined name of a key pressed this frame, if any — see
    /// [`UiInput::captured_key`].
    pub fn captured_key(&self) -> Option<&str> {
        self.captured_key.as_deref()
    }

    /// Scroll-wheel delta for this frame, in wheel lines.
    pub fn scroll_delta(&self) -> (f32, f32) {
        self.scroll
    }
}
