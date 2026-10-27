//! Boats in the world: construction and `CBoat::ProcessControl`.
use super::World;
use crate::entity::physical::GRAVITY;
use crate::entity::{status, Ent, Entity, EntityRef, EntityType, Kind};
use crate::general::Rng;
use crate::math::{dot, magnitude, magnitude_squared};
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::{VehicleData, VehicleSub, VehicleEvent};
use crate::vehicle::boat::{self, BoatData, INVALID_ORIENTATION};
use crate::vehicle::env::{CarEnv, CarEvent, CarHooks, CarSound, ExplosionKind, Particle, ParticleKind};
use crate::vehicle::floater::BoatModel;
use glam::Vec3;

fn boat_model(model: i16) -> BoatModel {
    match model {
        mi::PREDATOR => BoatModel::Predator,
        mi::SPEEDER => BoatModel::Speeder,
        mi::REEFER => BoatModel::Reefer,
        _ => BoatModel::Other,
    }
}

/// Light-scaled particle colour, as `ProcessControl` computes it from the time cycle.
fn light_colour(env: &CarEnv, scale: f32) -> [u8; 3] {
    let f = |i: usize| ((scale * (env.ambient[i] + 0.5 * env.directional[i])) as i32).clamp(0, 255) as u8;
    [f(0), f(1), f(2)]
}

impl World {
    /// `CBoat::CBoat`; place it with [`World::add`].
    pub fn new_boat(&mut self, model_index: i16, created_by: u8, rng: &mut Rng) -> Option<EntityRef> {
        let time_ms = self.ents.timer.time_in_ms;
        let info = self.ents.models.get_mut(model_index as i32)?;
        let crate::modelinfo::Kind::Vehicle(mi_data) = &mut info.kind else { return None };
        let handling_id = mi_data.handling_id as usize;
        let h = *self.ents.handling.get(handling_id);
        let nb = boat::new_boat(model_index, created_by, time_ms, handling_id, &h, mi_data, rng);
        let mut base = Entity::new(EntityType::Vehicle, rng);
        base.model_index = model_index;
        base.status = status::ABANDONED;
        base.flags.touching_water = true;
        self.insert(Ent::new(base, Some(nb.physical), Kind::Vehicle(Box::new(nb.vehicle))))
    }

    /// `CBoat::Teleport`.
    pub fn teleport_boat(&mut self, r: EntityRef, pos: Vec3) {
        self.remove(r);
        if let Some(e) = self.get_mut(r) {
            e.base.set_position(pos);
            e.base.set_orientation(0.0, 0.0, 0.0);
            if let Some(p) = e.phys.as_mut() {
                p.move_speed = Vec3::ZERO;
                p.turn_speed = Vec3::ZERO;
            }
        }
        let _ = self.add(r);
    }

