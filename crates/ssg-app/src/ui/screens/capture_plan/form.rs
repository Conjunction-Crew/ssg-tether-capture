use super::*;

// ── Validation and serialization helpers ─────────────────────────────────

pub fn validate_form(form: &NewCapturePlanForm) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    let require_text = |val: &str, label: &str, errs: &mut Vec<String>| {
        if val.trim().is_empty() {
            errs.push(format!("{} is required", label));
        }
    };
    let require_number = |val: &str, label: &str, errs: &mut Vec<String>| {
        if val.trim().is_empty() {
            errs.push(format!("{} is required", label));
        } else if val.parse::<f64>().is_err() {
            errs.push(format!("{} must be a number", label));
        }
    };

    require_text(&form.plan_name, "Plan Name", &mut errors);
    require_text(&form.tether_name, "Tether Name", &mut errors);

    if form.tether_length.trim().is_empty() {
        errors.push("Tether Length is required".to_string());
    } else {
        match form.tether_length.parse::<f64>() {
            Ok(v) if v > 0.0 => {}
            Ok(_) => errors.push("Tether Length must be greater than zero".to_string()),
            Err(_) => errors.push("Tether Length must be a number".to_string()),
        }
    }

    require_number(
        &form.approach_max_velocity,
        "Approach Max Velocity",
        &mut errors,
    );
    require_number(&form.approach_max_force, "Approach Max Force", &mut errors);
    require_number(
        &form.terminal_max_velocity,
        "Terminal Max Velocity",
        &mut errors,
    );
    require_number(&form.terminal_max_force, "Terminal Max Force", &mut errors);
    require_number(
        &form.terminal_shrink_rate,
        "Terminal Shrink Rate",
        &mut errors,
    );
    require_number(
        &form.capture_max_velocity,
        "Capture Max Velocity",
        &mut errors,
    );
    require_number(&form.capture_max_force, "Capture Max Force", &mut errors);
    require_number(
        &form.capture_shrink_rate,
        "Capture Shrink Rate",
        &mut errors,
    );

    for (i, t) in form.approach_transitions.iter().enumerate() {
        if t.to.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} 'To State' is required",
                i + 1
            ));
        }
        if t.distance_kind.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} condition is required",
                i + 1
            ));
        }
        if t.distance_value.trim().is_empty() {
            errors.push(format!(
                "Approach Transition {} distance value is required",
                i + 1
            ));
        } else if t.distance_value.parse::<f64>().is_err() {
            errors.push(format!(
                "Approach Transition {} distance value must be a number",
                i + 1
            ));
        }
    }

    for (i, t) in form.terminal_transitions.iter().enumerate() {
        if t.to.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} 'To State' is required",
                i + 1
            ));
        }
        if t.distance_kind.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} condition is required",
                i + 1
            ));
        }
        if t.distance_value.trim().is_empty() {
            errors.push(format!(
                "Terminal Transition {} distance value is required",
                i + 1
            ));
        } else if t.distance_value.parse::<f64>().is_err() {
            errors.push(format!(
                "Terminal Transition {} distance value must be a number",
                i + 1
            ));
        }
    }

    errors
}

pub fn generate_filename(plan_name: &str) -> String {
    // Characters illegal on Windows (superset of what POSIX disallows)
    const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let sanitized: String = plan_name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c == ' ' || ILLEGAL.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{}.json", sanitized)
}

fn unit_conv_linear(s: &str, unit: UnitSystem) -> f64 {
    let v = s.parse::<f64>().unwrap_or(0.0);
    if unit == UnitSystem::Imperial {
        v * 0.3048
    } else {
        v
    }
}

fn unit_conv_force(s: &str, unit: UnitSystem) -> f64 {
    let v = s.parse::<f64>().unwrap_or(0.0);
    if unit == UnitSystem::Imperial {
        v * 4.44822
    } else {
        v
    }
}

pub fn build_capture_plan_json(form: &NewCapturePlanForm) -> serde_json::Value {
    use serde_json::{Value, json};

    let unit = form.unit_system;
    let make_transitions = |transitions: &[TransitionForm]| -> Value {
        let arr: Vec<Value> = transitions
            .iter()
            .map(|t| {
                let distance_val = unit_conv_linear(&t.distance_value, unit);
                let dist = json!({ t.distance_kind.clone(): distance_val });
                json!({ "to": t.to.trim(), "distance": dist })
            })
            .collect();
        Value::Array(arr)
    };

    let approach_transitions = make_transitions(&form.approach_transitions);
    let terminal_transitions = make_transitions(&form.terminal_transitions);

    json!({
        "name": form.plan_name.trim(),
        "tether": form.tether_name.trim(),
        "device": {
            "type": form.tether_type.trim(),
            "tether_length": form.tether_length.parse::<f64>().unwrap_or(20.0)
        },
        "states": [
            {
                "id": "approach",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.approach_max_velocity, unit),
                    "max_force": unit_conv_force(&form.approach_max_force, unit)
                },
                "transitions": approach_transitions
            },
            {
                "id": "terminal",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.terminal_max_velocity, unit),
                    "max_force": unit_conv_force(&form.terminal_max_force, unit),
                    "shrink_rate": unit_conv_linear(&form.terminal_shrink_rate, unit)
                },
                "transitions": terminal_transitions
            },
            {
                "id": "capture",
                "parameters": {
                    "max_velocity": unit_conv_linear(&form.capture_max_velocity, unit),
                    "max_force": unit_conv_force(&form.capture_max_force, unit),
                    "shrink_rate": unit_conv_linear(&form.capture_shrink_rate, unit)
                }
            }
        ]
    })
}
