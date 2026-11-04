//! Validating a manifest against design D2 and converting it to and from the
//! sim inputs.

use super::*;

impl ProjectManifest {
    /// Every rule of design D2 that serde cannot check, in field order, as
    /// `[project.json] <json-pointer>: <message>`.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            errors.push(message(
                "/schema_version",
                format!("must be {MANIFEST_SCHEMA_VERSION}"),
            ));
        }
        if self.name.trim().is_empty() {
            errors.push(message("/name", "must not be blank"));
        }
        let start = parse_utc(&self.start_epoch_utc);
        if start.is_none() {
            errors.push(message(
                "/start_epoch_utc",
                format!(
                    "`{}` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
                    self.start_epoch_utc
                ),
            ));
        }
        if self
            .active_plan
            .as_deref()
            .is_some_and(|id| id.trim().is_empty())
        {
            errors.push(message("/active_plan", "must not be blank"));
        }
        self.target.validate("/target", Role::Target, &mut errors);
        self.chaser.validate("/chaser", Role::Chaser, &mut errors);
        if let Some(recording) = &self.recording {
            recording.validate("/recording", &mut errors);
        }
        errors
    }

    /// The sim inputs of this manifest. `Err` carries every message of
    /// [`Self::validate`] (and nothing else can fail after it passes).
    pub fn sim_inputs(&self) -> Result<ManifestSimInputs, Vec<String>> {
        let errors = self.validate();
        if !errors.is_empty() {
            return Err(errors);
        }
        let start_epoch = parse_utc(&self.start_epoch_utc)
            .ok_or_else(|| vec![message("/start_epoch_utc", "is not a UTC epoch")])?;
        Ok(ManifestSimInputs {
            start_epoch,
            target: self.target.selected_object("/target", start_epoch)?,
            chaser: self.chaser.selected_object("/chaser", start_epoch)?,
            active_plan: self.active_plan.clone(),
        })
    }

    /// The manifest of a scenario about to run: the inverse of
    /// [`Self::sim_inputs`]. A body with a catalog record is written as that record
    /// (`OBJECT_NAME` is its label), any other as its elements. `start_epoch` is
    /// written in the file format. `Err` when the target or the chaser is not
    /// selected.
    pub fn from_selection(
        name: &str,
        start_epoch: Epoch,
        active_plan: Option<&str>,
        selection: &OrbitalSelectionState,
    ) -> Result<Self, String> {
        let body = |role: &str, selected: &Option<SelectedOrbitalObject>| {
            selected
                .as_ref()
                .map(ManifestBody::from_selected)
                .ok_or_else(|| format!("no {role} is selected"))
        };
        Ok(Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            name: name.to_string(),
            start_epoch_utc: format_utc(start_epoch),
            active_plan: active_plan.map(str::to_string),
            target: body("target", &selection.target)?,
            chaser: body("chaser", &selection.chaser)?,
            built_with: Some(BuiltWith::current()),
            recording: None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Target,
    Chaser,
}

impl ManifestBody {
    fn validate(&self, at: &str, role: Role, errors: &mut Vec<String>) {
        if self.label.trim().is_empty() {
            errors.push(message(&format!("{at}/label"), "must not be blank"));
        }
        self.validate_orbit(at, role, errors);
        if let Some(source) = &self.source
            && source.dataset.trim().is_empty()
        {
            errors.push(message(
                &format!("{at}/source/dataset"),
                "must not be blank",
            ));
        }
        // Q10 row 11: both roles' mass and inertia are applied (the chaser's to the
        // tether root).
        if let Some(mass) = self.mass_kg
            && !(mass.is_finite() && mass > 0.0)
        {
            errors.push(message(&format!("{at}/mass_kg"), "must be greater than 0"));
        }
        if let Some(tensor) = &self.inertia_kg_m2
            && !is_inertia_tensor(tensor)
        {
            errors.push(message(
                &format!("{at}/inertia_kg_m2"),
                "must be a finite, symmetric, positive-definite 3×3 tensor",
            ));
        }
    }

    fn validate_orbit(&self, at: &str, role: Role, errors: &mut Vec<String>) {
        let at = format!("{at}/orbit");
        match &self.orbit {
            ManifestOrbit::Gp(record) => {
                let at = format!("{at}/gp");
                let is_number =
                    |value: &Option<Value>| value.as_ref().is_some_and(|v| v.as_f64().is_some());
                if !record
                    .norad_cat_id
                    .as_ref()
                    .is_some_and(|v| v.as_u64().is_some())
                {
                    errors.push(message(
                        &format!("{at}/NORAD_CAT_ID"),
                        "missing, or not a whole number",
                    ));
                }
                let numbers = [
                    &record.mean_motion,
                    &record.eccentricity,
                    &record.inclination,
                    &record.ra_of_asc_node,
                    &record.arg_of_pericenter,
                    &record.mean_anomaly,
                ];
                for (key, value) in GP_NUMBER_KEYS.iter().zip(numbers) {
                    if !is_number(value) {
                        errors.push(message(&format!("{at}/{key}"), "missing, or not a number"));
                    }
                }
            }
            ManifestOrbit::Elements(elements) => {
                let at = format!("{at}/elements");
                for (key, value) in [
                    ("semi_major_axis_m", elements.semi_major_axis_m),
                    ("eccentricity", elements.eccentricity),
                    ("inclination_rad", elements.inclination_rad),
                    ("raan_rad", elements.raan_rad),
                    ("arg_perigee_rad", elements.arg_perigee_rad),
                    ("mean_anomaly_rad", elements.mean_anomaly_rad),
                ] {
                    if !value.is_finite() {
                        errors.push(message(&format!("{at}/{key}"), "must be a finite number"));
                    }
                }
                if elements.semi_major_axis_m.is_finite() && elements.semi_major_axis_m <= 0.0 {
                    errors.push(message(
                        &format!("{at}/semi_major_axis_m"),
                        "must be greater than 0",
                    ));
                }
                if elements.eccentricity.is_finite() && !(0.0..1.0).contains(&elements.eccentricity)
                {
                    errors.push(message(
                        &format!("{at}/eccentricity"),
                        "must be at least 0 and below 1",
                    ));
                }
                if parse_utc(&elements.epoch_utc).is_none() {
                    errors.push(message(
                        &format!("{at}/epoch_utc"),
                        format!(
                            "`{}` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
                            elements.epoch_utc
                        ),
                    ));
                }
            }
            ManifestOrbit::RelativeToTarget(relative) => {
                let at = format!("{at}/relative_to_target");
                match role {
                    Role::Target => errors.push(message(
                        &at,
                        "only the chaser can be placed relative to the target",
                    )),
                    Role::Chaser => errors.push(message(
                        &at,
                        format!("an orbit relative to the target {NOT_YET}"),
                    )),
                }
                if !relative.offset_m.is_finite() {
                    errors.push(message(
                        &format!("{at}/offset_m"),
                        "must be a finite number",
                    ));
                }
            }
        }
    }

    /// The selected object of a valid body. `at` is only used in messages.
    fn selected_object(
        &self,
        at: &str,
        start_epoch: Epoch,
    ) -> Result<SelectedOrbitalObject, Vec<String>> {
        let source = OrbitalSelectionSource::Custom {
            label: self.label.clone(),
        };
        match &self.orbit {
            ManifestOrbit::Gp(record) => {
                let (_, gp, _) = gp_record_to_elements(record, start_epoch).ok_or_else(|| {
                    vec![message(
                        &format!("{at}/orbit/gp"),
                        "the record does not give an orbit",
                    )]
                })?;
                // A catalog pick, as the app makes it (F-27): SGP4 at the start epoch.
                Ok(SelectedOrbitalObject {
                    source,
                    elements: EditableOrbitalElements::from_gp(&gp),
                    gp: Some(gp),
                    mass_kg: self.mass_kg,
                    inertia_kg_m2: self.inertia_kg_m2,
                })
            }
            ManifestOrbit::Elements(e) => {
                let epoch = parse_utc(&e.epoch_utc).ok_or_else(|| {
                    vec![message(
                        &format!("{at}/orbit/elements/epoch_utc"),
                        "is not a UTC epoch",
                    )]
                })?;
                Ok(SelectedOrbitalObject {
                    source,
                    elements: EditableOrbitalElements {
                        semi_major_axis_m: e.semi_major_axis_m,
                        eccentricity: e.eccentricity,
                        inclination_rad: e.inclination_rad,
                        raan_rad: e.raan_rad,
                        arg_perigee_rad: e.arg_perigee_rad,
                        mean_anomaly_rad: e.mean_anomaly_rad,
                        epoch,
                    },
                    gp: None,
                    mass_kg: self.mass_kg,
                    inertia_kg_m2: self.inertia_kg_m2,
                })
            }
            ManifestOrbit::RelativeToTarget(_) => Err(vec![message(
                &format!("{at}/orbit/relative_to_target"),
                format!("an orbit relative to the target {NOT_YET}"),
            )]),
        }
    }

    fn from_selected(selected: &SelectedOrbitalObject) -> Self {
        let label = match &selected.source {
            OrbitalSelectionSource::Catalog { label, .. }
            | OrbitalSelectionSource::Custom { label } => label.clone(),
        };
        let orbit = match &selected.gp {
            Some(gp) => ManifestOrbit::Gp(Box::new(gp_to_record(&label, gp))),
            None => {
                let e = &selected.elements;
                ManifestOrbit::Elements(ManifestElements {
                    semi_major_axis_m: e.semi_major_axis_m,
                    eccentricity: e.eccentricity,
                    inclination_rad: e.inclination_rad,
                    raan_rad: e.raan_rad,
                    arg_perigee_rad: e.arg_perigee_rad,
                    mean_anomaly_rad: e.mean_anomaly_rad,
                    epoch_utc: format_utc(e.epoch),
                })
            }
        };
        Self {
            label,
            orbit,
            source: None,
            mass_kg: selected.mass_kg,
            inertia_kg_m2: selected.inertia_kg_m2,
        }
    }
}

/// A finite, symmetric (to 1e-9 of its largest entry) and positive-definite 3×3
/// tensor (Sylvester's criterion: the three leading principal minors are positive).
fn is_inertia_tensor(t: &[[f64; 3]; 3]) -> bool {
    if !t.iter().flatten().all(|v| v.is_finite()) {
        return false;
    }
    let scale = t.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
    let symmetric = (0..3).all(|i| (0..3).all(|j| (t[i][j] - t[j][i]).abs() <= 1e-9 * scale));
    let minor1 = t[0][0];
    let minor2 = t[0][0] * t[1][1] - t[0][1] * t[1][0];
    let det = t[0][0] * (t[1][1] * t[2][2] - t[1][2] * t[2][1])
        - t[0][1] * (t[1][0] * t[2][2] - t[1][2] * t[2][0])
        + t[0][2] * (t[1][0] * t[2][1] - t[1][1] * t[2][0]);
    symmetric && minor1 > 0.0 && minor2 > 0.0 && det > 0.0
}

/// The GP record a catalog pick came from, as far as the sim reads it.
fn gp_to_record(label: &str, gp: &GpElements) -> JsonOrbital {
    JsonOrbital {
        object_name: Some(label.to_string()),
        object_id: gp.object_id.clone(),
        epoch: (!gp.epoch_str.is_empty()).then(|| gp.epoch_str.clone()),
        mean_motion: Some(Value::from(gp.mean_motion_rev_per_day)),
        eccentricity: Some(Value::from(gp.eccentricity)),
        inclination: Some(Value::from(gp.inclination_deg)),
        ra_of_asc_node: Some(Value::from(gp.raan_deg)),
        arg_of_pericenter: Some(Value::from(gp.arg_perigee_deg)),
        mean_anomaly: Some(Value::from(gp.mean_anomaly_deg)),
        ephemeris_type: gp.ephemeris_type.map(Value::from),
        classification_type: gp.classification.map(|c| c.to_string()),
        norad_cat_id: Some(Value::from(gp.norad_id)),
        element_set_no: None,
        rev_at_epoch: None,
        bstar: gp.bstar.map(Value::from),
        mean_motion_dot: gp.mean_motion_dot.map(Value::from),
        mean_motion_ddot: gp.mean_motion_ddot.map(Value::from),
    }
}
