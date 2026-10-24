//! Moving physicals through the world (`CPhysical::ProcessShift`, `ProcessCollision` and the
//! sector-list helpers). GTA III behaviour.
//!
//! An entity being processed is lent out of its pool, so a second one can be borrowed next to it;
//! every path puts it back before returning.
// The per-wheel loops index several parallel arrays, as the original does.
#![allow(clippy::needless_range_loop)]
use super::{Ents, Sector, World};
use crate::collision::{self, ColPoint};
use crate::entity::collide::{self, CollisionEvent, Ctx};
use crate::entity::{status, Ent, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{magnitude, magnitude_squared};
use crate::surface;
use glam::Vec3;

/// What the physics asks the rest of the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum WorldEvent {
    Collision(CollisionEvent),
    /// `DMAudio.ReportCollision`.
    ReportCollision { a: EntityRef, b: EntityRef, surface_a: u8, surface_b: u8, impulse: f32, speed_diff: f32 },
    /// `CPed::KillPedWithCar`.
    KillPedWithCar { ped: EntityRef, vehicle: EntityRef, impulse: f32 },
    /// `CCarCtrl::SwitchVehicleToRealPhysics`.
    SwitchToRealPhysics(EntityRef),
    /// Something a car asks for outside its own update (sounds from collision handling).
    Car(crate::vehicle::env::CarEvent),
    /// `gFireManager.StartFire(target, culprit, 0.8, true)`.
    StartFire { target: EntityRef, culprit: Option<EntityRef> },
    /// `gFireManager.StartFire(pos, size, false)`.
    StartFireAt { pos: Vec3, size: f32 },
    /// `CVehicle::ExtinguishCarFire`.
    ExtinguishCarFire(EntityRef),
    /// An explosion hurts a ped: `InflictDamage(culprit, EXPLOSION, damage, torso, direction)`, then the fall.
    ExplosionHitPed { ped: EntityRef, culprit: Option<EntityRef>, damage: f32, force_dir: Vec3 },
    /// `CGlass::WindowRespondsToExplosion`.
    WindowExplosion { entity: EntityRef, pos: Vec3 },
    /// `CPopulation::RemovePed`.
    RemovePed(EntityRef),
}

fn is_ped(e: &Ent) -> bool {
    e.base.ty == EntityType::Ped
}

fn is_vehicle(e: &Ent) -> bool {
    e.base.ty == EntityType::Vehicle
}

fn is_object(e: &Ent) -> bool {
    e.base.ty == EntityType::Object
}

fn is_building(e: &Ent) -> bool {
    e.base.ty == EntityType::Building
}

fn phys(e: &mut Ent) -> &mut crate::entity::physical::Physical {
    e.phys.as_mut().expect("physical entity")
}

fn model_ids(ents: &Ents) -> (i16, i16, i16, i16) {
    let i = &ents.indices;
    (i.car_bumper(), i.files(), i.rc_bandit(), i.train())
}

/// `AddCollisionRecord_Treadable`: remember a road the entity touched.
fn add_treadable_record(this: &mut Ent, ent: &Ent, ref_ent: EntityRef) {
    if let Kind::Treadable(t) = &ent.kind
        && let Some(p) = this.phys.as_mut()
    {
        let any = |row: usize| t.node_indices[row][..4].iter().any(|&n| n >= 0);
        // [0] car paths, [1] ped paths
        if any(1) {
            p.treadable[1] = Some(ref_ent);
        }
        if any(0) {
            p.treadable[0] = Some(ref_ent);
        }
    }
}

/// `CPhysical::AddCollisionRecord(ent)` on `this`.
fn add_collision_record(this: &mut Ent, ent: &mut Ent, ref_ent: EntityRef, time_ms: u32) {
    add_treadable_record(this, ent, ref_ent);
    this.base.flags.has_collided = true;
    ent.base.flags.has_collided = true;
    if let (Kind::Vehicle(a), Kind::Vehicle(b)) = (&mut this.kind, &mut ent.kind) {
        if a.alarm_state == -1 {
            a.alarm_state = 15000;
        }
        if b.alarm_state == -1 {
            b.alarm_state = 15000;
        }
    }
    if this.base.flags.use_collision_records
        && let Some(p) = this.phys.as_mut()
    {
        p.add_collision_record(ref_ent, time_ms);
    }
}

