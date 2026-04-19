use std::f64::consts::PI;
use std::fs;

use crate::components::orbit::{Earth, JsonOrbitalData, Orbit, Orbital, TetherNode, TetherRoot};
use crate::components::orbit_camera::CameraTarget;
use crate::constants::{
    MAP_LAYER, MAX_ORIGIN_OFFSET, PHYSICS_DISABLE_RADIUS, PHYSICS_ENABLE_RADIUS, eci_to_orbit_frame,
};
use crate::plugins::gpu_compute::{GpuComputeEpochOrigin, GpuElements, GpuOrbitalElements};
use crate::resources::capture_log::{LogEvent, LogLevel};
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::space_catalog::{SpaceCatalogEntry, SpaceObjectCatalog};
use crate::resources::world_time::WorldTime;

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQuery};
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::pbr::Atmosphere;
use bevy::prelude::*;
use brahe::utils::DOrbitStateProvider;
use brahe::{Epoch, GM_EARTH, KeplerianPropagator, TimeSystem};
use nalgebra::{DVector, Vector6};

pub fn calculate_com_rv(
    entities: Query<Entity, With<TetherRoot>>,
    rigidbodies: Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    nodes: Query<(Entity, &TetherNode)>,
    mut orbital_cache: ResMut<OrbitalCache>,
) {
    for entity in entities {
        let Ok(target_rb) = rigidbodies.get(entity) else {
            continue;
        };

        let mut weighted_pos = (target_rb.position.0
            + target_rb.rotation.0 * target_rb.center_of_mass.0)
            * target_rb.mass.value();
        let mut weighted_linvel = target_rb.linear_velocity.0 * target_rb.mass.value();
        let mut total_mass = target_rb.mass.value();

        for (node_entity, node) in nodes.iter() {
            if node.root != entity {
                continue;
            }

            let Ok(node_rb) = rigidbodies.get(node_entity) else {
                continue;
            };

            weighted_pos += (node_rb.position.0 + node_rb.rotation.0 * node_rb.center_of_mass.0)
                * node_rb.mass.value();
            weighted_linvel += node_rb.linear_velocity.0 * node_rb.mass.value();
            total_mass += node_rb.mass.value();
        }

        if total_mass <= 0.0 {
            continue;
        }

        orbital_cache.com_rv.insert(
            entity,
            (weighted_pos / total_mass, weighted_linvel / total_mass),
        );
    }
}

