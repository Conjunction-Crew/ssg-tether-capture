//! Simulation core: everything that runs headless (time, orbits, the physics
//! bubble, the floating origin, the tether, capture, logging, project files).
//!
//! This tree is the future `ssg-sim` crate (P3 S3). Nothing under `sim/` may name
//! `crate::render`, `crate::ui`, `crate::resolve_assets_dir` or a render, window
//! or UI type.

pub mod anchor;
pub mod capture;
pub mod floating_origin;
pub mod orbit;
pub mod physics_bubble;
pub mod project;
pub mod session;
pub mod telemetry;
pub mod tether;
pub mod time;
