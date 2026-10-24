//! Collision response between two physicals (`CPhysical::ApplyCollision`, `ApplyCollisionAlt`,
//! `ApplyFriction`). GTA III behaviour; Vice City's versions differ and are not ported yet.
//! Glass, particles and sparks come back as events.
use super::object::{ObjectEvent, ObjectTable};
use super::physical::{Physical, GRAVITY};
use super::{status, Ent, EntityRef, EntityType, Kind};
use crate::collision::ColPoint;
use crate::general::Rng;
use crate::math::{dot, magnitude};
use crate::modelinfo::{Indices, ModelInfoStore};
use crate::surface;
use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionEvent {
    /// `CGlass::WindowRespondsToCollision` (hard) or `WindowRespondsToSoftCollision`.
    Window { entity: EntityRef, impulse: f32, speed: Vec3, point: Vec3, soft: bool },
    /// `POBJECT_FIRE_HYDRANT` at this position.
    FireHydrant { pos: Vec3 },
    /// `PARTICLE_SPARK_SMALL`.
    Spark { pos: Vec3, dir: Vec3 },
    Object(ObjectEvent),
}

/// Everything the response needs besides the two entities.
pub struct Ctx<'a> {
    pub step: f32,
    /// `CWorld::bNoMoreCollisionTorque`.
    pub no_more_collision_torque: bool,
    pub indices: &'a Indices,
    pub models: &'a ModelInfoStore,
    pub objects: &'a mut ObjectTable,
    pub rng: &'a mut Rng,
    pub events: Vec<CollisionEvent>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Response {
    pub applied: bool,
    pub impulse_a: f32,
    pub impulse_b: f32,
    /// B turned non-static and must join the moving list (`AddToMovingList`).
    pub b_to_moving_list: bool,
}

fn phys(e: &mut Ent) -> &mut Physical {
    e.phys.as_mut().expect("collision response needs a physical")
}

fn turn_force(e: &mut Ent, j: Vec3, p: Vec3) {
    let m = e.base.matrix;
    phys(e).apply_turn_force(&m, j, p);
}

fn friction_turn_force(e: &mut Ent, j: Vec3, p: Vec3) {
    let m = e.base.matrix;
    phys(e).apply_friction_turn_force(&m, j, p);
}

fn is_player_ped(e: &Ent) -> bool {
    matches!(&e.kind, Kind::Ped(p) if p.is_player)
}

fn stands_on(e: &Ent, other: EntityRef) -> bool {
    matches!(&e.kind, Kind::Ped(p) if p.current_phys_surface == Some(other))
}