pub fn load_dataset_entities(
    mut space_catalog: ResMut<SpaceObjectCatalog>,
    gpu_elements: Option<ResMut<GpuElements>>,
    world_time: Res<WorldTime>,
    gpu_epoch_origin: Option<ResMut<GpuComputeEpochOrigin>>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let plans_dir = crate::resolve_assets_dir().join("datasets");
    let mut gpu_elements = gpu_elements;
    let mut gpu_epoch_origin = gpu_epoch_origin;
    let reference_epoch = if let Some(origin) = gpu_epoch_origin.as_deref_mut() {
        let epoch = origin.0.get_or_insert(world_time.epoch);
        *epoch
    } else {
        world_time.epoch
    };

    space_catalog.entries.clear();

    if let Some(gpu_elements) = gpu_elements.as_deref_mut() {
        gpu_elements.0.clear();
    }

    let dir_iter = match fs::read_dir(&plans_dir) {
        Ok(iter) => iter,
        Err(e) => {
            warn!("Could not read dataset directory {plans_dir:?}: {e}");
            return;
        }
    };
    for dataset_file_result in dir_iter {
        if let Ok(dataset_file) = dataset_file_result {
            let path = dataset_file.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                if let Ok(raw_json) = fs::read_to_string(&path) {
                    if let Ok(dataset) = serde_json::from_str(&raw_json) {
                        let data: JsonOrbitalData = dataset;
                        for object in data.data {
                            let Some(id_val) = object.norad_cat_id else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "dataset",
                                    message: "Failed to parse norad_cat_id for an object"
                                        .to_string(),
                                });
                                continue;
                            };
                            let Some(id) = id_val.as_u64() else {
                                continue;
                            };
                            let Some(mean_motion_val) = object.mean_motion else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "dataset",
                                    message: format!(
                                        "Failed to parse mean_motion for object {:?}",
                                        id_val
                                    ),
                                });
                                continue;
                            };
                            let Some(mean_motion) = mean_motion_val.as_f64() else {
                                continue;
                            };
                            let mean_motion_rad_s = mean_motion * 2.0 * PI / 86400.0;
                            let semi_major_axis = (GM_EARTH
                                / (mean_motion_rad_s * mean_motion_rad_s))
                                .powf(1.0 / 3.0);
                            let Some(eccentricity_val) = object.eccentricity else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "propagation",
                                    message: format!(
                                        "Failed to parse eccentricity for object {id}"
                                    ),
                                });
                                continue;
                            };
                            let Some(eccentricity) = eccentricity_val.as_f64() else {
                                continue;
                            };
                            let Some(inclination_val) = object.inclination else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "propagation",
                                    message: format!("Failed to parse inclination for object {id}"),
                                });
                                continue;
                            };
                            let Some(inclination) = inclination_val.as_f64() else {
                                continue;
                            };
                            let inclination = inclination.to_radians();
                            let Some(raan_val) = object.ra_of_asc_node else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "propagation",
                                    message: format!(
                                        "Failed to parse ra_of_asc_node for object {id}"
                                    ),
                                });
                                continue;
                            };
                            let Some(raan) = raan_val.as_f64() else {
                                continue;
                            };
                            let raan = raan.to_radians();
                            let Some(argp_val) = object.arg_of_pericenter else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "propagation",
                                    message: format!(
                                        "Failed to parse arg_of_pericenter for object {id}"
                                    ),
                                });
                                continue;
                            };
                            let Some(argp) = argp_val.as_f64() else {
                                continue;
                            };
                            let argp = argp.to_radians();
                            let Some(mean_anomaly_val) = object.mean_anomaly else {
                                log_events.write(LogEvent {
                                    level: LogLevel::Error,
                                    source: "propagation",
                                    message: format!(
                                        "Failed to parse mean_anomaly for object {id}"
                                    ),
                                });
                                continue;
                            };
                            let Some(mean_anomaly) = mean_anomaly_val.as_f64() else {
                                continue;
                            };
                            let mean_anomaly = mean_anomaly.to_radians();
                            let object_epoch = object
                                .epoch
                                .as_deref()
                                .and_then(parse_dataset_epoch_fast)
                                .unwrap_or(reference_epoch);
                            let object_name = object
                                .object_name
                                .clone()
                                .filter(|name| !name.is_empty())
                                .or_else(|| object.object_id.clone().filter(|id| !id.is_empty()))
                                .unwrap_or_else(|| format!("NORAD {}", id));
                            let object_id = object
                                .object_id
                                .clone()
                                .filter(|id| !id.is_empty())
                                .unwrap_or_default();
                            let gpu_index = space_catalog.entries.len();

                            let elements = Vector6::new(
                                semi_major_axis,
                                eccentricity,
                                inclination,
                                raan,
                                argp,
                                mean_anomaly,
                            );

                            space_catalog.entries.push(SpaceCatalogEntry {
                                gpu_index,
                                norad_id: id as u32,
                                search_blob: format!(
                                    "{} {} {}",
                                    object_name.to_lowercase(),
                                    object_id.to_lowercase(),
                                    id
                                ),
                                object_name,
                                object_id,
                            });

                            if let Some(gpu_elements) = gpu_elements.as_deref_mut() {
                                gpu_elements.0.push(GpuOrbitalElements {
                                    id: id as u32,
                                    a: elements[0] as f32,
                                    e: elements[1] as f32,
                                    i: elements[2] as f32,
                                    raan: elements[3] as f32,
                                    argp: elements[4] as f32,
                                    mean_anomaly: elements[5] as f32,
                                    epoch_offset_seconds: (object_epoch - reference_epoch) as f32,
                                });
                            }
                        }
                    } else {
                        log_events.write(LogEvent {
                            level: LogLevel::Error,
                            source: "dataset",
                            message: format!("Failed to parse dataset JSON: {}", path.display()),
                        });
                    }
                }
            }
        }
    }

    let entry_count = space_catalog.entries.len();
    space_catalog
        .entries
        .sort_by(|left, right| left.display_name().cmp(right.display_name()));
    if entry_count > 0 {
        log_events.write(LogEvent {
            level: LogLevel::Info,
            source: "propagation",
            message: format!("Dataset loaded: {entry_count} objects"),
        });
    }
}

