/// The minimal input state the UI needs to hit-test, drag sliders, and fire
/// button/checkbox callbacks: where the cursor is, and the state of a single
/// logical "primary" button. Decoupled from the host's own input system so
/// the UI doesn't need to know about its actions or bindings — the host is
/// responsible for translating its input into this each frame.
pub struct UiInput {
    cursor: (f32, f32),
    primary_held: bool,
    primary_pressed: bool,
    primary_released: bool,
}

impl UiInput {
    pub fn new(cursor: (f32, f32), primary_held: bool, primary_pressed: bool, primary_released: bool) -> Self {
        Self { cursor, primary_held, primary_pressed, primary_released }
    }

    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    pub fn primary_held(&self) -> bool {
        self.primary_held
    }

    pub fn primary_pressed(&self) -> bool {
        self.primary_pressed
    }

    pub fn primary_released(&self) -> bool {
        self.primary_released
    }
}
