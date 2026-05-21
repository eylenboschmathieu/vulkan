#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::{HashMap, HashSet}, fs::File};

use winit::{self,
    event::{
        ElementState,
        MouseButton
    },
    keyboard::KeyCode, window::Window
};

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Action {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    Jump,
    Prone,
    Crouch,
    ToggleMouseLock,
    PrimaryAction,
    SecondaryAction,
    ToggleMenu,
    Quit,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Input {
    Keyboard(KeyCode),
    Mouse(MouseButton),
}

impl From<KeyCode> for Input {
    fn from(k: KeyCode) -> Self { Input::Keyboard(k) }
}

impl From<MouseButton> for Input {
    fn from(b: MouseButton) -> Self { Input::Mouse(b) }
}

#[derive(Debug)]
pub struct Binding {
    pub first: Option<Input>,
    pub second: Option<Input>,
}

#[derive(Debug)]
pub struct InputBindings {
    bindings: HashMap<Action, Binding>,
    reverse:  HashMap<Input, Action>,
}

impl InputBindings {
    pub fn default() -> Self {
        let mut s = Self { bindings: HashMap::new(), reverse: HashMap::new() };

        s.bind(Action::ToggleMenu,      Binding { first: Some(Input::Keyboard(KeyCode::Escape)),    second: None });
        s.bind(Action::MoveForward,     Binding { first: Some(Input::Keyboard(KeyCode::ArrowUp)),    second: None });
        s.bind(Action::MoveBackward,    Binding { first: Some(Input::Keyboard(KeyCode::ArrowDown)),  second: None });
        s.bind(Action::MoveLeft,        Binding { first: Some(Input::Keyboard(KeyCode::ArrowLeft)),  second: None });
        s.bind(Action::MoveRight,       Binding { first: Some(Input::Keyboard(KeyCode::ArrowRight)), second: None });
        s.bind(Action::Jump,            Binding { first: Some(Input::Keyboard(KeyCode::Numpad1)),    second: None });
        s.bind(Action::Crouch,          Binding { first: Some(Input::Keyboard(KeyCode::Numpad2)),    second: None });
        s.bind(Action::Prone,           Binding { first: Some(Input::Keyboard(KeyCode::Numpad3)),    second: None });
        s.bind(Action::PrimaryAction,   Binding { first: Some(Input::Mouse(MouseButton::Left)),      second: None });
        s.bind(Action::SecondaryAction, Binding { first: Some(Input::Mouse(MouseButton::Right)),     second: None });
        s.bind(Action::ToggleMouseLock, Binding { first: Some(Input::Keyboard(KeyCode::KeyC)),       second: Some(Input::Keyboard(KeyCode::KeyL)) });
        s.bind(Action::Quit,            Binding { first: Some(Input::Keyboard(KeyCode::KeyQ)),       second: None });

        s
    }

    // TODO
    pub fn from_file(file: File) -> Self {
        Self { bindings: HashMap::new(), reverse: HashMap::new() }
    }

    // Applies the binding, stealing any conflicting inputs from their current actions.
    // Returns false if a conflict was encountered (and resolved), true if clean.
    pub fn bind(&mut self, action: Action, binding: Binding) -> bool {
        let inputs: Vec<Input> = [binding.first, binding.second]
            .into_iter().flatten().collect();

        let mut had_conflict = false;

        for &input in &inputs {
            if let Some(&owner) = self.reverse.get(&input) {
                if owner != action {
                    had_conflict = true;
                    if let Some(b) = self.bindings.get_mut(&owner) {
                        if b.first  == Some(input) { b.first  = None; }
                        if b.second == Some(input) { b.second = None; }
                    }
                    self.reverse.remove(&input);
                }
            }
        }

        if let Some(old) = self.bindings.get(&action) {
            if let Some(i) = old.first  { if !inputs.contains(&i) { self.reverse.remove(&i); } }
            if let Some(i) = old.second { if !inputs.contains(&i) { self.reverse.remove(&i); } }
        }

        for &input in &inputs {
            self.reverse.insert(input, action);
        }

        self.bindings.entry(action).insert_entry(binding);
        !had_conflict
    }

    pub fn unbind(&mut self, action: Action) {
        if let Some(binding) = self.bindings.remove(&action) {
            if let Some(i) = binding.first  { self.reverse.remove(&i); }
            if let Some(i) = binding.second { self.reverse.remove(&i); }
        }
    }
}

#[derive(Debug)]
pub struct InputState {
    held:     HashSet<Action>,
    pressed:  HashSet<Action>,
    released: HashSet<Action>,
}

impl InputState {
    fn update(&mut self, action: Action, state: ElementState) {
        if state.is_pressed() {
            if !self.held.contains(&action) {
                self.pressed.insert(action);
            }
            self.held.insert(action);
        } else {
            self.held.remove(&action);
            self.released.insert(action);
        }
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

#[derive(Debug)]
pub struct InputManager {
    pub bindings: InputBindings,
    pub state:    InputState,
    cursor: (f32, f32), // Mouse position on screen in range of [0, 0] -> [screen_width, screen_height]
    window_size: (f32, f32), // inner width and height of the window
}

impl InputManager {
    pub fn new(window: &Window) -> Self {
        let area = window.inner_size();  
        Self {
            bindings: InputBindings::default(),
            state: InputState {
                held:     HashSet::new(),
                pressed:  HashSet::new(),
                released: HashSet::new(),
            },
            cursor: ( (area.width as f32) / 2.0, (area.height as f32) / 2.0 ),
            window_size: (area.width as f32, area.height as f32),
        }
    }

    /// Update state for keyboard and mouse button
    pub fn button_update<T: Into<Input>>(&mut self, button: T, state: ElementState) {
        if let Some(&action) = self.bindings.reverse.get(&button.into()) {
            self.state.update(action, state);
        }
    }

    pub fn cursor_update(&mut self, x: f32, y: f32) {
        self.cursor = (x, y);
    }

    pub fn is_held(&self, action: Action) -> bool {
        self.state.held.contains(&action)
    }

    pub fn is_pressed(&self, action: Action) -> bool {
        self.state.pressed.contains(&action)
    }

    pub fn is_released(&self, action: Action) -> bool {
        self.state.released.contains(&action)
    }

    pub fn cursor(&self) -> (f32, f32) {
        (self.cursor.0.clamp(0.0, self.window_size.0), self.cursor.1.clamp(0.0, self.window_size.1))
    }
}