/// `CPhysical::ApplyCollision(B, ...)`: A is the entity that hit B at `c`.
pub fn apply_collision(a: &mut Ent, ref_a: EntityRef, b: &mut Ent, ref_b: EntityRef, c: &ColPoint, cx: &mut Ctx) -> Response {
    let n = c.normal;
    let (a_ped, b_ped) = (a.base.flags.ped_physics, b.base.flags.ped_physics);
    let (mut contact_a, mut contact_b) = (false, false);
    let mut r = Response::default();

    let mass_factor_a = if b_ped {
        contact_a = stands_on(b, ref_a);
        10.0
    } else if phys(a).flags.is_heavy {
        2.0
    } else {
        1.0
    };
    let mass_factor_b = if a_ped {
        contact_b = stands_on(a, ref_b);
        let abandoned = b.base.status == status::ABANDONED || b.base.status == status::WRECKED || a.base.flags.has_hit_wall;
        if is_player_ped(a) && b.base.ty == EntityType::Vehicle && abandoned {
            2200.0 / phys(b).mass
        } else {
            10.0
        }
    } else if phys(b).flags.is_heavy {
        2.0
    } else {
        1.0
    };

    if b.base.flags.is_static {
        if a_ped {
            let speed_a = dot(phys(a).move_speed, n);
            if speed_a < 0.0 {
                if b.base.ty == EntityType::Object {
                    r.impulse_a = -speed_a * phys(a).mass;
                    r.impulse_b = r.impulse_a;
                    let uproot = if let Kind::Object(o) = &b.kind { o.uproot_limit } else { 0.0 };
                    let glass = cx.indices.is_glass(cx.models, b.base.model_index);
                    if r.impulse_a > uproot {
                        if glass {
                            cx.events.push(CollisionEvent::Window { entity: ref_b, impulse: r.impulse_a, speed: phys(a).move_speed, point: c.point, soft: false });
                        } else if !phys(b).flags.infinite_mass {
                            b.base.flags.is_static = false;
                        }
                    } else {
                        if glass {
                            cx.events.push(CollisionEvent::Window { entity: ref_b, impulse: r.impulse_a, speed: Vec3::ZERO, point: c.point, soft: true });
                        }
                        if !phys(a).flags.infinite_mass {
                            let e = phys(a).elasticity;
                            phys(a).apply_move_force(n * (1.0 + e) * r.impulse_a);
                        }
                        r.applied = true;
                        return r;
                    }
                } else if !phys(b).flags.infinite_mass {
                    b.base.flags.is_static = false;
                }
                if phys(b).flags.infinite_mass {
                    r.impulse_a = -speed_a * phys(a).mass;
                    r.impulse_b = 0.0;
                    if !phys(a).flags.infinite_mass {
                        let e = phys(a).elasticity;
                        phys(a).apply_move_force(n * (1.0 + e) * r.impulse_a);
                    }
                    r.applied = true;
                    return r;
                }
            }
        } else {
            let point_a = c.point - a.base.position();
            let speed_a = dot(phys(a).speed_at(point_a), n);
            if speed_a < 0.0 {
                if b.base.ty == EntityType::Object {
                    let e_a = if a.base.flags.has_hit_wall { -1.0 } else { -(1.0 + phys(a).elasticity) };
                    r.impulse_a = e_a * speed_a * phys(a).mass_at(point_a, n);
                    r.impulse_b = r.impulse_a;

                    let (effect, uproot) = match &b.kind {
                        Kind::Object(o) => (o.collision_damage_effect, o.uproot_limit),
                        _ => (0, 0.0),
                    };
                    if effect != 0 && r.impulse_a > 20.0 {
                        let ev = cx.objects.object_damage(b, r.impulse_a, cx.indices.body_cast(), cx.rng);
                        cx.events.extend(ev.into_iter().map(CollisionEvent::Object));
                        if !b.base.flags.uses_collision {
                            if !phys(a).flags.infinite_mass {
                                let f = n * 0.2 * r.impulse_a;
                                phys(a).apply_move_force(f);
                                turn_force(a, f, point_a);
                            }
                            return r;
                        }
                    }

                    let glass = cx.indices.is_glass(cx.models, b.base.model_index);
                    if (r.impulse_a > uproot || a.base.flags.is_stuck) && !phys(b).flags.infinite_mass {
                        if glass {
                            cx.events.push(CollisionEvent::Window { entity: ref_b, impulse: r.impulse_a, speed: phys(a).move_speed, point: c.point, soft: false });
                        } else {
                            b.base.flags.is_static = false;
                        }
                        let model = b.base.model_index;
                        let damaged = matches!(&b.kind, Kind::Object(o) if o.has_been_damaged);
                        if model == cx.indices.fire_hydrant() && !damaged {
                            cx.events.push(CollisionEvent::FireHydrant { pos: b.base.position() - Vec3::new(0.0, 0.0, 0.5) });
                            if let Kind::Object(o) = &mut b.kind {
                                o.has_been_damaged = true;
                            }
                        } else if !cx.indices.is_explosive_thing(model)
                            && let Kind::Object(o) = &mut b.kind
                        {
                            o.has_been_damaged = true;
                        }
                    } else {
                        if glass {
                            cx.events.push(CollisionEvent::Window { entity: ref_b, impulse: r.impulse_a, speed: Vec3::ZERO, point: c.point, soft: true });
                        }
                        let mut f = n * r.impulse_a;
                        if a.base.ty == EntityType::Vehicle && n.z < 0.7 {
                            f.z *= 0.3;
                        }
                        if !phys(a).flags.infinite_mass {
                            phys(a).apply_move_force(f);
                            if a.base.ty != EntityType::Vehicle || !cx.no_more_collision_torque {
                                turn_force(a, f, point_a);
                            }
                        }
                        r.applied = true;
                        return r;
                    }
                } else if !phys(b).flags.infinite_mass {
                    b.base.flags.is_static = false;
                }
            }
        }

        if b.base.flags.is_static {
            return r;
        }
        if !phys(b).flags.infinite_mass {
            r.b_to_moving_list = true;
        }
    }

    // B is not static
    let (el_a, el_b) = (phys(a).elasticity, phys(b).elasticity);
    let avg_e = (el_a + el_b) / 2.0;
    if a_ped && b_ped {
        let speed_a = dot(phys(a).move_speed, n);
        let speed_sum = dot(phys(b).move_speed, n).max(0.0);
        if speed_a < speed_sum {
            let e_a = if a.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_a - speed_sum) * avg_e };
            r.impulse_a = (e_a - speed_a) * phys(a).mass * mass_factor_a;
            if !phys(a).flags.infinite_mass {
                phys(a).apply_move_force(n * (r.impulse_a / mass_factor_a));
            }
            r.applied = true;
            return r;
        }
    } else if a_ped {
        let point_b = c.point - b.base.position();
        let speed_a = dot(phys(a).move_speed, n);
        let speed_b = dot(phys(b).speed_at(point_b), n);
        let m_a = phys(a).mass * mass_factor_a;
        let m_b = phys(b).mass_tweak(point_b, n, mass_factor_b);
        let speed_sum = (m_b * speed_b + m_a * speed_a) / (m_a + m_b);
        if speed_a < speed_sum {
            let e_a = if a.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_a - speed_sum) * avg_e };
            let e_b = if b.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_b - speed_sum) * avg_e };
            r.impulse_a = (e_a - speed_a) * m_a;
            r.impulse_b = -(e_b - speed_b) * m_b;
            let mut f_a = n * (r.impulse_a / mass_factor_a);
            let f_b = n * (-r.impulse_b / mass_factor_b);
            if !phys(a).flags.infinite_mass {
                f_a.z = f_a.z.max(0.0);
                if contact_b {
                    f_a.x *= 2.0;
                    f_a.y *= 2.0;
                }
                phys(a).apply_move_force(f_a);
            }
            if !phys(b).flags.infinite_mass && !contact_b {
                phys(b).apply_move_force(f_b);
                turn_force(b, f_b, point_b);
            }
            r.applied = true;
            return r;
        }
    } else if b_ped {
        let point_a = c.point - a.base.position();
        let speed_a = dot(phys(a).speed_at(point_a), n);
        let speed_b = dot(phys(b).move_speed, n);
        let m_a = phys(a).mass_tweak(point_a, n, mass_factor_a);
        let m_b = phys(b).mass * mass_factor_b;
        let speed_sum = (m_b * speed_b + m_a * speed_a) / (m_a + m_b);
        if speed_a < speed_sum {
            let e_a = if a.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_a - speed_sum) * avg_e };
            let e_b = if b.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_b - speed_sum) * avg_e };
            r.impulse_a = (e_a - speed_a) * m_a;
            r.impulse_b = -(e_b - speed_b) * m_b;
            let f_a = n * (r.impulse_a / mass_factor_a);
            let mut f_b = n * (-r.impulse_b / mass_factor_b);
            if !phys(a).flags.infinite_mass && !contact_a {
                let mut f = f_a;
                f.z = f.z.max(0.0);
                phys(a).apply_move_force(f);
                turn_force(a, f, point_a);
            }
            if !phys(b).flags.infinite_mass {
                if f_b.z < 0.0 {
                    f_b.z = 0.0;
                    if speed_a.abs() < 0.01 {
                        f_b *= 0.5;
                    }
                }
                if contact_a {
                    f_b.x *= 2.0;
                    f_b.y *= 2.0;
                }
                phys(b).apply_move_force(f_b);
            }
            r.applied = true;
            return r;
        }
    } else {
        let mut point_a = c.point - a.base.position();
        let mut point_b = c.point - b.base.position();
        let speed_a = dot(phys(a).speed_at(point_a), n);
        let speed_b = dot(phys(b).speed_at(point_b), n);
        let m_a = phys(a).mass_tweak(point_a, n, mass_factor_a);
        let m_b = phys(b).mass_tweak(point_b, n, mass_factor_b);
        let speed_sum = (m_b * speed_b + m_a * speed_a) / (m_a + m_b);
        if speed_a < speed_sum {
            let e_a = if a.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_a - speed_sum) * avg_e };
            let e_b = if b.base.flags.has_hit_wall { speed_sum } else { speed_sum - (speed_b - speed_sum) * avg_e };
            r.impulse_a = (e_a - speed_a) * m_a;
            r.impulse_b = -(e_b - speed_b) * m_b;
            let mut f_a = n * (r.impulse_a / mass_factor_a);
            let mut f_b = n * (-r.impulse_b / mass_factor_b);
            if a.base.ty == EntityType::Vehicle && !a.base.flags.has_hit_wall {
                f_a.x *= 1.4;
                f_a.y *= 1.4;
                if n.z < 0.7 {
                    f_a.z *= 0.3;
                }
                if a.base.status == status::PLAYER {
                    point_a *= 0.8;
                }
                if cx.no_more_collision_torque {
                    phys(a).apply_friction_move_force(f_a * -0.3);
                    friction_turn_force(a, f_a * -0.3, point_a);
                }
            }
            if b.base.ty == EntityType::Vehicle && !b.base.flags.has_hit_wall {
                f_b.x *= 1.4;
                f_b.y *= 1.4;
                if -n.z < 0.7 {
                    f_b.z *= 0.3;
                }
                if b.base.status == status::PLAYER {
                    point_b *= 0.8;
                }
                if cx.no_more_collision_torque {
                    phys(b).apply_friction_move_force(f_b * -0.3);
                    friction_turn_force(b, f_b * -0.3, point_b);
                }
            }
            if !phys(a).flags.infinite_mass {
                phys(a).apply_move_force(f_a);
                turn_force(a, f_a, point_a);
            }
            if !phys(b).flags.infinite_mass {
                if b.base.flags.is_in_safe_position {
                    super::physical::unset_is_in_safe_position(&mut b.base, b.phys.as_mut().unwrap(), cx.step);
                }
                phys(b).apply_move_force(f_b);
                turn_force(b, f_b, point_b);
            }
            r.applied = true;
            return r;
        }
    }
    r
}