    /// `CBoat::BlowUpCar`.
    pub fn blow_up_boat(&mut self, r: EntityRef, culprit: Option<EntityRef>, env: &mut CarEnv, rng: &mut Rng) {
        let Some(e) = self.get_mut(r) else { return };
        let Kind::Vehicle(v) = &mut e.kind else { return };
        if !v.flags.can_be_damaged {
            return;
        }
        let p = e.phys.as_mut().unwrap();
        // the explosion pushes the vehicle up
        p.move_speed.z += 0.13;
        e.base.status = status::WRECKED;
        e.base.flags.render_scorched = true;
        v.health = 0.0;
        v.bomb_timer = 0;
        let pos = e.base.position();
        let moving_node = match &v.sub {
            VehicleSub::Boat(b) => b.moving_node,
            _ => None,
        };
        env.events.push(CarEvent::CamShake { amount: 0.7, pos });
        if env.player.vehicle == Some(r) {
            env.events.push(CarEvent::Vehicle(VehicleEvent::KillPlayer));
        }
        env.events.push(CarEvent::KillOccupants);
        v.flags.engine_on = false;
        v.flags.lights_on = false;
        let law_enforcer = v.change_law_enforcer_state(false).is_some();
        env.events.push(CarEvent::DutyEnded { ambulance: false, firetruck: false, law_enforcer });
        env.events.push(CarEvent::Explosion { kind: ExplosionKind::Car, pos, culprit });

        // the propeller comes loose, as a flying part
        let Some(node_pos) = moving_node else { return };
        let (m, move_speed, turn_speed) = (e.base.matrix, p.move_speed, p.turn_speed);
        let model = e.base.model_index;
        let mut o = Entity::new(EntityType::Object, rng);
        o.model_index = mi::CAR_WHEEL;
        o.matrix = m;
        o.matrix.pos = m.transform_point(node_pos);
        let mut phys = crate::entity::physical::Physical { mass: 10.0, turn_mass: 25.0, air_resistance: 0.99, elasticity: 0.1, buoyancy: 10.0 * GRAVITY / 0.75, ..Default::default() };
        let mut data = crate::entity::ObjectData::new();
        data.created_by = crate::entity::object::CreatedBy::Temp;
        data.end_of_life_time = self.ents.timer.time_in_ms + 20000;
        phys.move_speed = move_speed;
        phys.move_speed.z = if m.up.z > 0.0 { 0.3 } else { 0.0 };
        phys.turn_speed = turn_speed * 2.0;
        phys.turn_speed.x = 0.5;
        let mut dist = crate::math::normalise(o.matrix.pos - m.pos);
        if m.up.z > 0.0 {
            dist += m.up;
        }
        o.matrix.pos += dist;
        o.flags.is_static = false;
        data.ref_model_index = model;
        self.ents.models.add_ref(model as i32);
        if let Some(or) = self.insert(Ent::new(o, Some(phys), Kind::Object(Box::new(data).as_ref().clone()))) {
            let _ = self.add(or);
        }
    }

