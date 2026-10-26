//! `CPed::ProcessEntityCollision`: a ped's model against another entity, plus the line under its feet
//! that decides what it stands on (GTA III, without `VC_PED_PORTS`).
use super::physics::{add_collision_record, WorldEvent};
use super::Ents;
use crate::anim::tables_gta3::anim;
use crate::collision::{self, ColPoint, Line, LineFilter};
use crate::entity::{Ent, EntityRef, EntityType, Kind};
use crate::math::normalise;
use crate::ped::data::{state, FEET_OFFSET};
use crate::ped::fight::local_direction;
use crate::surface;
use glam::{Vec2, Vec3};

/// The ped `a` against `b`; returns how many of its spheres touch.
pub(super) fn ped_entity_collision(ents: &mut Ents, a: &mut Ent, ra: EntityRef, b: &mut Ent, rb: EntityRef, points: &mut [ColPoint]) -> usize {
    let (Some(ca), Some(cb)) = (ents.col_model(a.base.model_index), ents.col_model(b.base.model_index)) else { return 0 };
    if !a.base.flags.uses_collision {
        return 0;
    }
    let game = ents.game();
    let step = ents.timer.time_step;
    let gravity_effect = -0.15 * step;
    let boat = matches!(&b.kind, Kind::Vehicle(v) if v.is_boat());
    let mut below_torso = false;
    let Kind::Ped(ped) = &mut a.kind else { return 0 };
    let phys = a.phys.as_mut().expect("ped physical");
    let mut ip = ColPoint::default();
    if !phys.is_vehicle_being_shifted && !phys.flags.skip_line_col {
        if !a.base.flags.collision_processed {
            if ped.flags.is_standing {
                ped.flags.is_standing = false;
                ped.flags.was_standing = true;
            }
            a.base.flags.collision_processed = true;
            ped.collision_speed += Vec2::new(phys.move_speed.x, phys.move_speed.y).length() * step;
            ped.flags.still_on_valid_poly = false;
            if ped.is_player() || ped.collision_speed >= 1.0 && (ped.collision_speed >= 2.0 || ped.ped_state != state::WANDER_PATH) {
                ped.coll_poly.valid = false;
                ped.collision_speed = 0.0;
                ped.flags.hit_steep_slope = false;
            } else {
                let mut pos = a.base.position();
                let mut potential_ground_z = pos.z - FEET_OFFSET;
                if ped.flags.was_standing {
                    pos.z -= 0.25;
                    potential_ground_z += gravity_effect;
                }
                if collision::is_stored_poly_still_valid_vertical_line(pos, potential_ground_z, &mut ip, &mut ped.coll_poly) {
                    ped.flags.still_on_valid_poly = true;
                    a.base.matrix.pos.z = FEET_OFFSET + ip.point.z;
                    phys.move_speed.z = 0.0;
                    ped.flags.is_standing = true;
                } else {
                    ped.coll_poly.valid = false;
                    ped.collision_speed = 0.0;
                    ped.flags.hit_steep_slope = false;
                }
            }
        }
        if !ped.flags.still_on_valid_poly {
            let pos = a.base.position();
            let mut centre = Vec3::new(pos.x, pos.y, pos.z - 0.52);
            // 0.52 is about a ped's radius
            let mut total_radius = b.base.bound_radius(&cb) + 0.52 - gravity_effect;
            if ped.flags.was_standing {
                if boat {
                    centre.z += 2.0 * gravity_effect;
                    total_radius += gravity_effect.abs();
                } else {
                    centre.z += gravity_effect;
                }
            }
            if total_radius * total_radius > (centre - b.base.bound_centre(&cb)).length_squared() {
                let mut line = Line { p0: pos, p1: Vec3::new(pos.x, pos.y, pos.z - FEET_OFFSET) };
                if ped.flags.was_standing {
                    line.p1.z += gravity_effect;
                    line.p0.z -= 0.25;
                }
                let mut min_dist = 1.0;
                let filter = LineFilter::default();
                below_torso = collision::process_vertical_line(game, &line, &b.base.matrix, &cb, &mut ip, &mut min_dist, filter, Some(&mut ped.coll_poly));
                if boat && ped.flags.was_standing && !below_torso {
                    line.p0.z = line.p1.z;
                    line.p1.z += gravity_effect;
                    below_torso = collision::process_vertical_line(game, &line, &b.base.matrix, &cb, &mut ip, &mut min_dist, filter, Some(&mut ped.coll_poly));
                }
                if below_torso {
                    let z = a.base.position().z;
                    if !ped.flags.is_standing || FEET_OFFSET + ip.point.z > z || boat && 3.12 + ip.point.z > z {
                        if !matches!(b.base.ty, EntityType::Vehicle | EntityType::Object) {
                            ped.cur_surface = Some(rb);
                            ped.flags.trying_to_reach_dry_land = false;
                            ped.flags.on_boat = false;
                        } else {
                            ped.current_phys_surface = Some(rb);
                            ped.offset_from_phys_surface = ip.point - b.base.position();
                            ped.cur_surface = Some(rb);
                            ped.coll_poly.valid = false;
                            ped.flags.on_boat = boat;
                        }
                        a.base.matrix.pos.z = FEET_OFFSET + ip.point.z;
                        phys.surface_touched = ip.surface_b;
                        if phys.surface_touched == surface::STEEP_CLIFF {
                            ped.flags.hit_steep_slope = true;
                            phys.damage_normal = ip.normal;
                        }
                    }
                    let speed = phys.move_speed;
                    let falling = ped.clump.as_ref().is_some_and(|c| c.anim.association(anim::STD_FALL as i32).is_some());
                    let mut speed_sqr = 0.0;
                    if !ped.flags.was_standing && (speed.z < -0.25 || { speed_sqr = speed.length_squared(); speed_sqr > 0.25 }) {
                        if speed_sqr == 0.0 {
                            speed_sqr = speed.z * speed.z;
                        }
                        let mut direction = 2; // from backward
                        if speed.x > 0.01 || speed.x < -0.01 || speed.y > 0.01 || speed.y < -0.01 {
                            direction = local_direction(ped.rotation_cur, Vec2::new(-speed.x, -speed.y));
                        }
                        ents.events.push(WorldEvent::PedFall { ped: ra, by: rb, damage: 350.0 * speed_sqr * speed_sqr, direction: direction as u8 });
                    } else if !ped.flags.was_standing && falling && -0.016 * step > speed.z {
                        ents.events.push(WorldEvent::PedFall { ped: ra, by: rb, damage: 15.0, direction: 2 });
                    }
                    phys.move_speed.z = 0.0;
                    ped.flags.is_standing = true;
                } else {
                    ped.flags.on_boat = false;
                }
            }
        }
    }
    let n = collision::process_col_models(&a.base.matrix, &ca, &b.base.matrix, &cb, points, &mut [], &mut []);
    let building = b.base.ty == EntityType::Building;
    if n > 0 || below_torso {
        let t = ents.timer.time_in_ms;
        add_collision_record(a, b, rb, t);
        if !building {
            add_collision_record(b, a, ra, t);
        }
        if n > 0 && (building || b.base.flags.is_static) {
            a.base.flags.has_hit_wall = true;
        }
    }
    if (building || b.base.flags.is_static) && matches!(&a.kind, Kind::Ped(p) if p.flags.was_standing) {
        for p in &mut points[..n] {
            let mut normal = p.normal;
            let length = Vec2::new(normal.x, normal.y).length();
            if length != 0.0 {
                normal.x /= length;
                normal.y /= length;
            }
            p.normal = normalise(normal);
            if p.surface_b == surface::STEEP_CLIFF
                && let Kind::Ped(ped) = &mut a.kind
            {
                ped.flags.hit_steep_slope = true;
            }
        }
    }
    n
}
