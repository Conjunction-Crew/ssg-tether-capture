use bevy::prelude::*;

use crate::systems::propagation::init_orbitals;

pub struct OrbitalMechanicsPlugin;

impl Plugin for OrbitalMechanicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, init_orbitals);
    }
}
