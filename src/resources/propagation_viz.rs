use std::collections::VecDeque;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::systems::hill_frame::HillBasis;

/// A single sample of a tether node's position relative to the tether root,
/// expressed both in CW/Hill axes (nadir, along-track, cross-track, metres)
/// and as a renderable world-space point (for gizmo trails).
pub type CwSample = (f64, f64, f64, f64, Vec3);

/// Ring buffer of CW-frame samples for one tether node.
#[derive(Debug, Default)]
pub struct NodeCwHistory {
    pub samples: VecDeque<CwSample>,
}

impl NodeCwHistory {
    const MAX_SAMPLES: usize = 2000;

    pub fn push(&mut self, sample: CwSample) {
        if self.samples.len() >= Self::MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
}

/// History of each tether node's position in the reference orbit's Hill/CW
/// frame, collected while a propagation sim is running. Used to drive the CW
/// ellipse/time-series egui plots and the Hill-frame gizmo overlay.
#[derive(Resource, Debug, Default)]
pub struct PropagationVizData {
    pub nodes: HashMap<Entity, NodeCwHistory>,
    pub current_basis: Option<HillBasis>,
    pub root_world_pos: Vec3,
}

impl PropagationVizData {
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.current_basis = None;
        self.root_world_pos = Vec3::ZERO;
    }
}
