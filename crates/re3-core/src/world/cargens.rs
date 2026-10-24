//! Car generators in the world (`CCarGenerator::Process`, `DoInternalProcessing`).
use super::{Kinds, World};
use crate::entity::{status, EntityRef, Kind, PoolKind};
use crate::general::Rng;
use crate::vehicle::base::{CarLock, VehicleSub, PARKED_VEHICLE};
use crate::vehicle::cargen::*;
use crate::vehicle::env::{CarEnv, CarHooks};
use glam::{Vec2, Vec3};

impl World {
    /// `CreateCarGenerator`.
    #[allow(clippy::too_many_arguments)] // the script command's arguments
    pub fn create_car_generator(&mut self, pos: Vec3, angle: f32, model: i32, colour1: i16, colour2: i16, force: u8, alarm: u8, lock: u8, min_delay: u16, max_delay: u16) -> Option<usize> {
        let col = self.ents.models.get(model)?.col_model.clone()?;
        let g = CarGenerator::setup(pos, angle, model, colour1, colour2, force, alarm, lock, min_delay, max_delay, self.ents.timer.time_in_ms, col.bounding_box.min, col.bounding_box.max);
        self.ents.cargens.create(g)
    }

    /// `CCarGenerator::CheckForBlockage`: any car or ped overlapping the spot.
    fn car_gen_blocked(&mut self, i: usize) -> bool {
        let g = &self.ents.cargens.generators[i];
        let (pos, size) = (g.pos, g.size);
        !self.find_objects_kinda_colliding(pos, size, true, 2, Kinds { vehicles: true, peds: true, ..Kinds::NONE }).is_empty()
    }

