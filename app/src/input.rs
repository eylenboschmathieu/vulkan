#![allow(dead_code, unsafe_op_in_unsafe_fn, unused_variables, clippy::too_many_arguments, clippy::unnecessary_wraps)]

use std::{collections::{HashMap, HashSet}, fs::File};

use winit::{self,
    event::{
        ElementState,
        MouseButton
    },
    keyboard::KeyCode
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
    AddBlock,
    RemoveBlock,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Input {
    Keyboard(KeyCode),
    Mouse(MouseButton),
}

#[derive(Debug)]
struct Binding {
    pub first: Option<Input>,
    pub second: Option<Input>,
}

#[derive(Debug)]
pub struct InputBindings {
    bindings: HashMap<Action, Binding>
}

impl InputBindings {
    pub fn default() -> Self {
        let mut bindings = HashMap::new();

        bindings.insert(Action::MoveForward,        Binding{ first: Some(Input::Keyboard(KeyCode::ArrowUp)), second: None});
        bindings.insert(Action::MoveBackward,       Binding{ first: Some(Input::Keyboard(KeyCode::ArrowDown)), second: None});
        bindings.insert(Action::MoveLeft,           Binding{ first: Some(Input::Keyboard(KeyCode::ArrowLeft)), second: None});
        bindings.insert(Action::MoveRight,          Binding{ first: Some(Input::Keyboard(KeyCode::ArrowRight)), second: None});
        bindings.insert(Action::Jump,               Binding{ first: Some(Input::Keyboard(KeyCode::Numpad1)), second: None});
        bindings.insert(Action::Crouch,             Binding{ first: Some(Input::Keyboard(KeyCode::Numpad2)), second: None});
        bindings.insert(Action::Prone,              Binding{ first: Some(Input::Keyboard(KeyCode::Numpad3)), second: None});
        bindings.insert(Action::AddBlock,           Binding{ first: Some(Input::Mouse(MouseButton::Left)), second: None});
        bindings.insert(Action::RemoveBlock,        Binding{ first: Some(Input::Mouse(MouseButton::Right)), second: None});
        bindings.insert(Action::ToggleMouseLock,    Binding{ first: Some(Input::Keyboard(KeyCode::KeyC)), second: Some(Input::Keyboard(KeyCode::KeyL))});

        Self { bindings }
    }
    
    // TODO
    pub fn from_file(file: File) -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn bind(&mut self, action: Action, binding: Binding) {
        self.bindings.entry(action).insert_entry(binding);
    }

    pub fn unbind(&mut self, action: Action) {
        self.bindings.remove(&action);
    }
}

#[derive(Debug)]
pub struct InputState {
    held: HashSet<Input>,
    pressed: HashSet<Input>,
    released: HashSet<Input>,
}

impl InputState {
    pub fn update_key(&mut self, button: KeyCode, state: ElementState) {
        let input = Input::Keyboard(button);
        if state.is_pressed() {
            if !self.held.contains(&input) {
                self.pressed.insert(input.clone());
            }
            self.held.insert(input);
        } else {
            self.held.remove(&input);
            self.released.insert(input);
        }
    }

    pub fn update_mouse(&mut self, button: MouseButton, state: ElementState) {
        let input = Input::Mouse(button);
        if state.is_pressed() {
            if !self.held.contains(&input) {
                self.pressed.insert(input.clone());
            }
            self.held.insert(input);
        } else {
            self.held.remove(&input);
            self.released.insert(input);
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
    pub state: InputState,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            bindings: InputBindings::default(),
            state: InputState {
                held:     HashSet::new(),
                pressed:  HashSet::new(),
                released: HashSet::new(),
            },
        }
    }

    pub fn is_held(&self, action: Action) -> bool {
        self.bindings.bindings
            .get(&action)
            .map(|binding| {
                binding.first.map_or(false,  |b| self.state.held.contains(&b))
                || binding.second.map_or(false, |b| self.state.held.contains(&b))
            }).unwrap_or(false)
    }

    pub fn is_pressed(&self, action: Action) -> bool {
        self.bindings.bindings
            .get(&action)
            .map(|binding| {
                binding.first.map_or(false,  |b| self.state.pressed.contains(&b))
                || binding.second.map_or(false, |b| self.state.pressed.contains(&b))
            }).unwrap_or(false)
    }

    pub fn is_released(&self, action: Action) -> bool {
        self.bindings.bindings
            .get(&action)
            .map(|binding| {
                binding.first.map_or(false,  |b| self.state.released.contains(&b))
                || binding.second.map_or(false, |b| self.state.released.contains(&b))
            }).unwrap_or(false)
    }
}