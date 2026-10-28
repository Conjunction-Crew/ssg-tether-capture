use super::*;
use crate::orbit::ISS_ORBIT;
use crate::test_support::{fixed_epoch, kepler_eci};

fn iss_frame() -> LvlhFrame {
    LvlhFrame::from_eci(&kepler_eci(ISS_ORBIT, fixed_epoch())).expect("ISS frame")
}

#[test]
fn lvlh_triad_is_orthonormal_right_handed() {
    let f = iss_frame();
    for (name, axis) in [("radial", f.radial), ("along", f.along), ("cross", f.cross)] {
        assert!(
            (axis.length() - 1.0).abs() < 1e-12,
            "|{name}| = {}",
            axis.length()
        );
    }
    for (name, dot) in [
        ("radial·along", f.radial.dot(f.along)),
        ("radial·cross", f.radial.dot(f.cross)),
        ("along·cross", f.along.dot(f.cross)),
    ] {
        assert!(dot.abs() < 1e-12, "{name} = {dot:e}");
    }
    assert!(
        (f.radial.cross(f.along) - f.cross).length() < 1e-12,
        "radial × along = {:?}, cross = {:?}",
        f.radial.cross(f.along),
        f.cross
    );
    // The round trip through LVLH components.
    let v = DVec3::new(1.0, -2.0, 3.0);
    assert!((f.to_eci(f.to_lvlh(v)) - v).length() < 1e-12);
}

#[test]
fn lvlh_rest_velocity_is_zero_for_corotating_body() {
    // F-35: fixed in P4. A body 3,536 m along-track at rest in LVLH.
    let f = iss_frame();
    let rel_r = f.along * 3_536.0;
    let rel_v = f.omega.cross(rel_r);
    // The plain ECI difference: the ≈ 4 m/s artefact.
    assert!(
        rel_v.length() > 3.5,
        "ECI relative speed {}",
        rel_v.length()
    );
    let in_frame = f.relative_velocity_in_frame(rel_r, rel_v);
    assert!(
        in_frame.length() < 1e-12,
        "LVLH relative velocity {in_frame:?}"
    );
}

#[test]
fn relative_gravity_matches_cw_tensor_at_100m() {
    let f = iss_frame();
    for (p, tol) in [(100.0, 1e-8), (3_500.0, 2e-5)] {
        for (name, axis) in [("radial", f.radial), ("along", f.along), ("cross", f.cross)] {
            let offset = axis * p;
            let exact = differential_gravity(f.origin_r, offset);
            let linear = cw_gravity_gradient(f.origin_r, offset);
            let diff = (exact - linear).length();
            assert!(
                diff < tol,
                "{name} {p} m: |differential − gradient| = {diff:e} m/s² (tol {tol:e})"
            );
        }
    }
    assert_eq!(differential_gravity(f.origin_r, DVec3::ZERO), DVec3::ZERO);
}

/// A free particle in the translating, ECI-axis bubble frame under
/// [`differential_gravity`] about a circular orbit (a = 6.8e6 m), RK4 at 0.5 s
/// for one period, starting at rest in LVLH. Returns the largest distance from
/// the CW solution, sampled every 200 s and at the end (the planner's probe,
/// `plans/stage-4/results/cw_check.py`).
fn cw_max_error(x0: DVec3) -> f64 {
    const A: f64 = 6.8e6;
    const DT: f64 = 0.5;
    let n = (GM_EARTH / A.powi(3)).sqrt();
    let reference = |t: f64| {
        let (s, c) = (n * t).sin_cos();
        Vector6::new(A * c, A * s, 0.0, -A * n * s, A * n * c, 0.0)
    };
    let origin = |t: f64| {
        let r = reference(t);
        DVec3::new(r[0], r[1], r[2])
    };
    let frame0 = LvlhFrame::from_eci(&reference(0.0)).unwrap();
    let mut p = frame0.to_eci(x0);
    let mut v = frame0.omega.cross(p);

    let steps = (2.0 * std::f64::consts::PI / n / DT) as usize;
    let mut t = 0.0;
    let mut worst: f64 = 0.0;
    for k in 0..steps {
        let a = |t: f64, p: DVec3| differential_gravity(origin(t), p);
        let (k1p, k1v) = (v, a(t, p));
        let (k2p, k2v) = (v + 0.5 * DT * k1v, a(t + 0.5 * DT, p + 0.5 * DT * k1p));
        let (k3p, k3v) = (v + 0.5 * DT * k2v, a(t + 0.5 * DT, p + 0.5 * DT * k2p));
        let (k4p, k4v) = (v + DT * k3v, a(t + DT, p + DT * k3p));
        p += DT / 6.0 * (k1p + 2.0 * k2p + 2.0 * k3p + k4p);
        v += DT / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
        t += DT;
        if k % 400 == 0 || k == steps - 1 {
            let frame = LvlhFrame::from_eci(&reference(t)).unwrap();
            let (cw, _) = cw_solution(n, x0, DVec3::ZERO, t);
            worst = worst.max((frame.to_lvlh(p) - cw).length());
        }
    }
    worst
}

#[test]
fn free_particle_follows_cw_solution_one_orbit() {
    let along = cw_max_error(DVec3::new(0.0, 100.0, 0.0));
    let cross = cw_max_error(DVec3::new(0.0, 0.0, 100.0));
    let radial = cw_max_error(DVec3::new(100.0, 0.0, 0.0));
    println!(
        "free particle vs CW, max |error| over one orbit: \
         radial {radial:.4} m, along-track {along:.4} m, cross-track {cross:.4} m"
    );
    assert!(along < 0.1, "along-track 100 m: {along} m");
    assert!(cross < 0.1, "cross-track 100 m: {cross} m");
    // CW's own linearisation error for a 100 m radial offset is ≈ 1.1 m.
    assert!(radial < 1.5, "radial 100 m: {radial} m");
}
