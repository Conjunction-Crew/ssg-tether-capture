//! Migration of a v0 plan file (no `schema_version`) to the v1 model, in memory
//! (design D4). A value v0 ignored stays ignored: it goes to
//! `migrated_from_v0.unmapped` with its JSON Pointer and a warning, never onto a
//! v1 field. A transition with no numeric bound never fires in v0 and is removed
//! (kept under `unmapped`). What v0 refused is refused again, with typed messages.

use serde_json::{Map, Value};

use crate::capture::plan::{
    CapturePlan, CapturePlanDevice, ConditionBounds, MigratedFromV0, PLAN_SCHEMA_VERSION, Phase,
    Transition, TransitionConditions, UnmappedValue,
};

/// A migrated v0 plan. `errors` are what v0 validation refused (the plan does not
/// load); the plan is still built, with the v0 compile's fallbacks, so test code
/// can insert invalid plans as before.
pub(crate) struct Migration {
    pub plan: CapturePlan,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

struct Migrator<'a> {
    plan_id: &'a str,
    unmapped: Vec<UnmappedValue>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl Migrator<'_> {
    fn error(&mut self, pointer: &str, message: &str) {
        self.errors
            .push(format!("[{}] {pointer}: {message}", self.plan_id));
    }

    fn unmap(&mut self, pointer: String, value: &Value, note: &str) {
        self.warnings
            .push(format!("[{}] {pointer}: {note}", self.plan_id));
        self.unmapped.push(UnmappedValue {
            pointer,
            value: value.clone(),
        });
    }

    /// Unmaps a key v0 read nothing from, with a hint at the v1 field to set.
    fn ignore(&mut self, pointer: String, value: &Value) {
        let key = pointer.rsplit('/').next().unwrap_or_default();
        let hint = match (pointer.as_str(), key) {
            (_, "units") => "; v1 values are always metric (m, m/s, N)",
            ("/device/segment_mass", _) => "; set device.segment_mass_kg instead",
            ("/device/segments", _) => "; set device.segment_spacing_m instead",
            ("/device/damping", _) => {
                "; set device.joint_damping_linear and joint_damping_angular instead"
            }
            ("/start_epoch" | "/chaser" | "/target", _) => "; set it in the project manifest",
            _ => "",
        };
        let note = format!("not used by v0 or v1; kept under migrated_from_v0.unmapped{hint}");
        self.unmap(pointer, value, &note);
    }

    /// The numeric bounds of a v0 condition object; `None` when it has none.
    /// Every other key (and a non-numeric bound) is unmapped.
    fn bounds(&mut self, pointer: &str, value: &Value) -> Option<ConditionBounds> {
        let object = value.as_object()?;
        let bounds = ConditionBounds {
            less_than: object.get("less_than").and_then(Value::as_f64),
            greater_than: object.get("greater_than").and_then(Value::as_f64),
        };
        if bounds.less_than.is_none() && bounds.greater_than.is_none() {
            return None;
        }
        for (key, v) in object {
            if !matches!(key.as_str(), "less_than" | "greater_than") || !v.is_number() {
                self.ignore(format!("{pointer}/{}", token(key)), v);
            }
        }
        Some(bounds)
    }

    fn device(&mut self, device: &Map<String, Value>, hard: &mut Vec<String>) -> CapturePlanDevice {
        let mut out = CapturePlanDevice::default();
        for (key, value) in device {
            match key.as_str() {
                "type" => match value.as_str() {
                    Some(t) => out.device_type = Some(t.to_string()),
                    None => hard.push(self.typed("/device/type", "must be a string")),
                },
                "tether_length" => match value.as_f64() {
                    Some(l) => out.tether_length_m = Some(l),
                    None => hard.push(self.typed("/device/tether_length", "must be a number")),
                },
                _ => self.ignore(format!("/device/{}", token(key)), value),
            }
        }
        if out.tether_length_m.is_none() && !device.contains_key("tether_length") {
            // v0 read a missing length as 0, which its validation refused.
            self.error(
                "/device/tether_length",
                "missing (v0 requires it in a device block)",
            );
        }
        out
    }

    fn typed(&self, pointer: &str, message: &str) -> String {
        format!("[{}] {pointer}: {message}", self.plan_id)
    }