/// `CAutomobile::ProcessEntityCollision`: the car's wheels are tested as lines too.
fn car_entity_collision(ents: &mut Ents, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, points: &mut [ColPoint]) -> usize {
    let Some(mut p) = crate::vehicle::automobile::Parts::of(ra, a) else { return 0 };
    if p.base.status != status::SIMPLE {
        p.veh.flags.vehicle_col_processed = true;
    }
    let col = if p.veh.flags.using_special_col_model { Some(std::sync::Arc::new(ents.player_col_model.clone())) } else { ents.col_model(p.base.model_index) };
    let cb = ents.col_model(b.base.model_index);
    let (Some(col), Some(cb)) = (col, cb) else {
        p.finish();
        return 0;
    };
    let prev_ratios = p.veh.suspension_spring_ratio;
    let mut wheel_points = p.car.wheel_col_points;
    let mut ratios = p.veh.suspension_spring_ratio;
    let n = collision::process_col_models(&p.base.matrix, &col, &b.base.matrix, &cb, points, &mut wheel_points, &mut ratios);
    p.car.wheel_col_points = wheel_points;
    p.veh.suspension_spring_ratio = ratios;
    // the ratios now point at where the tyre touches; ProcessControl rescales them

    let mut wheel_hits = 0;
    let dodo_vs_physical = p.base.model_index == ents.indices.dodo() && (is_ped(b) || is_vehicle(b));
    if p.phys.is_vehicle_being_shifted || p.phys.flags.skip_line_col || dodo_vs_physical {
        p.veh.suspension_spring_ratio = prev_ratios;
    } else {
        for i in 0..4 {
            if p.veh.suspension_spring_ratio[i] < 1.0 && p.veh.suspension_spring_ratio[i] < prev_ratios[i] {
                wheel_hits += 1;
                if is_vehicle(b) || is_object(b) {
                    // the wheel touches a physical
                    p.car.ground_physical[i] = Some(rb);
                    p.car.ground_offset[i] = p.car.wheel_col_points[i].point - b.base.position();
                    if b.base.model_index == ents.indices.body_cast() && p.base.status == status::PLAYER {
                        let speed = crate::math::magnitude_squared(p.phys.move_speed);
                        if speed > 0.1 {
                            ents.objects.body_cast_health = (ents.objects.body_cast_health as f32 - 0.1 * p.phys.mass * speed) as i16;
                            ents.events.push(WorldEvent::Car(crate::vehicle::env::CarEvent::Sound { sound: crate::vehicle::env::CarSound::PedBodycastHit, volume: 0.0 }));
                        }
                        if b.base.flags.is_static {
                            b.base.flags.is_static = false;
                            if let Some(bp) = b.phys.as_mut() {
                                bp.static_frames = 0;
                                bp.apply_move_force(p.phys.move_speed / speed.sqrt());
                            }
                            ents.pending_moving.push(rb);
                        }
                    }
                }
                p.phys.surface_touched = p.car.wheel_col_points[i].surface_b;
                if is_building(b) {
                    p.veh.cur_ground_entity = Some(rb);
                }
            }
        }
    }

    p.finish();
    if n > 0 || wheel_hits > 0 {
        let t = ents.timer.time_in_ms;
        add_collision_record(a, b, rb, t);
        if !is_building(b) {
            add_collision_record(b, a, ra, t);
        }
        if n > 0 && (is_building(b) || (is_object(b) && b.phys.as_ref().is_some_and(|p| p.flags.infinite_mass))) {
            a.base.flags.has_hit_wall = true;
        }
    }
    n
}

/// `ProcessEntityCollision`: A's col model against B's (no lines), recording the contact.
fn process_entity_collision(ents: &mut Ents, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, points: &mut [ColPoint]) -> usize {
    if matches!(&a.kind, Kind::Vehicle(v) if matches!(v.sub, crate::vehicle::base::VehicleSub::Car(_))) {
        return car_entity_collision(ents, a, ra, b, rb, points);
    }
    let (Some(ca), Some(cb)) = (ents.col_model(a.base.model_index), ents.col_model(b.base.model_index)) else { return 0 };
    let n = collision::process_col_models(&a.base.matrix, &ca, &b.base.matrix, &cb, points, &mut [], &mut []);
    if n > 0 {
        let t = ents.timer.time_in_ms;
        add_collision_record(a, b, rb, t);
        if !is_building(b) {
            add_collision_record(b, a, ra, t);
        }
        if is_building(b) || b.base.flags.is_static {
            a.base.flags.has_hit_wall = true;
        }
    }
    n
}

fn adhesive_limit(ents: &Ents, c: &ColPoint) -> f32 {
    let g = ents.models.game;
    let (a, b) = (surface::adhesion_group(g, c.surface_b), surface::adhesion_group(g, c.surface_a));
    ents.adhesive_limits.get(a).and_then(|r| r.get(b)).copied().unwrap_or(0.0)
}

/// Whether A (an object being tested against a vehicle) passes through it for now.
/// `inv` is B's matrix inverse; true means "skip", with the object remembering the vehicle.
fn object_passes_vehicle(ents: &Ents, obj: &Ent, veh: &Ent) -> bool {
    let Some(col) = ents.col_model(obj.base.model_index) else { return false };
    let size = obj.base.matrix.transform_point(col.bounding_box.max - col.bounding_box.min);
    size.z < veh.base.position().z || veh.base.matrix.inverted().transform_point(size).z < 0.0
}