/// `CPhysical::ApplyCollisionAlt`: bounce off something that does not move (a building or an
/// infinite-mass physical, `b_solid`). The velocity change is added to `move_speed`/`turn_speed`
/// instead of the entity so the caller can apply it after all contacts are summed.
pub fn apply_collision_alt(a: &mut Ent, b_solid: bool, c: &ColPoint, step: f32, impulse: &mut f32, move_speed: &mut Vec3, turn_speed: &mut Vec3) -> bool {
    let n = c.normal;
    if a.base.flags.ped_physics {
        let p = phys(a);
        let normal_speed = dot(p.move_speed, n);
        if normal_speed < 0.0 {
            *impulse = -normal_speed * p.mass;
            // unlike the others, a ped is pushed out directly
            p.apply_move_force(n * *impulse);
            return true;
        }
        return false;
    }
    let p = a.phys.as_ref().expect("collision response needs a physical");
    let point = c.point - a.base.position();
    let normal_speed = dot(p.speed_at(point), n);
    if normal_speed >= 0.0 {
        return false;
    }
    let min_speed = 1.3 * GRAVITY * step;
    let upside_down_vehicle = match &a.kind {
        Kind::Vehicle(v) => a.base.matrix.up.z < -0.3 || (v.is_bike() && (a.base.status == status::ABANDONED || a.base.status == status::WRECKED)),
        _ => false,
    };
    let e = if (a.base.ty == EntityType::Object || upside_down_vehicle)
        && !a.base.flags.has_contacted
        && p.move_speed.x.abs() < min_speed
        && p.move_speed.y.abs() < min_speed
        && p.move_speed.z.abs() < min_speed * 2.0
    {
        -1.0
    } else {
        -(p.elasticity + 1.0)
    };
    *impulse = normal_speed * e * p.mass_at(point, n);

    let v_impulse = n * *impulse;
    let boost = a.base.ty == EntityType::Vehicle && (!a.base.flags.has_hit_wall || !(crate::math::magnitude_squared(p.move_speed) as f64 > 0.1 || !b_solid));
    *move_speed += if boost { v_impulse * 1.2 * (1.0 / p.mass) } else { v_impulse * (1.0 / p.mass) };
    let com = a.base.matrix.rotate_vector(p.centre_of_mass);
    *turn_speed += crate::math::cross(point - com, v_impulse) * (1.0 / p.turn_mass);
    true
}