    /// `DoInternalProcessing`: tries to make the generator's car.
    fn car_gen_spawn(&mut self, i: usize, hooks: &mut dyn CarHooks, rng: &mut Rng) {
        if self.car_gen_blocked(i) {
            let g = &mut self.ents.cargens.generators[i];
            g.timer += 4;
            if g.uses_remaining == 0 {
                self.ents.cargens.current_active_count -= 1;
            }
            return;
        }
        if hooks.num_parked_cars() >= 10 {
            return;
        }
        let model = self.ents.cargens.generators[i].model_index;
        hooks.request_model(model);
        if !hooks.model_loaded(model) {
            return;
        }
        let g = self.ents.cargens.generators[i].clone();
        let is_boat = self.ents.indices.is_boat_model(model as i16);
        let mut made: Option<EntityRef> = None;
        if is_boat {
            if let Some(r) = self.new_boat(model as i16, PARKED_VEHICLE, rng) {
                let mut pos = g.pos;
                if pos.z <= -100.0 {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let base_height = self.ents.col_model(model as i16).map_or(0.0, |c| -c.bounding_box.min.z);
                pos.z += base_height;
                self.finish_parked(r, pos, g.angle);
                made = Some(r);
            }
        } else {
            let mut pos = g.pos;
            let found;
            if pos.z > -100.0 {
                let (z, f) = self.find_ground_z_for_3d_coord(pos);
                pos.z = z;
                found = f;
            } else {
                match self.process_vertical_line(Vec3::new(pos.x, pos.y, 1000.0), -1000.0, Kinds { buildings: true, ..Kinds::NONE }, false, None) {
                    Some(h) => {
                        pos.z = h.point.point.z;
                        found = true;
                    }
                    None => found = false,
                }
            }
            if found
                && let Some(r) = self.new_car(model as i16, PARKED_VEHICLE, rng)
            {
                let base_height = self.ents.col_model(model as i16).map_or(0.0, |c| -c.bounding_box.min.z);
                pos.z += base_height;
                self.finish_parked(r, pos, g.angle);
                if let Some(Kind::Vehicle(v)) = self.get_mut(r).map(|e| &mut e.kind) {
                    v.flags.lights_on = false;
                }
                made = Some(r);
            }
        }
        if let Some(r) = made {
            if let Some(Kind::Vehicle(v)) = self.get_mut(r).map(|e| &mut e.kind) {
                if rng.range_i32(0, 100) < g.alarm as i32 {
                    v.alarm_state = -1;
                }
                if rng.range_i32(0, 100) < g.doorlock as i32 {
                    v.door_lock = CarLock::Locked;
                }
                if g.colour1 != -1 && g.colour2 != 0 {
                    v.current_colour1 = g.colour1 as u8;
                    v.current_colour2 = g.colour2 as u8;
                }
                let _ = &v.sub;
            }
            self.ents.cargens.generators[i].vehicle_handle = r.handle;
        }
        let gen_ = &mut self.ents.cargens.generators[i];
        if gen_.uses_remaining < u16::MAX {
            gen_.uses_remaining -= 1;
        }
        gen_.timer = CarGenerator::calc_next_gen(self.ents.timer.time_in_ms);
        if gen_.uses_remaining == 0 {
            self.ents.cargens.current_active_count -= 1;
        }
    }

    /// Places a freshly made parked vehicle.
    fn finish_parked(&mut self, r: EntityRef, pos: Vec3, angle_deg: f32) {
        if let Some(e) = self.get_mut(r) {
            e.base.flags.is_static = false;
            e.base.set_position(pos);
            e.base.set_orientation(0.0, 0.0, angle_deg * std::f32::consts::PI / 180.0);
            e.base.status = status::ABANDONED;
            if let Kind::Vehicle(v) = &mut e.kind {
                v.flags.engine_on = false;
                v.door_lock = CarLock::Unlocked;
                let _ = matches!(v.sub, VehicleSub::None);
            }
            e.base.flags.uses_collision = true;
        }
        let _ = self.add(r);
    }

    /// `CCarGenerator::Process` for one generator.
    fn process_car_generator(&mut self, i: usize, env: &CarEnv, hooks: &mut dyn CarHooks, rng: &mut Rng) {
        let now = self.ents.timer.time_in_ms;
        let even = self.ents.cargens.generate_even_if_player_is_close_counter != 0;
        let (speed, centre) = (Vec2::new(env.player.speed.x, env.player.speed.y), Vec2::new(env.player_centre.x, env.player_centre.y));
        let ready = {
            let g = &self.ents.cargens.generators[i];
            g.vehicle_handle == -1 && (even || now >= g.timer) && g.uses_remaining != 0
        };
        if ready && self.ents.cargens.generators[i].within_range(centre, speed, env.generation_dist_multiplier, even) {
            self.car_gen_spawn(i, hooks, rng);
        }
        let handle = self.ents.cargens.generators[i].vehicle_handle;
        if handle == -1 {
            return;
        }
        let r = EntityRef { pool: PoolKind::Vehicle, handle };
        match self.get(r) {
            None => self.ents.cargens.generators[i].vehicle_handle = -1,
            Some(e) if e.base.status == status::PLAYER => {
                let g = &mut self.ents.cargens.generators[i];
                g.timer += 60000;
                g.vehicle_handle = -1;
                g.is_blocking = true;
                if let Some(Kind::Vehicle(v)) = self.get_mut(r).map(|e| &mut e.kind) {
                    v.flags.extended_range = false;
                }
            }
            Some(_) => {}
        }
    }

    /// `CTheCarGenerators::Process`: a quarter of the generators each call.
    pub fn process_car_generators(&mut self, env: &CarEnv, hooks: &mut dyn CarHooks, rng: &mut Rng, player_on_train_or_cutscene: bool) {
        if player_on_train_or_cutscene {
            return;
        }
        let m = &mut self.ents.cargens;
        m.process_counter += 1;
        if m.process_counter == 4 {
            m.process_counter = 0;
        }
        let n = m.generators.len();
        let mut i = self.ents.cargens.process_counter as usize;
        while i < n {
            self.process_car_generator(i, env, hooks, rng);
            i += 4;
        }
        if self.ents.cargens.generate_even_if_player_is_close_counter != 0 {
            self.ents.cargens.generate_even_if_player_is_close_counter -= 1;
        }
    }
}