    /// `CBoat::ProcessControl`.
    pub fn process_control_boat(&mut self, r: EntityRef, env: &mut CarEnv, hooks: &mut dyn CarHooks, rng: &mut Rng) {
        let Some(e0) = self.get(r) else { return };
        let Kind::Vehicle(v0) = &e0.kind else { return };
        if !matches!(v0.sub, VehicleSub::Boat(_)) {
            return;
        }
        let model = e0.base.model_index;
        let h = *self.ents.handling.get(v0.handling);
        let timer = self.ents.timer.clone();
        let step = timer.time_step;
        let Some(col) = self.ents.col_model(model) else { return };
        let me_is_player_vehicle = env.player.vehicle == Some(r);

        let Some(mut e) = self.ents.pools.take(r) else { return };
        let mut events = std::mem::take(&mut env.events);
        if e.base.status == status::PHYSICS {
            hooks.steer_ai_boat_with_physics(&mut e);
        }
        let mut blow_up: Option<Option<EntityRef>> = None;
        // the boat data is moved out of the vehicle while it is worked on
        let mut boat_data = {
            let Kind::Vehicle(v) = &mut e.kind else { return };
            match std::mem::take(&mut v.sub) {
                VehicleSub::Boat(b) => b,
                other => {
                    v.sub = other;
                    return;
                }
            }
        };
        'body: {
            let ent_status = e.base.status;
            let Ent { base, phys, kind, .. } = &mut e;
            let phys = phys.as_mut().unwrap();
            let Kind::Vehicle(veh) = kind else { break 'body };
            let b: &mut BoatData = &mut boat_data;
            if phys.zone_level > 0 && phys.zone_level as i32 != self.ents.collision_in_memory {
                break 'body;
            }
            let on_land = phys.damage_impulse > 0.0 && phys.damage_normal.z > 0.1;
            b.prune_wake_trail(step);
            let splash_rgb = light_colour(env, 114.75);
            let splash_alpha = rng.range_i32(128, 150) as u8;
            let jet_rgb = light_colour(env, 242.25);
            let jet_alpha = rng.range_i32(96, 128) as u8;
            let splash_colour = [splash_rgb[0], splash_rgb[1], splash_rgb[2], splash_alpha];
            let jet_colour = [jet_rgb[0], jet_rgb[1], jet_rgb[2], jet_alpha];
            rng.number(); // unused

            veh.process_car_alarm(&timer);

            match ent_status {
                status::PLAYER => {
                    b.is_anchored = false;
                    b.orientation = INVALID_ORIENTATION;
                    b.process_control_inputs(veh, &h, &env.pad);
                    // the Predator's fixed machine guns are the weapon code's job
                    if model == mi::PREDATOR {
                        events.push(CarEvent::FixedMachineGuns);
                    }
                }
                status::SIMPLE => {
                    b.is_anchored = false;
                    b.orientation = INVALID_ORIENTATION;
                    crate::entity::physical::process_control(base, phys, step);
                    b.in_water = true;
                    b.propeller_in_water = true;
                    phys.flags.is_in_water = true;
                    break 'body;
                }
                status::PHYSICS => {
                    b.is_anchored = false;
                    b.orientation = INVALID_ORIENTATION;
                }
                status::ABANDONED | status::WRECKED => {
                    b.in_water = true;
                    b.propeller_in_water = true;
                    phys.flags.is_in_water = true;
                    veh.steer_angle = 0.0;
                    veh.flags.is_handbrake_on = false;
                    veh.brake_pedal = 0.5;
                    veh.gas_pedal = 0.0;
                    if magnitude(base.position() - env.player.coors) > 150.0 {
                        phys.move_speed = Vec3::ZERO;
                        phys.turn_speed = Vec3::ZERO;
                        break 'body;
                    }
                }
                _ => {}
            }

            let collision_damage = h.collision_damage_multiplier * phys.damage_impulse;
            if collision_damage > 25.0 && base.status != status::WRECKED && veh.health >= 150.0 && !base.flags.collision_proof {
                let prev = veh.health;
                if me_is_player_vehicle {
                    veh.health -= (collision_damage - 25.0) / if veh.flags.take_less_damage { 6.0 } else { 2.0 };
                } else {
                    if collision_damage > 60.0 && veh.driver.is_some() {
                        events.push(CarEvent::Sound { sound: CarSound::AnnoyedDriver, volume: 0.0 });
                    }
                    veh.health -= (collision_damage - 25.0) / if veh.flags.take_less_damage { 12.0 } else { 4.0 };
                }
                if veh.health <= 0.0 && prev > 0.0 {
                    veh.health = 1.0;
                    b.set_on_fire_entity = phys.damage_entity;
                }
            }

            // damage particles
            let m = base.matrix;
            if veh.health <= 600.0 && base.status != status::WRECKED && (m.pos.x - env.camera_pos.x).abs() < 200.0 && (m.pos.y - env.camera_pos.y).abs() < 200.0 {
                let speed_sq = magnitude_squared(phys.move_speed);
                let mut smoke_dir = 0.8 * phys.move_speed;
                let smoke_pos = match model {
                    mi::SPEEDER => {
                        smoke_dir += 0.05 * m.right;
                        smoke_dir.z += 0.2 * phys.move_speed.z;
                        Vec3::new(0.4, -2.4, 0.8)
                    }
                    mi::REEFER => {
                        smoke_dir += 0.07 * m.right;
                        Vec3::new(2.0, -1.0, 0.5)
                    }
                    _ => {
                        smoke_dir += -0.08 * m.right;
                        Vec3::new(-1.5, -0.5, 1.2)
                    }
                };
                let smoke_pos = m.transform_point(smoke_pos);
                if veh.health < 150.0 {
                    events.push(CarEvent::Particle(Particle::new(ParticleKind::CarFlame, smoke_pos, Vec3::new(0.0, 0.0, rng.range_f32(2.25 / 200.0, 0.09)), 0.9)));
                    let mut p2 = smoke_pos;
                    p2.x += rng.range_f32(-2.25 / 4.0, 2.25 / 4.0);
                    p2.y += rng.range_f32(-2.25 / 4.0, 2.25 / 4.0);
                    p2.z += rng.range_f32(2.25 / 4.0, 2.25);
                    events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSmoke2, p2, Vec3::ZERO, 0.0)));
                    b.damage += timer.time_step_in_milliseconds() as f32;
                    if b.damage > 5000.0 {
                        blow_up = Some(b.set_on_fire_entity);
                    }
                }
                if speed_sq < 0.25 && (timer.frame_counter.wrapping_add(base.random_seed as u32)) & 1 != 0 {
                    events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSteam, smoke_pos, smoke_dir, 0.0)));
                }
                if speed_sq < 0.25 && veh.health <= 350.0 {
                    events.push(CarEvent::Particle(Particle::new(ParticleKind::EngineSmoke, smoke_pos, 1.25 * smoke_dir, 0.0)));
                }
            }
            if blow_up.is_some() {
                break 'body;
            }

            crate::entity::physical::process_control(base, phys, step);

            let is_anchored_after = {
                let m = base.matrix;
                let result = self.ents.buoyancy.process(self.ents.water.as_ref(), &m, &col, Some(boat_model(model)), base.flags.touching_water, h.buoyancy, step);
                let volume = self.ents.buoyancy.volume_under_water();
                if let Some((buoyance_point, buoyance_impulse)) = result {
                    // the boat is in the water
                    if 0.1 * phys.mass * GRAVITY * step < buoyance_impulse.z {
                        b.in_water = true;
                        phys.flags.is_in_water = true;
                    } else {
                        b.in_water = false;
                        phys.flags.is_in_water = false;
                    }
                    b.volume_under_water = volume;
                    b.buoyance_point = buoyance_point;
                    phys.apply_move_force(buoyance_impulse);
                    if !on_land {
                        phys.apply_turn_force(&m, buoyance_impulse, buoyance_point);
                    }
                    if !on_land && b.in_water && m.up.z > 0.0 {
                        let mut impulse = if veh.gas_pedal > 0.05 { magnitude_squared(phys.move_speed) * h.suspension_force_level * buoyance_impulse.z * step * 0.5 * veh.gas_pedal } else { 0.0 };
                        impulse = impulse.min(GRAVITY * h.suspension_damping_level * phys.mass * step);
                        phys.apply_move_force(impulse * m.up);
                        phys.apply_turn_force(&m, impulse * m.up, buoyance_point - h.suspension_bias * m.forward);
                    }

                    // moving forward
                    let flat_speed = (phys.move_speed.x * phys.move_speed.x + phys.move_speed.y * phys.move_speed.y).sqrt();
                    if veh.gas_pedal.abs() > 0.05 || flat_speed > 0.01 {
                        if b.in_water {
                            b.add_wake_point(m.pos, m.pos);
                        }
                        let mut steer_factor = 1.0 - dot(phys.move_speed, m.forward);
                        if model == mi::GHOST {
                            steer_factor = 1.0 - dot(phys.move_speed, m.forward) * 0.3;
                        }
                        steer_factor = steer_factor.max(0.0);
                        let propeller = m.rotate_vector(Vec3::new(0.0, -h.dimension.y * b.thrust_y, -h.dimension.z * b.thrust_z));
                        let mut propeller_world = m.pos + propeller;
                        let steer_sin = (-veh.steer_angle * steer_factor).sin();
                        let steer_cos = (-veh.steer_angle * steer_factor).cos();
                        let water_level = self.ents.water.height(propeller_world.x, propeller_world.y, propeller_world.z, true).unwrap_or(0.0);
                        if propeller_world.z - 0.5 < water_level {
                            let mut depth = water_level - (propeller_world.z - 0.5);
                            if depth > 1.0 {
                                depth = 1.0;
                            } else {
                                depth *= depth;
                            }
                            b.propeller_in_water = true;
                            if veh.gas_pedal.abs() > 0.05 {
                                let force_dir = m.rotate_vector(Vec3::new(-steer_sin, steer_cos, -veh.steer_angle.abs()));
                                let mut force = depth * veh.gas_pedal * 40.0 * h.transmission.engine_acceleration * h.mass * force_dir;
                                if force.z > 0.2 {
                                    force.z = (1.2 - force.z) * (1.2 - force.z) + 0.2;
                                }
                                if on_land {
                                    if veh.gas_pedal < 0.0 {
                                        force.x *= 5.0;
                                        force.y *= 5.0;
                                    }
                                    if force.z < 0.0 {
                                        force.z = 0.0;
                                    }
                                    phys.apply_move_force(force * step);
                                } else {
                                    phys.apply_move_force(force * step);
                                    phys.apply_turn_force(&m, force * step, propeller - h.traction_bias * m.up);
                                    let right_force = dot(m.right, force);
                                    phys.apply_turn_force(&m, -right_force * m.right * step, m.up);
                                }

                                // spray
                                let mut jet_dir = -0.04 * force;
                                if veh.gas_pedal > 0.0 {
                                    if base.status == status::PLAYER {
                                        let camera_hack = env.camera_topdown;
                                        let mut stern = col.bounding_box.min;
                                        stern.x = 0.0;
                                        stern.z = 0.0;
                                        let stern = m.rotate_vector(stern);
                                        let mut jet_pos = m.pos + stern;
                                        jet_pos.z = if camera_hack { 1.0 } else { 0.0 };
                                        let mut wake_pos = m.pos + stern;
                                        wake_pos.z -= 0.65;
                                        let wake_dir = 0.75 * jet_dir;
                                        let mut jet = Particle::new(ParticleKind::BoatThrustJet, jet_pos, jet_dir, 0.0);
                                        jet.colour = Some(jet_colour);
                                        events.push(CarEvent::Particle(jet));
                                        let mut sp = Particle::new(ParticleKind::CarSplash, jet_pos, 0.25 * jet_dir, 1.0);
                                        sp.colour = Some(splash_colour);
                                        sp.rotation_speed = rng.range_i32(0, 30) as f32;
                                        sp.rotation = rng.range_i32(0, 90) as f32;
                                        sp.frame = 3;
                                        events.push(CarEvent::Particle(sp));
                                        if !camera_hack {
                                            let mut wk = Particle::new(ParticleKind::BoatWake, wake_pos, wake_dir, 0.0);
                                            wk.colour = Some(jet_colour);
                                            events.push(CarEvent::Particle(wk));
                                        }
                                    } else if (timer.frame_counter.wrapping_add(base.random_seed as u32)) & 1 != 0 {
                                        jet_dir.z = 0.018;
                                        jet_dir.x *= 0.01;
                                        jet_dir.y *= 0.01;
                                        propeller_world.z += 1.5;
                                        let mut sp = Particle::new(ParticleKind::BoatSplash, propeller_world, jet_dir, 1.5);
                                        sp.colour = Some(jet_colour);
                                        events.push(CarEvent::Particle(sp));
                                        let mut cs = Particle::new(ParticleKind::CarSplash, propeller_world, 0.1 * jet_dir, 0.5);
                                        cs.colour = Some(splash_colour);
                                        cs.rotation_speed = rng.range_i32(0, 30) as f32;
                                        cs.rotation = rng.range_i32(0, 90) as f32;
                                        cs.frame = 3;
                                        events.push(CarEvent::Particle(cs));
                                    }
                                }
                            } else if !on_land {
                                let force = (50.0 * dot(phys.move_speed, m.forward)).min(10.0);
                                let propeller_force = depth * m.rotate_vector(force * Vec3::new(-steer_sin, 0.0, 0.0));
                                phys.apply_move_force(propeller_force * step * 0.5);
                                phys.apply_turn_force(&m, propeller_force * step * 0.5, propeller);
                            }
                        } else {
                            b.propeller_in_water = false;
                        }
                    }

                    // slow down or push down the boat at the world limits
                    phys.move_speed.x = phys.move_speed.x.min(-(m.pos.x - 1900.0) * 0.01);
                    phys.move_speed.x = phys.move_speed.x.max(-(m.pos.x - -1515.0) * 0.01);
                    phys.move_speed.y = phys.move_speed.y.min(-(m.pos.y - 600.0) * 0.01);
                    phys.move_speed.y = phys.move_speed.y.max(-(m.pos.y - -1900.0) * 0.01);

                    if !on_land && b.in_water {
                        b.apply_water_resistance(phys, &m, step);
                    }

                    // drag in YZ
                    let (fx, fy, fz) = (b.turn_res.x.powf(step), b.turn_res.y.powf(step), b.turn_res.z.powf(step));
                    let mut ts = m.rotate_vector_inverse(phys.turn_speed);
                    let magic = 1.0 / (1000.0 * (ts.x * ts.x) + 1.0) * fx;
                    ts.y *= fy;
                    ts.z *= fz;
                    let force_up = (magic - 1.0) * ts.x * phys.turn_mass;
                    phys.turn_speed = m.rotate_vector(ts);
                    let com = m.rotate_vector(phys.centre_of_mass);
                    phys.apply_turn_force(&m, Vec3::new(0.0, 0.0, force_up), com + m.forward);

                    b.delta_volume_under_water = ((b.volume_under_water - b.prev_volume_under_water) * 10000.0) as i16;

                    // falling into water
                    if !on_land && b.in_water && m.up.z > 0.0 && b.delta_volume_under_water > 200 {
                        events.push(CarEvent::Sound { sound: CarSound::CarSplash, volume: b.delta_volume_under_water as f32 });
                        let mut speed_up = magnitude_squared(phys.move_speed) * b.delta_volume_under_water as f32 * 0.0004;
                        if speed_up + phys.move_speed.z > h.brake_deceleration {
                            speed_up = h.brake_deceleration - phys.move_speed.z;
                        }
                        if speed_up < 0.0 {
                            speed_up = 0.0;
                        }
                        let mut speed_fwd = dot(phys.move_speed, m.forward);
                        speed_fwd *= -(b.delta_volume_under_water as f32) * 0.01 * h.traction_loss;
                        let speed = speed_fwd * m.forward + Vec3::new(0.0, 0.0, speed_up);
                        let splash_impulse = speed * phys.mass;
                        phys.apply_move_force(splash_impulse);
                        phys.apply_turn_force(&m, splash_impulse, buoyance_point);
                    }

                    // spray at the sides of the boat
                    if b.delta_volume_under_water > 75 {
                        let speed = magnitude(phys.move_speed);
                        let splash1 = speed.min(0.75);
                        let splash2 = b.delta_volume_under_water as f32 * 0.005 * 0.2;
                        let front = 0.9 * col.bounding_box.max.y;
                        for side in [1.0f32, -1.0] {
                            let mut dir = -0.5 * phys.move_speed;
                            dir.z += 0.1 * speed;
                            dir += side * 0.5 * m.right * speed;
                            let mut pos = front * m.forward + side * 0.5 * m.right + m.pos + b.buoyance_point;
                            if let Some(level) = self.ents.water.height(pos.x, pos.y, pos.z, true) {
                                pos.z = level;
                            }
                            let mut cs = Particle::new(ParticleKind::CarSplash, pos, 0.75 * dir, splash1);
                            cs.colour = Some(splash_colour);
                            cs.rotation_speed = rng.range_i32(0, 30) as f32;
                            cs.rotation = rng.range_i32(0, 90) as f32;
                            cs.frame = 1;
                            events.push(CarEvent::Particle(cs));
                            let mut bs = Particle::new(ParticleKind::BoatSplash, pos, dir, splash2);
                            bs.colour = Some(jet_colour);
                            events.push(CarEvent::Particle(bs));
                        }
                    }
                    b.prev_volume_under_water = b.volume_under_water;
                } else {
                    b.in_water = false;
                    phys.flags.is_in_water = false;
                }
                b.is_anchored
            };

            if is_anchored_after {
                phys.move_speed.x = 0.0;
                phys.move_speed.y = 0.0;
                let heading = |m: &crate::math::Matrix| (-m.forward.x).atan2(m.forward.y);
                if b.orientation == INVALID_ORIENTATION {
                    b.orientation = heading(&base.matrix);
                } else {
                    let pos = base.matrix.pos;
                    let delta = b.orientation - heading(&base.matrix);
                    base.matrix.rotate_z(delta);
                    base.matrix.pos = pos;
                }
            }
            let _ = (status::PHYSICS, VehicleData::default);
            events.extend(veh.process_delayed_explosion(&timer, false).into_iter().map(CarEvent::Vehicle));
        }
        if let Kind::Vehicle(v) = &mut e.kind {
            v.sub = VehicleSub::Boat(boat_data);
        }
        self.ents.pools.restore(r, e);
        env.events = events;
        if let Some(culprit) = blow_up {
            self.blow_up_boat(r, culprit, env, rng);
        }
    }
}