#[allow(clippy::if_same_then_else)] // the branches mirror the original's case list
/// `ProcessShiftSectorList`: pushes A out of whatever it overlaps. True if it moved.
fn process_shift_sector_list(ents: &mut Ents, sector: &Sector, a: &mut Ent, ra: EntityRef) -> bool {
    let Some(col_a) = ents.col_model(a.base.model_index) else { return false };
    let (centre, radius) = (a.base.bound_centre(&col_a), a.base.bound_radius(&col_a));
    let mut points = vec![ColPoint::default(); collision::MAX_SPHERES];
    let mut shift = Vec3::ZERO;
    let mut do_shift = false;
    let mut boat: Option<EntityRef> = None;
    let code = ents.scan_code;

    for list in 0..=7 {
        for &rb in &sector.lists[list] {
            let Some(b) = ents.pools.get(rb) else { continue };
            let infinite = b.phys.as_ref().is_some_and(|p| p.flags.infinite_mass);
            let b_damaged = matches!(&b.kind, Kind::Object(o) if o.has_been_damaged);
            let can_shift = is_building(b) || (is_object(b) && infinite) || (is_ped(a) && is_object(b) && b.base.flags.is_static && !b_damaged);
            let touching = ents.col_model(b.base.model_index).is_some_and(|c| b.base.is_touching(&c, centre, radius));
            if rb == ra || b.base.scan_code == code || !b.base.flags.uses_collision || (a.base.flags.has_hit_wall && !can_shift) || !touching {
                continue;
            }

            let (a_model, b_model) = (a.base.model_index, b.base.model_index);
            let a_street = ents.indices.is_street_light(a_model);
            let b_street = ents.indices.is_street_light(b_model);
            let rc = ents.indices.rc_bandit();
            let a_colliding = match &a.kind {
                Kind::Ped(p) => p.colliding_entity,
                Kind::Object(o) => o.colliding_entity,
                _ => None,
            };
            let b_colliding = match &b.kind {
                Kind::Ped(p) => p.colliding_entity,
                Kind::Object(o) => o.colliding_entity,
                _ => None,
            };
            let a_obj_data = if let Kind::Object(o) = &a.kind { Some((o.created_by, o.has_been_damaged, o.colliding_entity)) } else { None };
            let b_obj_data = if let Kind::Object(o) = &b.kind { Some((o.created_by, o.has_been_damaged, o.colliding_entity)) } else { None };

            use crate::entity::object::CreatedBy;
            let mut skip = false;
            let mut set_a_colliding = None;
            let mut reset_b_colliding = false;
            if is_building(b) {
            } else if a_street && (is_vehicle(b) || is_ped(b)) && a.base.matrix.up.z < 0.66 {
                skip = true;
            } else if (is_vehicle(a) || is_ped(a)) && b.base.matrix.up.z < 0.66 && b_street {
                skip = true;
            } else if is_object(a) && is_vehicle(b) {
                let (created_by, damaged, colliding) = a_obj_data.unwrap();
                if created_by != CreatedBy::Temp && !damaged && a.base.flags.is_static {
                    if colliding == Some(rb) {
                        set_a_colliding = Some(None);
                    }
                } else if colliding != Some(rb) {
                    if object_passes_vehicle(ents, a, b) {
                        skip = true;
                        set_a_colliding = Some(Some(rb));
                    }
                } else {
                    skip = true;
                }
            } else if is_object(b) && is_vehicle(a) {
                let (created_by, damaged, colliding) = b_obj_data.unwrap();
                if created_by != CreatedBy::Temp && !damaged && b.base.flags.is_static {
                    reset_b_colliding = colliding == Some(ra);
                } else if colliding != Some(ra) {
                    skip = object_passes_vehicle(ents, b, a);
                } else {
                    skip = true;
                }
            } else if ents.indices.is_body_part(a_model) && is_ped(b) {
                skip = true;
            } else if is_ped(a) && ents.indices.is_body_part(b_model) {
                skip = true;
            } else if (is_ped(a) && a_colliding == Some(rb)) || (is_ped(b) && b_colliding == Some(ra)) {
                skip = true;
            } else if (a_model == rc && is_vehicle(b)) || (b_model == rc && (is_ped(a) || is_vehicle(a))) {
                skip = true;
            }

            // remembering or forgetting the entity an object is passing through
            if let Some(v) = set_a_colliding
                && let Kind::Object(o) = &mut a.kind
            {
                o.colliding_entity = v;
            }
            if reset_b_colliding
                && let Some(Kind::Object(o)) = ents.pools.get_mut(rb).map(|b| &mut b.kind)
            {
                o.colliding_entity = None;
            }
            if skip {
                continue;
            }

            let Some(mut b) = ents.pools.take(rb) else { continue };
            b.base.scan_code = code;
            let n = process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
            if n == 0 {
                ents.pools.restore(rb, b);
                continue;
            }

            let mut most = 0;
            for j in 1..n {
                if points[j].depth > points[most].depth {
                    most = j;
                }
            }
            let (k_all, k_most) = if ents.second_shift { (1.5, 0.4) } else { (1.2, 0.2) };
            for p in &points[..n] {
                shift += p.normal * p.depth * k_all / n as f32;
            }

            let b_is_boat = matches!(&b.kind, Kind::Vehicle(v) if v.is_boat());
            let a_is_boat = matches!(&a.kind, Kind::Vehicle(v) if v.is_boat());
            if is_vehicle(a) && is_vehicle(&b) {
                let mut dir = crate::math::normalise(a.base.position() - b.base.position());
                if dir.z < 0.0 && dir.z < a.base.matrix.forward.z && dir.z < a.base.matrix.right.z {
                    dir.z = 0.0f32.min(a.base.matrix.forward.z.min(a.base.matrix.right.z));
                }
                shift += dir * points[most].depth * 0.5;
            } else if is_ped(a) && b_is_boat {
                let mut dir = points[most].normal;
                let f = dir.z.abs().min(0.9);
                dir.z = 0.0;
                dir = crate::math::normalise(dir);
                shift += dir * points[most].depth / (1.0 - f);
                boat = Some(rb);
            } else if is_ped(&b) && a_is_boat {
                let mut dir = points[most].normal * -1.0;
                let f = dir.z.abs().min(0.9);
                dir.z = 0.0;
                dir = crate::math::normalise(dir);
                b.base.matrix.pos += dir * points[most].depth / (1.0 - f);
            } else {
                shift += points[most].normal * points[most].depth * k_most;
            }
            do_shift = true;
            ents.pools.restore(rb, b);
        }
    }

    if !do_shift {
        return false;
    }
    a.base.matrix.pos += shift;
    if let Some(rb) = boat
        && let Some(mut b) = ents.pools.take(rb)
    {
        process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
        ents.pools.restore(rb, b);
    }
    true
}