fn parse_dataset_epoch_fast(raw: &str) -> Option<Epoch> {
    let bytes = raw.as_bytes();
    let is_fixed_format = bytes.len() >= 19
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':');

    if !is_fixed_format {
        return Epoch::from_string(raw).or_else(|| Epoch::from_string(&format!("{raw}Z")));
    }

    let year = raw.get(0..4)?.parse().ok()?;
    let month = raw.get(5..7)?.parse().ok()?;
    let day = raw.get(8..10)?.parse().ok()?;
    let hour = raw.get(11..13)?.parse().ok()?;
    let minute = raw.get(14..16)?.parse().ok()?;
    let whole_seconds: f64 = raw.get(17..19)?.parse().ok()?;

    let fractional_seconds = raw
        .get(19..)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .and_then(|digits| {
            if digits.is_empty() {
                return None;
            }

            let value = digits.parse::<u32>().ok()? as f64;
            let scale = 10_f64.powi(digits.len() as i32);
            Some(value / scale)
        })
        .unwrap_or(0.0);

    Some(Epoch::from_datetime(
        year,
        month,
        day,
        hour,
        minute,
        whole_seconds + fractional_seconds,
        0.0,
        TimeSystem::UTC,
    ))
}

pub fn init_orbitals(
    mut commands: Commands,
    mut q: Query<(Entity, &Orbit, &mut Orbital), Added<Orbit>>,
) {
    for (entity, init, mut orbital) in &mut q {
        match init {
            Orbit::FromElements(elements) => {
                let epoch = Epoch::now();
                let propagator = KeplerianPropagator::from_keplerian(
                    epoch,
                    *elements,
                    brahe::AngleFormat::Radians,
                    1.0,
                );
                if let Ok(eci) = propagator.state_eci(epoch) {
                    orbital.propagator = Some(KeplerianPropagator::from_eci(epoch, eci, 1.0));
                    // println!("ECI Initialized to: {}", eci);
                    eci
                } else {
                    Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                }
            }
            Orbit::FromTle(tle) => {
                // TODO: init logic from TLE data (sgp4)
                Vector6::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
            }
        };

        commands.entity(entity).remove::<Orbit>();
    }
}

pub fn cache_eci_states(
    orbitals: Query<(Entity, &Orbital)>,
    mut orbital_cache: ResMut<OrbitalCache>,
    world_time: Res<WorldTime>,
) {
    let epoch = world_time.epoch;
    let states = std::sync::Mutex::new(bevy::platform::collections::HashMap::default());

    orbitals.par_iter().for_each(|(entity, orbital)| {
        if let Some(state) = orbital
            .propagator
            .as_ref()
            .and_then(|prop| prop.state_eci(epoch).ok())
        {
            states.lock().unwrap().insert(entity, state);
        }
    });

    orbital_cache.eci_states = states.into_inner().unwrap();
}

pub fn floating_origin_update_visuals(
    target_params_s: Single<
        (Entity, &Transform),
        (
            Without<RigidBodyDisabled>,
            With<CameraTarget>,
            Without<Earth>,
        ),
    >,
    camera_s: Single<(&mut Atmosphere, &RenderLayers), (With<Camera3d>, Without<Orbital>)>,
    earth: Single<&mut Transform, (With<Earth>, Without<CameraTarget>)>,
    orbital_cache: ResMut<OrbitalCache>,
) {
    let (mut atmosphere, render_layers) = camera_s.into_inner();

    // Do not calculate floating origin if we are in map view
    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        return;
    }

    // We want to position orbital objects relative to the camera's current target
    let (target_entity, target_transform) = target_params_s.into_inner();

    // Get current cartesian state of our target
    let Some(target_rv) = orbital_cache.eci_states.get(&target_entity) else {
        return;
    };

    // Earth translation becomes new position
    let mut earth_transform = earth.into_inner();
    let target_position = eci_to_orbit_frame(Vec3::new(
        target_rv[0] as f32,
        target_rv[1] as f32,
        target_rv[2] as f32,
    ));
    let new_translation = target_transform.translation - target_position;
    earth_transform.translation = new_translation;
    atmosphere.world_position = new_translation;
}

