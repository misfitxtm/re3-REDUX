use super::*;
use crate::general::Rng;
use crate::entity::{status, Entity, EntityType, PoolKind};

fn body(mass: f32, turn_mass: f32) -> Physical {
    Physical { mass, turn_mass, ..Default::default() }
}

#[test]
fn impulses_change_speed_by_impulse_over_mass() {
    let mut p = body(100.0, 400.0);
    p.apply_move_force(Vec3::new(100.0, 0.0, 0.0));
    assert_eq!(p.move_speed, Vec3::new(1.0, 0.0, 0.0));
    // an upward push at x = 2 spins the body about y: r x j = (2,0,0) x (0,0,1) = (0,-2,0)
    p.apply_turn_force(&Matrix::IDENTITY, Vec3::new(0.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(p.turn_speed, Vec3::new(0.0, -2.0 / 400.0, 0.0));
    // friction impulses accumulate separately until folded in
    p.apply_friction_move_force(Vec3::new(0.0, 50.0, 0.0));
    assert_eq!(p.move_speed.y, 0.0);
    p.apply_friction();
    assert_eq!((p.move_speed.y, p.move_friction), (0.5, Vec3::ZERO));
}

#[test]
fn centre_of_mass_offsets_the_torque_arm() {
    let mut p = body(100.0, 100.0);
    p.centre_of_mass = Vec3::new(0.0, 0.0, 1.0);
    // a car turned a quarter turn about z: its local +z is still +z, local x points along world y
    let m = Matrix::rotate_z_only(std::f32::consts::FRAC_PI_2);
    p.apply_turn_force(&m, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0));
    assert!(magnitude_squared(p.turn_speed) < 1e-12, "force through the centre of mass makes no torque");
}

#[test]
fn point_velocity_includes_rotation() {
    let mut p = body(1.0, 1.0);
    p.move_speed = Vec3::new(1.0, 0.0, 0.0);
    p.turn_speed = Vec3::new(0.0, 0.0, 2.0);
    // w x r for r = (1,0,0) is (0,2,0)
    assert_eq!(p.speed_at(Vec3::new(1.0, 0.0, 0.0)), Vec3::new(1.0, 2.0, 0.0));
    assert_eq!(p.speed(), Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn effective_mass_grows_with_a_longer_lever_arm_only_for_turning() {
    let p = body(100.0, 50.0);
    // through the centre: just the mass
    assert_eq!(p.mass_at(Vec3::ZERO, Vec3::X), 100.0);
    // off-centre pushes meet less resistance: 1 / (|r x d|^2 / I + 1/m)
    let m = p.mass_at(Vec3::new(0.0, 2.0, 0.0), Vec3::X);
    assert!((m - 1.0 / (4.0 / 50.0 + 0.01)).abs() < 1e-4 && m < 100.0);
    assert!((p.mass_tweak(Vec3::new(0.0, 2.0, 0.0), Vec3::X, 2.0) - 2.0 * m).abs() < 1e-3);
}

#[test]
fn gravity_and_air_resistance() {
    let mut p = body(1.0, 1.0);
    for _ in 0..50 {
        p.apply_gravity(1.0);
    }
    assert!((p.move_speed.z - (-0.4)).abs() < 1e-5);
    p.flags.affected_by_gravity = false;
    p.apply_gravity(1.0);
    assert!((p.move_speed.z - (-0.4)).abs() < 1e-5);

    // strong drag coefficient scales speed by coef^step
    let mut q = body(1.0, 1.0);
    q.move_speed = Vec3::new(10.0, 0.0, 0.0);
    q.turn_speed = Vec3::new(0.0, 0.0, 1.0);
    q.air_resistance = 0.9;
    q.apply_air_resistance(2.0);
    assert!((q.move_speed.x - 10.0 * 0.81).abs() < 1e-4 && (q.turn_speed.z - 0.81).abs() < 1e-6);
    // weak coefficient: quadratic drag, and the spin just decays by 1%
    let mut r = body(1.0, 1.0);
    r.move_speed = Vec3::new(10.0, 0.0, 0.0);
    r.turn_speed = Vec3::new(0.0, 0.0, 1.0);
    r.air_resistance = 0.002;
    r.apply_air_resistance(1.0);
    assert!((r.move_speed.x - 10.0 / (0.002 * 0.5 * 100.0 + 1.0)).abs() < 1e-4);
    assert!((r.turn_speed.z - 0.99).abs() < 1e-6);
}

#[test]
fn motion_is_applied_to_the_matrix() {
    let mut p = body(1.0, 1.0);
    p.move_speed = Vec3::new(1.0, 2.0, 3.0);
    let mut m = Matrix::IDENTITY;
    p.apply_move_speed(&mut m, 2.0);
    assert_eq!(m.pos, Vec3::new(2.0, 4.0, 6.0));
    // a quarter-turn per unit time about z tilts x towards y (and denormalises slightly)
    p.turn_speed = Vec3::new(0.0, 0.0, 0.1);
    p.apply_turn_speed(&mut m, 1.0);
    assert!((m.right - Vec3::new(1.0, 0.1, 0.0)).length() < 1e-6);
    assert!((m.forward - Vec3::new(-0.1, 1.0, 0.0)).length() < 1e-6);
}

#[test]
fn springs_push_back_only_when_compressed() {
    let mut p = body(1000.0, 1000.0);
    let down = Vec3::new(0.0, 0.0, -1.0);
    p.apply_spring_collision(&Matrix::IDENTITY, 1.0, 2.0, down, Vec3::ZERO, 1.0, 1.0);
    assert_eq!(p.move_speed, Vec3::ZERO, "fully extended spring does nothing");
    p.apply_spring_collision(&Matrix::IDENTITY, 1.0, 2.0, down, Vec3::ZERO, 0.5, 1.0);
    // impulse = -g * m * step * k * compression * bias * 2 along the (downward) direction => upward
    assert!((p.move_speed.z - 0.008 * 2.0 * 0.5 * 2.0).abs() < 1e-6);
    // a long step is clamped to 3
    let mut q = body(1000.0, 1000.0);
    q.apply_spring_collision(&Matrix::IDENTITY, 10.0, 2.0, down, Vec3::ZERO, 0.5, 1.0);
    assert!((q.move_speed.z - 3.0 * 0.008 * 2.0 * 0.5 * 2.0).abs() < 1e-6);
}

#[test]
fn damping_opposes_the_spring_speed_and_skips_a_resting_body() {
    let mut p = body(1000.0, 1000.0);
    assert!(p.apply_spring_dampening(&Matrix::IDENTITY, 1.0, 0.5, Vec3::Z, Vec3::ZERO, Vec3::ZERO));
    assert_eq!(p.move_speed, Vec3::ZERO, "no point velocity along the spring: nothing to damp");
    p.move_speed = Vec3::new(0.0, 0.0, 0.2);
    p.apply_spring_dampening(&Matrix::IDENTITY, 1.0, 0.5, Vec3::Z, Vec3::ZERO, Vec3::new(0.0, 0.0, 0.2));
    assert!(p.move_speed.z < 0.2 && p.move_speed.z > 0.0, "the spring's own speed is reduced");
}

#[test]
fn objects_fall_asleep_after_resting_eleven_frames() {
    let mut rng = Rng::default();
    let mut e = Entity::new(EntityType::Object, &mut rng);
    e.status = status::PHYSICS;
    let mut p = body(10.0, 10.0);
    p.flags.affected_by_gravity = false;
    for frame in 1..=10 {
        assert!(process_control(&mut e, &mut p, 1.0), "frame {frame}");
        assert!(!e.flags.is_static);
    }
    assert!(!process_control(&mut e, &mut p, 1.0), "the 11th resting frame puts it to sleep");
    assert!(e.flags.is_static && p.static_frames == 10);
    // a fast object never sleeps and has gravity and drag applied
    let mut e = Entity::new(EntityType::Object, &mut rng);
    let mut p = body(10.0, 10.0);
    p.move_speed = Vec3::new(5.0, 0.0, 0.0);
    p.air_resistance = 0.9;
    assert!(process_control(&mut e, &mut p, 1.0));
    assert!(p.move_speed.x < 5.0 && p.move_speed.z < 0.0 && !e.flags.is_static);
    // per-frame flags are reset
    e.flags.has_hit_wall = true;
    e.flags.was_postponed = true;
    process_control(&mut e, &mut p, 1.0);
    assert!(!e.flags.has_hit_wall && !e.flags.was_postponed);
}

#[test]
fn simple_status_skips_the_rest() {
    let mut rng = Rng::default();
    let mut e = Entity::new(EntityType::Vehicle, &mut rng);
    e.status = status::SIMPLE;
    let mut p = body(1.0, 1.0);
    p.move_speed = Vec3::X;
    assert!(process_control(&mut e, &mut p, 1.0));
    assert_eq!(p.move_speed, Vec3::X, "no gravity or drag for simple-status vehicles");
}

#[test]
fn collision_records_are_unique_and_capped() {
    let mut p = body(1.0, 1.0);
    let r = |h| EntityRef { pool: PoolKind::Vehicle, handle: h };
    for h in 0..10 {
        p.add_collision_record(r(h), 1000 + h as u32);
    }
    assert_eq!(p.collision_records.len(), MAX_COLLISION_RECORDS);
    p.add_collision_record(r(0), 5000);
    assert_eq!(p.collision_records.len(), MAX_COLLISION_RECORDS);
    assert!(p.has_collided_with(true, r(3)) && !p.has_collided_with(true, r(9)) && !p.has_collided_with(false, r(3)));
    p.damage_entity = Some(r(2));
    p.remove_refs_to(r(2));
    assert!(!p.collision_records.contains(&r(2)) && p.damage_entity.is_none());
    assert_eq!(p.collision_records.len(), MAX_COLLISION_RECORDS - 1);
}

#[test]
fn leaving_a_safe_position_undoes_the_frames_motion() {
    let mut rng = Rng::default();
    let mut e = Entity::new(EntityType::Vehicle, &mut rng);
    let mut p = body(1.0, 1.0);
    p.move_speed = Vec3::new(1.0, 0.0, 0.0);
    p.apply_move_speed(&mut e.matrix, 2.0);
    assert_eq!(e.matrix.pos.x, 2.0);
    e.flags.is_in_safe_position = true;
    unset_is_in_safe_position(&mut e, &mut p, 2.0);
    assert_eq!((e.matrix.pos.x, p.move_speed.x, e.flags.is_in_safe_position), (0.0, 1.0, false));
}