/// Pushes the pair's impulses into the damage records and reports the hit.
#[allow(clippy::too_many_arguments)] // two entities, their handles and the contact set
fn record_hit(ents: &mut Ents, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, c: &ColPoint, impulse_a: f32, impulse_b: f32, relative: bool) {
    if impulse_a > phys(a).damage_impulse {
        phys(a).set_damaged_piece_record(c.piece_a as u16, impulse_a, rb, c.normal);
    }
    if relative && impulse_b > phys(b).damage_impulse {
        phys(b).set_damaged_piece_record(c.piece_b as u16, impulse_b, ra, c.normal);
    }
    let (pa, pb) = (phys(a).clone(), phys(b).clone());
    let (turn, mv) = if relative {
        (magnitude_squared(pb.turn_speed - pa.turn_speed), magnitude_squared(pb.move_speed - pa.move_speed))
    } else {
        (magnitude_squared(pa.turn_speed), magnitude_squared(pa.move_speed))
    };
    ents.events.push(WorldEvent::ReportCollision { a: ra, b: rb, surface_a: c.surface_a, surface_b: c.surface_b, impulse: impulse_a, speed_diff: turn.max(mv) });
}

/// One pass of `ApplyCollision` (and optionally friction) over the contact points.
/// `maxima` collects the largest impulses on A and B.
#[allow(clippy::too_many_arguments)] // two entities, their handles and the contact set
fn collide_points(ents: &mut Ents, rng: &mut Rng, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, points: &[ColPoint], friction: bool, maxima: &mut (f32, f32)) {
    let n = points.len() as f32;
    for c in points {
        let r = {
            let Ents { models, indices, objects, timer, no_more_collision_torque, .. } = &mut *ents;
            let mut cx = Ctx { step: timer.time_step, no_more_collision_torque: *no_more_collision_torque, indices, models, objects, rng, events: vec![] };
            let r = collide::apply_collision(a, ra, b, rb, c, &mut cx);
            let evs = std::mem::take(&mut cx.events);
            ents.events.extend(evs.into_iter().map(WorldEvent::Collision));
            r
        };
        if r.b_to_moving_list {
            ents.pending_moving.push(rb);
        }
        if !r.applied {
            continue;
        }
        maxima.0 = maxima.0.max(r.impulse_a);
        maxima.1 = maxima.1.max(r.impulse_b);
        record_hit(ents, a, ra, b, rb, c, r.impulse_a, r.impulse_b, true);
        if friction {
            let limit = adhesive_limit(ents, c) / n;
            let step = ents.timer.time_step;
            if collide::apply_friction_pair(a, b, limit, c, step) {
                a.base.flags.has_contacted = true;
                b.base.flags.has_contacted = true;
            }
        }
    }
}

/// The has-contacted dance around `collide_points` shared by both sector-list processors.
#[allow(clippy::too_many_arguments)] // two entities, their handles and the contact set
fn respond_pair(ents: &mut Ents, rng: &mut Rng, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, points: &[ColPoint], maxima: &mut (f32, f32)) {
    let (ac, bc) = (a.base.flags.has_contacted, b.base.flags.has_contacted);
    if ac && bc {
        collide_points(ents, rng, a, ra, b, rb, points, false, maxima);
    } else if ac || bc {
        let who: &mut Ent = if ac { a } else { b };
        let saved = {
            let p = phys(who);
            let s = (p.move_friction, p.turn_friction);
            p.move_friction = Vec3::ZERO;
            p.turn_friction = Vec3::ZERO;
            who.base.flags.has_contacted = false;
            s
        };
        collide_points(ents, rng, a, ra, b, rb, points, true, maxima);
        let who: &mut Ent = if ac { a } else { b };
        if !who.base.flags.has_contacted {
            who.base.flags.has_contacted = true;
            let p = phys(who);
            p.move_friction = saved.0;
            p.turn_friction = saved.1;
        }
    } else {
        collide_points(ents, rng, a, ra, b, rb, points, true, maxima);
    }
}

fn wake_if_simple(ents: &mut Ents, b: &mut Ent, rb: EntityRef) {
    if b.base.status == status::SIMPLE {
        b.base.status = status::PHYSICS;
        if is_vehicle(b) {
            ents.events.push(WorldEvent::SwitchToRealPhysics(rb));
        }
    }
}