pub fn target_entity_reset_origin(
    mut true_params_query: Query<&mut Orbital, Without<RigidBodyDisabled>>,
    mut rigidbodies: Query<RigidBodyQuery, Without<RigidBodyDisabled>>,
    target_entity_q: Query<Entity, (With<CameraTarget>, Without<RigidBodyDisabled>)>,
    world_time: Res<WorldTime>,
    orbital_cache: Res<OrbitalCache>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let Ok(target_entity) = target_entity_q.single() else {
        return;
    };

    let Some((com_r, com_v)) = orbital_cache.com_rv.get(&target_entity) else {
        return;
    };

    if com_r.length() <= MAX_ORIGIN_OFFSET {
        return;
    }

    log_events.write(LogEvent {
        level: LogLevel::Debug,
        source: "propagation",
        message: format!("Floating origin reset (epoch: {})", world_time.epoch),
    });

    // Accumulate current linvel and position into rigidbodies
    true_params_query.par_iter_mut().for_each(|mut orbital| {
        if let Some(prop) = orbital.propagator.as_mut() {
            let Ok(rv) = prop.state_eci(world_time.epoch) else {
                return;
            };

            let new_rv = rv
                + DVector::<f64>::from_vec(vec![
                    com_r.x, com_r.y, com_r.z, com_v.x, com_v.y, com_v.z,
                ]);

            // Rebuild propagator
            *prop = KeplerianPropagator::from_eci(world_time.epoch, new_rv, 1.0);
        }
    });

    // Reset rigidbodies
    rigidbodies.par_iter_mut().for_each(|mut rb| {
        rb.position.0 -= com_r;
        rb.linear_velocity.0 -= com_v;
    });
}

pub fn physics_bubble_add_remove(
    mut commands: Commands,
    disabled_entities: Query<(Entity, &RigidBodyDisabled)>,
    orbital_entities: Query<(Entity, &mut Orbital, RigidBodyQuery), Without<CameraTarget>>,
    target_entity: Single<Entity, With<CameraTarget>>,
    mut orbital_cache: ResMut<OrbitalCache>,
    world_time: Res<WorldTime>,
    mut log_events: MessageWriter<LogEvent>,
) {
    let entity = target_entity.into_inner();

    // Get current cartesian state of our target
    let Some(mut target_rv) = orbital_cache.eci_states.get_mut(&entity).cloned() else {
        return;
    };

    // Loop through entities to see if any should be disabled/enabled
    for (entity, mut orbital, mut rb) in orbital_entities {
        let Some(entity_rv) = orbital_cache.eci_states.get_mut(&entity) else {
            return;
        };

        let mut enabled = false;

        if !disabled_entities.contains(entity) && rb.position.0.length() > PHYSICS_DISABLE_RADIUS {
            target_rv[0] += rb.position.x;
            target_rv[1] += rb.position.y;
            target_rv[2] += rb.position.z;
            target_rv[3] += rb.linear_velocity.x;
            target_rv[4] += rb.linear_velocity.y;
            target_rv[5] += rb.linear_velocity.z;

            orbital.propagator = Some(KeplerianPropagator::from_eci(
                world_time.epoch,
                target_rv.clone(),
                1.0,
            ));

            commands.entity(entity).insert(RigidBodyDisabled);
            log_events.write(LogEvent {
                level: LogLevel::Debug,
                source: "propagation",
                message: format!(
                    "Physics disabled for entity {:?} (dist {:.0} km from target)",
                    entity,
                    rb.position.0.length() / 1000.0,
                ),
            });
        } else if disabled_entities.contains(entity)
            && rb.position.0.length() < PHYSICS_ENABLE_RADIUS
        {
            // println!("rel: {}", relative_pos);
            entity_rv[0] -= rb.position.x;
            entity_rv[1] -= rb.position.y;
            entity_rv[2] -= rb.position.z;
            entity_rv[3] -= rb.linear_velocity.x;
            entity_rv[4] -= rb.linear_velocity.y;
            entity_rv[5] -= rb.linear_velocity.z;

            // *prop = KeplerianPropagator::from_eci(world_time.epoch, entity_rv, 1.0);

            commands.entity(entity).remove::<RigidBodyDisabled>();

            enabled = true;

            log_events.write(LogEvent {
                level: LogLevel::Debug,
                source: "propagation",
                message: format!(
                    "Physics enabled for entity {:?} (dist {:.0} km from target)",
                    entity,
                    rb.position.0.length() / 1000.0,
                ),
            });
        }

        // Set disabled bodies rigidbody values to their global relative state (for capture algorithm)
        if !enabled && disabled_entities.contains(entity) {
            rb.position.0 = DVec3::new(
                entity_rv[0] - target_rv[0],
                entity_rv[1] - target_rv[1],
                entity_rv[2] - target_rv[2],
            );
            rb.linear_velocity.0 = DVec3::new(
                entity_rv[3] - target_rv[3],
                entity_rv[4] - target_rv[4],
                entity_rv[5] - target_rv[5],
            );
        }
    }
}