/// Other-than-normal velocity and its direction; the original divides by zero here, `FIX_BUGS` normalises instead.
fn tangential(speed: Vec3, n: Vec3) -> (Vec3, f32, Vec3) {
    let other = speed - n * dot(speed, n);
    let dir = if other == Vec3::ZERO { Vec3::ZERO } else { crate::math::normalise(other) };
    (other, magnitude(other), dir)
}

/// `CPhysical::ApplyFriction(B, adhesiveLimit, colpoint)`: friction between two moving physicals.
pub fn apply_friction_pair(a: &mut Ent, b: &mut Ent, adhesive_limit: f32, c: &ColPoint, step: f32) -> bool {
    let n = c.normal;
    let (a_ped, b_ped) = (a.base.flags.ped_physics, b.base.flags.ped_physics);
    let limit = adhesive_limit * step;
    if a_ped && b_ped {
        let (_, f_a, dir) = tangential(phys(a).move_speed, n);
        let (_, f_b, _) = tangential(phys(b).move_speed, n);
        let (m_a, m_b) = (phys(a).mass, phys(b).mass);
        let sum = (m_b * f_b + m_a * f_a) / (m_b + m_a);
        if f_a > sum {
            let i_a = ((sum - f_a) * m_a).max(-limit);
            let i_b = ((sum - f_b) * m_b).min(limit);
            phys(a).apply_friction_move_force(dir * i_a);
            phys(b).apply_friction_move_force(dir * i_b);
            return true;
        }
    } else if a_ped {
        if b.base.ty == EntityType::Vehicle {
            return false;
        }
        let point_b = c.point - b.base.position();
        let speed_b = phys(b).speed_at(point_b);
        let (_, f_a, dir) = tangential(phys(a).move_speed, n);
        let (_, f_b, _) = tangential(speed_b, n);
        let m_b = phys(b).mass_at(point_b, dir);
        let m_a = phys(a).mass;
        let sum = (m_b * f_b + m_a * f_a) / (m_b + m_a);
        if f_a > sum {
            let i_a = ((sum - f_a) * m_a).max(-limit);
            let i_b = ((sum - f_b) * m_b).min(limit);
            phys(a).apply_friction_move_force(dir * i_a);
            phys(b).apply_friction_move_force(dir * i_b);
            friction_turn_force(b, dir * i_b, point_b);
            return true;
        }
    } else if b_ped {
        if a.base.ty == EntityType::Vehicle {
            return false;
        }
        let point_a = c.point - a.base.position();
        let speed_a = phys(a).speed_at(point_a);
        let (_, f_a, dir) = tangential(speed_a, n);
        let (_, f_b, _) = tangential(phys(b).move_speed, n);
        let m_a = phys(a).mass_at(point_a, dir);
        let m_b = phys(b).mass;
        let sum = (m_b * f_b + m_a * f_a) / (m_b + m_a);
        if f_a > sum {
            let i_a = ((sum - f_a) * m_a).max(-limit);
            let i_b = ((sum - f_b) * m_b).min(limit);
            phys(a).apply_friction_move_force(dir * i_a);
            friction_turn_force(a, dir * i_a, point_a);
            phys(b).apply_friction_move_force(dir * i_b);
            return true;
        }
    } else {
        let point_a = c.point - a.base.position();
        let point_b = c.point - b.base.position();
        let (_, f_a, dir) = tangential(phys(a).speed_at(point_a), n);
        let (_, f_b, _) = tangential(phys(b).speed_at(point_b), n);
        let m_a = phys(a).mass_at(point_a, dir);
        let m_b = phys(b).mass_at(point_b, dir);
        let sum = (m_b * f_b + m_a * f_a) / (m_b + m_a);
        if f_a > sum {
            let i_a = ((sum - f_a) * m_a).max(-limit);
            let i_b = ((sum - f_b) * m_b).min(limit);
            phys(a).apply_friction_move_force(dir * i_a);
            friction_turn_force(a, dir * i_a, point_a);
            phys(b).apply_friction_move_force(dir * i_b);
            friction_turn_force(b, dir * i_b, point_b);
            return true;
        }
    }
    false
}