#[allow(clippy::if_same_then_else)]
/// `ProcessCollisionSectorList`. True when A collided with something and processing ends.
fn process_collision_sector_list(ents: &mut Ents, rng: &mut Rng, sector: &Sector, a: &mut Ent, ra: EntityRef) -> bool {
    let Some(col_a) = ents.col_model(a.base.model_index) else { return false };
    let (centre, radius) = (a.base.bound_centre(&col_a), a.base.bound_radius(&col_a));
    let mut points = vec![ColPoint::default(); collision::MAX_SPHERES];
    let (car_bumper, files, rc, train) = model_ids(ents);
    let code = ents.scan_code;
    let mut impulse_a;

    for list in 0..=7 {
        for &rb in &sector.lists[list] {
            let Some(b) = ents.pools.get(rb) else { continue };
            if rb == ra || b.base.scan_code == code || !b.base.flags.uses_collision {
                continue;
            }
            let touching = ents.col_model(b.base.model_index).is_some_and(|c| b.base.is_touching(&c, centre, radius));
            if !touching {
                // forget a "passing through" entity once it is clear
                let forget_a = match &a.kind {
                    Kind::Object(o) => o.colliding_entity == Some(rb),
                    Kind::Ped(p) => p.colliding_entity == Some(rb),
                    _ => false,
                };
                if forget_a {
                    match &mut a.kind {
                        Kind::Object(o) => o.colliding_entity = None,
                        Kind::Ped(p) => p.colliding_entity = None,
                        _ => {}
                    }
                } else {
                    let forget_b = match &b.kind {
                        Kind::Object(o) => o.colliding_entity == Some(ra),
                        Kind::Ped(p) => p.colliding_entity == Some(ra),
                        _ => false,
                    };
                    if forget_b && let Some(bm) = ents.pools.get_mut(rb) {
                        match &mut bm.kind {
                            Kind::Object(o) => o.colliding_entity = None,
                            Kind::Ped(p) => p.colliding_entity = None,
                            _ => {}
                        }
                    }
                }
                continue;
            }

            phys(a).flags.skip_line_col = false;
            let (a_model, b_model) = (a.base.model_index, b.base.model_index);
            let (mut skip, mut alt) = (false, false);
            let mut b_colliding_set: Option<Option<EntityRef>> = None;
            let b_up_z = b.base.matrix.up.z;
            let b_obj = if let Kind::Object(o) = &b.kind { Some((o.created_by, o.has_been_damaged, o.colliding_entity, o.uproot_limit)) } else { None };
            let a_obj = if let Kind::Object(o) = &a.kind { Some((o.created_by, o.has_been_damaged, o.colliding_entity)) } else { None };
            let a_ped_colliding = if let Kind::Ped(p) = &a.kind { Some((p.colliding_entity, p.knocked_up_into_air)) } else { None };
            let b_ped_colliding = if let Kind::Ped(p) = &b.kind { p.colliding_entity } else { None };
            use crate::entity::object::CreatedBy;

            if is_building(b) {
            } else if ents.indices.is_street_light(a_model) && (is_vehicle(b) || is_ped(b)) && a.base.matrix.up.z < 0.66 {
                skip = true;
                phys(a).flags.skip_line_col = true;
                if let Kind::Object(o) = &mut a.kind {
                    o.colliding_entity = Some(rb);
                }
            } else if (is_vehicle(a) || is_ped(a)) && b_up_z < 0.66 && ents.indices.is_street_light(b_model) {
                skip = true;
                phys(a).flags.skip_line_col = true;
                b_colliding_set = Some(Some(ra));
            } else if is_object(a) && is_vehicle(b) {
                let (created_by, damaged, colliding) = a_obj.unwrap();
                if a_model == car_bumper || a_model == files {
                    skip = true;
                } else if created_by == CreatedBy::Temp || damaged || !a.base.flags.is_static {
                    if colliding == Some(rb) {
                        skip = true;
                    } else if object_passes_vehicle(ents, a, b) {
                        skip = true;
                        if let Kind::Object(o) = &mut a.kind {
                            o.colliding_entity = Some(rb);
                        }
                    }
                }
            } else if is_object(b) && is_vehicle(a) {
                let (created_by, damaged, colliding, _) = b_obj.unwrap();
                if b_model == car_bumper || b_model == files {
                    skip = true;
                } else if (created_by == CreatedBy::Temp || damaged || !b.base.flags.is_static) && (colliding == Some(ra) || object_passes_vehicle(ents, b, a)) {
                    skip = true;
                }
            } else if ents.indices.is_body_part(a_model) && is_ped(b) {
                skip = true;
            } else if is_ped(a) && ents.indices.is_body_part(b_model) {
                skip = true;
                phys(a).flags.skip_line_col = true;
            } else if is_ped(a) && a_ped_colliding.is_some_and(|(c, _)| c == Some(rb)) {
                skip = true;
                if !a_ped_colliding.is_some_and(|(_, k)| k) {
                    phys(a).flags.skip_line_col = true;
                }
            } else if is_ped(b) && b_ped_colliding == Some(ra) {
                skip = true;
                phys(a).flags.skip_line_col = true;
            } else if (a_model == rc && (is_ped(b) || is_vehicle(b))) || (b_model == rc && (is_ped(a) || is_vehicle(a))) {
                skip = true;
                phys(a).flags.skip_line_col = true;
            } else if is_ped(a) && is_object(b) && b_obj.is_some_and(|o| o.3 > 0.0) {
                alt = true;
            }

            if let Some(v) = b_colliding_set
                && let Some(bm) = ents.pools.get_mut(rb)
                && let Kind::Object(o) = &mut bm.kind
            {
                o.colliding_entity = v;
            }

            let Some(mut b) = ents.pools.take(rb) else { continue };
            b.base.scan_code = code;
            let b_stuck = b.base.flags.is_stuck;
            let b_infinite = b.phys.as_ref().is_some_and(|p| p.flags.infinite_mass);

            if !a.base.flags.uses_collision || skip {
                process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
                ents.pools.restore(rb, b);
            } else if is_building(&b) || b_stuck || b_infinite || alt {
                // B does not move
                let n = process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
                if n == 0 {
                    ents.pools.restore(rb, b);
                    continue;
                }
                let (mut move_speed, mut turn_speed) = (Vec3::ZERO, Vec3::ZERO);
                let mut responses = 0;
                let b_solid = is_building(&b) || b_infinite;
                impulse_a = -1.0;
                let had_contact = a.base.flags.has_contacted;
                for c in &points[..n] {
                    let step = ents.timer.time_step;
                    if !collide::apply_collision_alt(a, b_solid, c, step, &mut impulse_a, &mut move_speed, &mut turn_speed) {
                        continue;
                    }
                    responses += 1;
                    if impulse_a > phys(a).damage_impulse {
                        phys(a).set_damaged_piece_record(c.piece_a as u16, impulse_a, rb, c.normal);
                    }
                    let mut imp = impulse_a;
                    let pa = phys(a).clone();
                    if is_vehicle(a) && a.base.matrix.up.z < -0.6 && pa.move_speed.x.abs() < 0.05 && pa.move_speed.y.abs() < 0.05 {
                        imp *= 0.1;
                    }
                    let speed_diff = magnitude_squared(pa.turn_speed).max(magnitude_squared(pa.move_speed));
                    ents.events.push(WorldEvent::ReportCollision { a: ra, b: rb, surface_a: c.surface_a, surface_b: c.surface_b, impulse: imp, speed_diff });

                    if !had_contact {
                        let mut adhesion = adhesive_limit(ents, c) / n as f32;
                        let game = ents.models.game;
                        if a_model == rc {
                            adhesion *= 0.2;
                        } else if ents.indices.is_boat_model(a_model) {
                            if c.normal.z > 0.6 {
                                if surface::adhesion_group(game, c.surface_b) == surface::ADHESIVE_LOOSE {
                                    adhesion *= 3.0;
                                }
                            } else {
                                adhesion = 0.0;
                            }
                        } else if is_vehicle(a) {
                            if a.base.status == status::WRECKED {
                                adhesion *= 3.0;
                            } else if a.base.matrix.up.z > 0.3 {
                                adhesion = 0.0;
                            } else {
                                adhesion *= 5.0f32.min(0.03 * impulse_a + 1.0);
                            }
                        }
                        let mut evs = vec![];
                        if collide::apply_friction(a, adhesion, c, step, game, &mut evs) {
                            a.base.flags.has_contacted = true;
                        }
                        ents.events.extend(evs.into_iter().map(WorldEvent::Collision));
                    }
                }
                ents.pools.restore(rb, b);
                if responses > 0 {
                    let p = phys(a);
                    p.move_speed += move_speed / responses as f32;
                    p.turn_speed += turn_speed / responses as f32;
                    if !ents.no_more_collision_torque && a.base.status == status::PLAYER && is_vehicle(a) {
                        let p = phys(a);
                        if p.move_speed.x.abs() > 0.2 && p.move_speed.y.abs() > 0.2 {
                            p.move_friction.x += move_speed.x * -0.3 / n as f32;
                            p.move_friction.y += move_speed.y * -0.3 / n as f32;
                            p.turn_friction += turn_speed * -0.3 / n as f32;
                        }
                    }
                    return true;
                }
            } else {
                // B can move
                let n = process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
                if n == 0 {
                    ents.pools.restore(rb, b);
                    continue;
                }
                let mut maxima = (0.0f32, 0.0f32);
                respond_pair(ents, rng, a, ra, &mut b, rb, &points[..n], &mut maxima);

                let a_speed_sq = magnitude_squared(phys(a).move_speed);
                if is_ped(&b) && is_vehicle(a) && (!matches!(&b.kind, Kind::Ped(p) if p.is_player) || (b.base.flags.has_hit_wall && a_speed_sq > 0.05 * 0.05)) {
                    ents.events.push(WorldEvent::KillPedWithCar { ped: rb, vehicle: ra, impulse: maxima.1 });
                } else if b_model == train && is_ped(a) && (!matches!(&a.kind, Kind::Ped(p) if p.is_player) || a.base.flags.has_hit_wall) {
                    ents.events.push(WorldEvent::KillPedWithCar { ped: ra, vehicle: rb, impulse: maxima.0 * 2.0 });
                } else if is_object(&b) && b.base.flags.uses_collision && is_vehicle(a) {
                    // the original passes impulse B here too
                    damage_object(ents, rng, &mut b, rb, maxima.1);
                } else if is_object(a) && a.base.flags.uses_collision && is_vehicle(&b) {
                    damage_object(ents, rng, a, ra, maxima.1);
                }
                wake_if_simple(ents, &mut b, rb);
                ents.pools.restore(rb, b);
                return true;
            }
        }
    }
    false
}