    fn phase(&mut self, i: usize, state: &Map<String, Value>, hard: &mut Vec<String>) -> Phase {
        let at = |key: &str| format!("/states/{i}/{key}");
        let id = match state.get("id") {
            Some(Value::String(id)) => id.clone(),
            Some(_) => {
                hard.push(self.typed(&at("id"), "must be a string"));
                String::new()
            }
            None => {
                hard.push(self.typed(&at("id"), "missing"));
                String::new()
            }
        };
        let mut phase = Phase {
            id,
            max_velocity_mps: 0.0,
            max_force_n: 0.0,
            shrink_rate_mps: None,
            transitions: None,
            end: None,
        };
        let parameters = state.get("parameters").and_then(Value::as_object);
        for (key, slot) in [
            ("max_velocity", &mut phase.max_velocity_mps),
            ("max_force", &mut phase.max_force_n),
        ] {
            match parameters.and_then(|p| p.get(key)) {
                Some(value) => match value.as_f64() {
                    Some(v) => *slot = v,
                    None => self.error(&at(&format!("parameters/{key}")), "must be a number"),
                },
                None => self.error(&at(&format!("parameters/{key}")), "missing"),
            }
        }
        for (key, value) in parameters.into_iter().flatten() {
            match (key.as_str(), value.as_f64()) {
                ("max_velocity" | "max_force", _) => {}
                ("shrink_rate", Some(rate)) => phase.shrink_rate_mps = Some(rate),
                _ => self.ignore(at(&format!("parameters/{}", token(key))), value),
            }
        }

        for (key, value) in state {
            match key.as_str() {
                "id" | "parameters" => {}
                "next" if !value.is_string() && !value.is_null() => {
                    hard.push(self.typed(&at("next"), "must be a string"));
                }
                "transitions" => match value {
                    Value::Array(transitions) => {
                        phase.transitions = Some(self.transitions(i, transitions));
                    }
                    Value::Null => self.ignore(at("transitions"), value),
                    _ => hard.push(self.typed(&at(key), "must be an array")),
                },
                _ => self.ignore(at(&token(key)), value),
            }
        }
        phase
    }

    fn transitions(&mut self, i: usize, transitions: &[Value]) -> Vec<Transition> {
        let mut out = Vec::new();
        for (j, transition) in transitions.iter().enumerate() {
            let at = format!("/states/{i}/transitions/{j}");
            let Some(to) = transition.get("to").and_then(Value::as_str) else {
                self.error(&format!("{at}/to"), "missing");
                self.unmap(at, transition, "transition without 'to'; removed");
                continue;
            };
            let distance = transition
                .get("distance")
                .and_then(|d| self.bounds(&format!("{at}/distance"), d));
            let speed = transition
                .get("relative_velocity")
                .and_then(|v| self.bounds(&format!("{at}/relative_velocity"), v));
            if distance.is_none() && speed.is_none() {
                self.unmap(
                    at,
                    transition,
                    "never fires in v0 (no numeric distance or relative_velocity bound); \
                     removed from the transitions and kept under migrated_from_v0.unmapped",
                );
                continue;
            }
            for (key, value) in transition.as_object().into_iter().flatten() {
                let mapped = match key.as_str() {
                    "to" => true,
                    "distance" => distance.is_some(),
                    "relative_velocity" => speed.is_some(),
                    _ => false,
                };
                if !mapped {
                    self.ignore(format!("{at}/{}", token(key)), value);
                }
            }
            out.push(Transition {
                to: to.to_string(),
                all_of: TransitionConditions {
                    distance_m: distance,
                    relative_speed_mps: speed,
                },
            });
        }
        out
    }
}

/// `key` as a JSON Pointer reference token (RFC 6901).
fn token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Migrates the v0 document `doc` of plan `plan_id`. `Err` holds what v0 could
/// not even parse (a missing or wrongly typed required field).
pub(crate) fn migrate(plan_id: &str, doc: &Value) -> Result<Migration, Vec<String>> {
    let mut m = Migrator {
        plan_id,
        unmapped: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    let Some(root) = doc.as_object() else {
        return Err(vec![m.typed("/", "a capture plan must be a JSON object")]);
    };
    let mut hard = Vec::new();
    let mut string = |m: &Migrator, key: &str| match root.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(_) => {
            hard.push(m.typed(&format!("/{key}"), "must be a string"));
            String::new()
        }
        None => {
            hard.push(m.typed(&format!("/{key}"), "missing"));
            String::new()
        }
    };
    let name = string(&m, "name");
    let tether = string(&m, "tether");

    let mut device = None;
    let mut phases = Vec::new();
    for (key, value) in root {
        match (key.as_str(), value) {
            ("name" | "tether", _) => {}
            ("device", Value::Object(d)) => device = Some(m.device(d, &mut hard)),
            ("device", Value::Null) => m.ignore("/device".to_string(), value),
            ("device", _) => hard.push(m.typed("/device", "must be an object")),
            ("states", Value::Array(states)) => {
                for (i, state) in states.iter().enumerate() {
                    match state.as_object() {
                        Some(state) => phases.push(m.phase(i, state, &mut hard)),
                        None => hard.push(m.typed(&format!("/states/{i}"), "must be an object")),
                    }
                }
            }
            ("states", _) => hard.push(m.typed("/states", "must be an array")),
            _ => m.ignore(format!("/{}", token(key)), value),
        }
    }
    if !root.contains_key("states") {
        hard.push(m.typed("/states", "missing"));
    }
    if !hard.is_empty() {
        return Err(hard);
    }

    let plan = CapturePlan {
        schema_version: PLAN_SCHEMA_VERSION,
        name,
        id: String::new(),
        tether,
        device,
        capture_sphere: None,
        phases,
        migrated_from_v0: (!m.unmapped.is_empty()).then(|| MigratedFromV0 {
            unmapped: std::mem::take(&mut m.unmapped),
        }),
    };
    Ok(Migration {
        plan,
        errors: m.errors,
        warnings: m.warnings,
    })
}