/// `CPhysical::ApplyFriction(adhesiveLimit, colpoint)`: friction against the world.
pub fn apply_friction(a: &mut Ent, adhesive_limit: f32, c: &ColPoint, step: f32, game: re3_formats::datafile::Game, events: &mut Vec<CollisionEvent>) -> bool {
    let n = c.normal;
    if a.base.flags.ped_physics {
        let (_, f, dir) = tangential(phys(a).move_speed, n);
        if f > 0.0 {
            // not really an impulse but a speed
            let limit = adhesive_limit * step / phys(a).mass;
            let v = dir * (-f).max(-limit);
            phys(a).move_friction += Vec3::new(v.x, v.y, 0.0);
            return true;
        }
        return false;
    }
    let point = c.point - a.base.position();
    let (_, f, dir) = tangential(phys(a).speed_at(point), n);
    if f > 0.0 {
        // the 1.5 is a double in the original
        let limit = (adhesive_limit * step) as f64 * 1.5;
        let mut impulse = -f * phys(a).mass;
        if (impulse as f64) < -limit {
            impulse = (-limit) as f32;
        }
        phys(a).apply_friction_move_force(dir * impulse);
        friction_turn_force(a, dir * impulse, point);
        if f > 0.1 && c.surface_b != surface::GRASS && c.surface_b != surface::MUD_DRY && surface::adhesion_group(game, c.surface_a) == surface::ADHESIVE_HARD {
            let v = dir * f * 0.25;
            events.extend((0..4).map(|_| CollisionEvent::Spark { pos: c.point, dir: v }));
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests;