fn damage_object(ents: &mut Ents, rng: &mut Rng, obj: &mut Ent, r: EntityRef, impulse: f32) {
    let has_effect = matches!(&obj.kind, Kind::Object(o) if o.collision_damage_effect != 0);
    if has_effect && impulse > 20.0 {
        let body_cast = ents.indices.body_cast();
        let evs = ents.objects.object_damage(obj, impulse, body_cast, rng);
        ents.events.extend(evs.into_iter().map(|e| WorldEvent::Collision(CollisionEvent::Object(e))));
        let _ = r;
    }
}

/// `ProcessCollisionSectorList_SimpleCar`: a simple (script-driven) car only checks vehicles and objects.
fn process_collision_sector_list_simple_car(ents: &mut Ents, rng: &mut Rng, sector: &Sector, a: &mut Ent, ra: EntityRef) -> bool {
    let Some(col_a) = ents.col_model(a.base.model_index) else { return false };
    let (centre, radius) = (a.base.bound_centre(&col_a), a.base.bound_radius(&col_a));
    let mut points = vec![ColPoint::default(); collision::MAX_SPHERES];
    let code = ents.scan_code;

    // vehicles and objects, last list first
    for list in [super::OBJECTS + 1, super::OBJECTS, super::VEHICLES + 1, super::VEHICLES] {
        for &rb in &sector.lists[list] {
            let Some(b) = ents.pools.get(rb) else { continue };
            let touching = ents.col_model(b.base.model_index).is_some_and(|c| b.base.is_touching(&c, centre, radius));
            if rb == ra || b.base.scan_code == code || !b.base.flags.uses_collision || !touching {
                continue;
            }
            let Some(mut b) = ents.pools.take(rb) else { continue };
            b.base.scan_code = code;
            let n = process_entity_collision(&mut *ents, a, ra, &mut b, rb, &mut points);
            if n == 0 {
                ents.pools.restore(rb, b);
                continue;
            }
            // the original leaves the impulses at -1 when nothing responds, and ignores the maxima
            let mut maxima = (0.0, 0.0);
            respond_pair(ents, rng, a, ra, &mut b, rb, &points[..n], &mut maxima);
            wake_if_simple(ents, &mut b, rb);
            ents.pools.restore(rb, b);
            return true;
        }
    }
    false
}

