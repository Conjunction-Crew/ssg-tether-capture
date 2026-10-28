//! The body the floating origin and the physics bubble are centred on.

use bevy::prelude::*;

/// The body the floating origin and the physics bubble are centred on (Q15): the
/// tether root, inserted by `spawn_tether`. The app's camera target is a separate,
/// app-side marker and never moves it (F-25).
#[derive(Component, Debug, Clone, Copy)]
pub struct SimAnchor;
