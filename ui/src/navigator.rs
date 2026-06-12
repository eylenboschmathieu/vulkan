use std::collections::HashMap;
use std::hash::Hash;

/// Tracks the active screen and the registered screen/route tables for a
/// host-defined screen type `S`. Stored type-erased in [`Ui`](crate::Ui) via
/// `Box<dyn Any>` so [`Ui`](crate::Ui) itself doesn't need to be generic over
/// `S`.
pub(crate) struct Navigator<S> {
    pub current: S,
    pub screens: HashMap<S, usize>,
    pub routes: HashMap<usize, S>,
}

impl<S: Copy + Eq + Hash> Navigator<S> {
    pub fn new(initial: S) -> Self {
        Self { current: initial, screens: HashMap::new(), routes: HashMap::new() }
    }
}
