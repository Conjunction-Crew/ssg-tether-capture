pub(crate) mod form;
mod phases;
pub mod window;

use crate::ui::form::{NewCapturePlanForm, TransitionForm, UnitSystem};

pub(crate) use form::{
    NEW_PLAN_TEMPLATE, field_pointers, load_plan_into_form, new_plan_into_form, pointer,
    save_plan_from_form,
};
pub use window::capture_plan_form_window;