impl World {
    /// `AddToMovingList` for entities a collision just woke up.
    pub(super) fn flush_moving(&mut self) {
        for r in std::mem::take(&mut self.ents.pending_moving) {
            if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut())
                && !p.in_moving_list
            {
                p.in_moving_list = true;
                self.moving.push_front(r);
            }
        }
    }

    /// Runs `f` on A lent out of its pool, then puts it back.
    fn with_entity<T>(&mut self, r: EntityRef, f: impl FnOnce(&mut World, &mut Ent) -> T) -> Option<T> {
        let mut e = self.ents.pools.take(r)?;
        let out = f(self, &mut e);
        self.ents.pools.restore(r, e);
        self.flush_moving();
        Some(out)
    }

    fn sectors_of(&self, e: &Ent) -> Vec<Sector> {
        e.entry_info.iter().filter_map(|i| self.sector(i.sector.0, i.sector.1).cloned()).collect()
    }

    /// `CheckCollision`: true if A collided with something.
    pub fn check_collision(&mut self, r: EntityRef, rng: &mut Rng) -> bool {
        self.check(r, rng, false)
    }

    /// `CheckCollision_SimpleCar`.
    pub fn check_collision_simple_car(&mut self, r: EntityRef, rng: &mut Rng) -> bool {
        self.check(r, rng, true)
    }

    fn check(&mut self, r: EntityRef, rng: &mut Rng, simple: bool) -> bool {
        self.advance_scan_code();
        self.with_entity(r, |w, a| {
            a.base.flags.collision_processed = false;
            let sectors = w.sectors_of(a);
            for s in &sectors {
                let hit = if simple { process_collision_sector_list_simple_car(&mut w.ents, rng, s, a, r) } else { process_collision_sector_list(&mut w.ents, rng, s, a, r) };
                if hit {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
    }

    /// `ProcessShift`: moves the entity by its speed, pushes it out of overlaps and refiles it.
    pub fn process_shift(&mut self, r: EntityRef, rng: &mut Rng) {
        let step = self.ents.timer.time_step;
        let Some(e) = self.get_mut(r) else { return };
        let phys = e.phys.as_mut().expect("physical");
        phys.distance_travelled = 0.0;
        if e.base.status == status::SIMPLE {
            e.base.flags.is_stuck = false;
            e.base.flags.is_in_safe_position = true;
            let _ = self.remove_and_add(r);
            return;
        }
        let saved = e.base.matrix;
        phys.apply_move_speed(&mut e.base.matrix, step);
        phys.apply_turn_speed(&mut e.base.matrix, step);
        e.base.matrix.reorthogonalise();
        if e.base.ty == EntityType::Vehicle {
            phys.is_vehicle_being_shifted = true;
        }
        self.advance_scan_code();
        let shifted = self
            .with_entity(r, |w, a| {
                let sectors = w.sectors_of(a);
                let mut shifted = false;
                for s in &sectors {
                    shifted |= process_shift_sector_list(&mut w.ents, s, a, r);
                }
                a.phys.as_mut().unwrap().is_vehicle_being_shifted = false;
                shifted
            })
            .unwrap_or(false);
        if shifted {
            self.advance_scan_code();
            let hit = self
                .with_entity(r, |w, a| {
                    let sectors = w.sectors_of(a);
                    for s in &sectors {
                        if process_collision_sector_list(&mut w.ents, rng, s, a, r) {
                            a.base.matrix = saved;
                            return true;
                        }
                    }
                    false
                })
                .unwrap_or(false);
            if hit {
                return;
            }
        }
        let Some(e) = self.get_mut(r) else { return };
        e.base.flags.is_stuck = false;
        e.base.flags.is_in_safe_position = true;
        e.phys.as_mut().unwrap().distance_travelled = magnitude(e.base.position() - saved.pos);
        let _ = self.remove_and_add(r);
    }

    /// `ProcessCollision`: the main per-frame movement and collision of a physical, in sub-steps
    /// for fast ones. The refiling happens at the end.
    pub fn process_collision(&mut self, r: EntityRef, rng: &mut Rng) {
        let Some(e) = self.get_mut(r) else { return };
        let p = e.phys.as_mut().expect("physical");
        p.distance_travelled = 0.0;
        p.is_vehicle_being_shifted = false;
        p.flags.skip_line_col = false;

        if !e.base.flags.uses_collision {
            e.base.flags.is_stuck = false;
            e.base.flags.is_in_safe_position = true;
            let _ = self.remove_and_add(r);
            return;
        }

        if e.base.status == status::SIMPLE {
            if self.check_collision_simple_car(r, rng) && self.get(r).is_some_and(|e| e.base.status == status::SIMPLE) {
                let e = self.get_mut(r).unwrap();
                e.base.status = status::PHYSICS;
                if e.base.ty == EntityType::Vehicle {
                    self.ents.events.push(WorldEvent::SwitchToRealPhysics(r));
                }
            }
            let e = self.get_mut(r).unwrap();
            e.base.flags.is_stuck = false;
            e.base.flags.is_in_safe_position = true;
            let _ = self.remove_and_add(r);
            return;
        }

        let e = self.get(r).unwrap();
        let saved_matrix = e.base.matrix;
        let saved_step = self.ents.timer.time_step;
        let mut n = 1i32;
        let mut step = 0.0f32;
        let move_speed = e.phys.as_ref().unwrap().move_speed;
        let mut dist_sq = magnitude_squared(move_speed) * (saved_step * saved_step);
        let num_steps = |dist_sq: f32, x: f32| (dist_sq.sqrt() * (1.0 / x)).ceil() as i32;
        let is_player_ped = matches!(&e.kind, Kind::Ped(p) if p.is_player);
        let a_ty = e.base.ty;

        if a_ty == EntityType::Ped && (dist_sq >= 0.2 * 0.2 || is_player_ped) {
            n = if is_player_ped { num_steps(dist_sq, 0.2).max(2) } else { num_steps(dist_sq, 0.3) };
            step = saved_step / n as f32;
        } else if a_ty == EntityType::Vehicle && dist_sq >= 0.4 * 0.4 {
            n = if e.base.status == status::PLAYER {
                num_steps(dist_sq, 0.2)
            } else if dist_sq > 0.32 {
                num_steps(dist_sq, 0.3)
            } else {
                num_steps(dist_sq, 0.4)
            };
            step = saved_step / n as f32;
        } else if a_ty == EntityType::Object {
            let response = if let Kind::Object(o) = &e.kind { o.special_collision_response } else { 0 };
            match response {
                1 => {
                    // lamp post: judged by its ends
                    let radius = self.ents.col_model(e.base.model_index).map_or(0.0, |c| e.base.bound_radius(&c));
                    let up = e.base.matrix.rotate_vector(Vec3::new(0.0, 0.0, radius));
                    let down = e.base.matrix.rotate_vector(Vec3::new(0.0, 0.0, -radius));
                    let p = e.phys.as_ref().unwrap();
                    let (su, sd) = (p.speed_at(up), p.speed_at(down));
                    dist_sq = magnitude_squared(su).max(magnitude_squared(sd)) * (saved_step * saved_step);
                    if dist_sq >= 0.3 * 0.3 {
                        n = num_steps(dist_sq, 0.3);
                        step = saved_step / n as f32;
                    }
                }
                5 => {
                    if dist_sq >= 0.009 {
                        n = num_steps(dist_sq, 0.09);
                        step = saved_step / n as f32;
                    }
                }
                2 | 4 => {
                    if dist_sq >= 0.15 * 0.15 {
                        n = num_steps(dist_sq, 0.15);
                        step = saved_step / n as f32;
                    }
                }
                _ => {
                    if dist_sq >= 0.3 * 0.3 {
                        n = num_steps(dist_sq, 0.3);
                        step = saved_step / n as f32;
                    }
                }
            }
        }
        // the original stores this in an int8
        let n = n.clamp(1, 127);

        let mut saved_matrix = saved_matrix;
        for i in 1..n {
            self.ents.timer.time_step = i as f32 * step;
            let t = self.ents.timer.time_step;
            let e = self.get_mut(r).unwrap();
            let p = e.phys.as_mut().unwrap();
            p.apply_move_speed(&mut e.base.matrix, t);
            p.apply_turn_speed(&mut e.base.matrix, t);
            let hit = self.check_collision(r, rng);
            let e = self.get_mut(r).unwrap();
            let standing_fix = matches!(&e.kind, Kind::Ped(p) if !p.was_standing && p.is_standing) && e.phys.as_ref().unwrap().move_speed.z == 0.0;
            if standing_fix {
                saved_matrix.pos.z = e.base.position().z;
            }
            e.base.matrix = saved_matrix;
            self.ents.timer.time_step = saved_step;
            if hit {
                return;
            }
            if let Kind::Vehicle(v) = &mut self.get_mut(r).unwrap().kind {
                v.suspension_spring_ratio = [1.0; 4];
            }
        }

        let e = self.get_mut(r).unwrap();
        let p = e.phys.as_mut().unwrap();
        p.apply_move_speed(&mut e.base.matrix, saved_step);
        p.apply_turn_speed(&mut e.base.matrix, saved_step);
        e.base.matrix.reorthogonalise();
        p.is_vehicle_being_shifted = false;
        p.flags.skip_line_col = false;
        let moves = p.move_speed != Vec3::ZERO || p.turn_speed != Vec3::ZERO || p.flags.hit_by_train || e.base.status == status::PLAYER || is_player_ped;
        if moves {
            if let Kind::Vehicle(v) = &mut e.kind {
                v.flags.vehicle_col_processed = true;
            }
            if self.check_collision(r, rng) {
                self.get_mut(r).unwrap().base.matrix = saved_matrix;
                return;
            }
        }
        let e = self.get_mut(r).unwrap();
        let p = e.phys.as_mut().unwrap();
        p.flags.hit_by_train = false;
        p.distance_travelled = magnitude(e.base.position() - saved_matrix.pos);
        p.flags.skip_line_col = false;
        e.base.flags.is_stuck = false;
        e.base.flags.is_in_safe_position = true;
        let _ = self.remove_and_add(r);
    }

    /// Takes the events the physics queued since the last call.
    pub fn drain_events(&mut self) -> Vec<WorldEvent> {
        std::mem::take(&mut self.ents.events)
    }
}
