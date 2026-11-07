mod form;
pub mod window;

use bevy::prelude::*;

use crate::ui::form::{NewCapturePlanForm, TransitionForm, UnitSystem};

pub use form::{build_capture_plan_json, generate_filename, validate_form};
pub use window::capture_plan_form_window;
