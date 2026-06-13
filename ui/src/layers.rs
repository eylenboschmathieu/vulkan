use std::collections::HashMap;
use std::hash::Hash;

/// Assigns root-children render/hit-test bands to a host-defined layer type
/// `L`, in registration order: the first distinct `layer` value seen by
/// [`band`](Self::band) becomes band `0` (bottom), the next becomes `1`, etc.
/// Stored type-erased in [`Ui`](crate::Ui) via `Box<dyn Any>` so
/// [`Ui`](crate::Ui) itself doesn't need to be generic over `L`.
pub(crate) struct LayerOrder<L> {
    bands: HashMap<L, u32>,
}

impl<L: Copy + Eq + Hash> LayerOrder<L> {
    pub fn new() -> Self {
        Self { bands: HashMap::new() }
    }

    /// Returns `layer`'s band index, assigning the next one (registration
    /// order) the first time `layer` is seen.
    pub fn band(&mut self, layer: L) -> u32 {
        let next = self.bands.len() as u32;
        *self.bands.entry(layer).or_insert(next)
    }
}
