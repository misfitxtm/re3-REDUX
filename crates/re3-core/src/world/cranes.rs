//! Cranes in the world: `CCranes::InitCranes`, activation and `CCrane::Update`.
use super::{Kinds, World};
use crate::entity::object::CreatedBy;
use crate::entity::{EntityRef, Kind};
use crate::general::Rng;
use crate::math::{atan_of_xy, cross, normalise};
use crate::vehicle::cranes::*;
use crate::vehicle::env::{CarEnv, CarEvent, CarSound};
use glam::{Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2 as HALFPI, PI};

impl World {
    /// `InitCranes`: finds the crane buildings and gives each a crane (and a hook).
    pub fn init_cranes(&mut self, rng: &mut Rng) {
        self.ents.cranes = CraneManager::default();
        let models = self.ents.indices.crane_models();
        let mut found: Vec<EntityRef> = vec![];
        for s in &self.sectors {
            for &r in &s.lists[super::BUILDINGS] {
                if let Some(e) = self.ents.pools.get(r)
                    && models.contains(&e.base.model_index)
                {
                    found.push(r);
                }
            }
        }
        // the industrial big-building list
        if let Some(list) = self.big_buildings.get(1) {
            for &r in list {
                if let Some(e) = self.ents.pools.get(r)
                    && models.contains(&e.base.model_index)
                {
                    found.push(r);
                }
            }
        }
        for r in found {
            self.add_one_crane(r, rng);
        }
    }

    /// `AddThisOneCrane`.
    pub fn add_one_crane(&mut self, entity: EntityRef, rng: &mut Rng) {
        let full = self.ents.cranes.cranes.len() >= NUM_CRANES;
        let Some(e) = self.get_mut(entity) else { return };
        e.base.matrix.reset_orientation();
        if full {
            return;
        }
        let (pos, seed, model) = (e.base.position(), e.base.random_seed, e.base.model_index);
        let n = self.ents.cranes.cranes.len();
        let mut c = Crane { entity: Some(entity), position: pos, random_seed: seed, ..Default::default() };
        c.hook_angle = n as f32;
        while c.hook_angle > PI * 2.0 {
            c.hook_angle -= PI * 2.0;
        }
        c.hook_offset = DEFAULT_OFFSET;
        c.hook_height = DEFAULT_OFFSET;
        c.is_top = self.ents.indices.crane_models()[0] != model;
        // the cab on the south side of the map has no hook (the military crane)
        if c.is_top || pos.y > 0.0 {
            let magnet = self.ents.indices.magnet();
            if let Some(h) = self.new_object(magnet, crate::math::Matrix::IDENTITY, rng) {
                if let Some(he) = self.get_mut(h) {
                    he.base.flags.uses_collision = false;
                    he.base.flags.explosion_proof = true;
                    if let Some(p) = he.phys.as_mut() {
                        p.flags.affected_by_gravity = false;
                    }
                    if let Kind::Object(o) = &mut he.kind {
                        o.created_by = CreatedBy::Mission;
                    }
                }
                c.hook = Some(h);
            }
            c.hook_cur_pos = c.hook_coordinates();
        }
        let idx = self.ents.cranes.cranes.len();
        self.ents.cranes.cranes.push(c);
        self.set_hook_matrix(idx);
    }

    /// `ActivateCrane`.
    #[allow(clippy::too_many_arguments)] // the script command's arguments
    pub fn activate_crane(&mut self, inf_x: f32, sup_x: f32, inf_y: f32, sup_y: f32, drop: Vec3, heading: f32, is_crusher: bool, is_military: bool, pos_x: f32, pos_y: f32) {
        let (mut x, mut y) = (pos_x, pos_y);
        if x <= MIN_VALID_POSITION || y <= MIN_VALID_POSITION {
            x = drop.x;
            y = drop.y;
        }
        // FIX_BUGS: nothing near enough means no crane
        let Some(i) = self.ents.cranes.find_crane(x, y) else { return };
        let z = if is_crusher {
            CRUSHER_Z
        } else if is_military {
            MILITARY_Z
        } else {
            self.find_ground_z_for_coord((inf_x + sup_x) / 2.0, (inf_y + sup_y) / 2.0)
        };
        let c = &mut self.ents.cranes.cranes[i];
        c.pickup_x1 = inf_x;
        c.pickup_x2 = sup_x;
        c.pickup_y1 = inf_y;
        c.pickup_y2 = sup_y;
        c.dropoff_target = drop;
        c.status = CraneStatus::Activated;
        c.vehicle_picked_up = None;
        c.vehicles_collected = 0;
        c.dropoff_heading = heading;
        c.is_crusher = is_crusher;
        c.is_military_crane = is_military;
        c.was_military_crane = is_military || c.was_military_crane;
        c.time_for_next_check = 0;
        c.state = CraneState::Idle;
        let (a, d, h) = c.parameters_for_target(Vec3::new((inf_x + sup_x) / 2.0, (inf_y + sup_y) / 2.0, z));
        c.pickup_angle = a;
        c.pickup_distance = d;
        c.pickup_height = h;
        let (a, d, h) = c.parameters_for_target(drop);
        c.dropoff_angle = a;
        c.dropoff_distance = d;
        c.dropoff_height = h;
    }

    /// `DeActivateCrane`.
    pub fn deactivate_crane(&mut self, x: f32, y: f32) {
        if let Some(i) = self.ents.cranes.find_crane(x, y) {
            let c = &mut self.ents.cranes.cranes[i];
            c.status = CraneStatus::Deactivated;
            c.state = CraneState::Idle;
        }
    }

    /// `UpdateCranes`.
    pub fn update_cranes(&mut self, env: &mut CarEnv, replay_playing: bool) {
        let cam = env.camera_pos;
        for i in 0..self.ents.cranes.cranes.len() {
            let c = &self.ents.cranes.cranes[i];
            // the original's range test compares `y + r < y` for the south edge, so it never holds
            let near = cam.x + CRANE_UPDATE_RADIUS > c.position.x && cam.x - CRANE_UPDATE_RADIUS < c.position.x && cam.y + CRANE_UPDATE_RADIUS > c.position.y && cam.y + CRANE_UPDATE_RADIUS < c.position.y;
            if c.is_top || c.is_crusher || near {
                self.update_crane(i, env, replay_playing);
            }
        }
    }

    /// `SetHookMatrix`.
    fn set_hook_matrix(&mut self, i: usize) {
        let c = &self.ents.cranes.cranes[i];
        let Some(h) = c.hook else { return };
        let (cur, init) = (c.hook_cur_pos, c.hook_init_pos);
        if let Some(e) = self.get_mut(h) {
            e.base.matrix.pos = cur;
            let up = normalise(Vec3::new(init.x - cur.x, init.y - cur.y, 20.0));
            e.base.matrix.right = cross(Vec3::Y, up);
            e.base.matrix.forward = cross(up, e.base.matrix.right);
            e.base.matrix.up = up;
            e.base.set_orientation(0.0, 0.0, -HALFPI);
        }
        self.remove(h);
        let _ = self.add(h);
    }

    /// `RotateCarriedCarProperly`: true when the car faces the drop-off heading.
    fn rotate_carried_car(&mut self, i: usize, step: f32) -> bool {
        let c = &self.ents.cranes.cranes[i];
        if c.dropoff_heading <= 0.0 {
            return true;
        }
        let Some(car) = c.vehicle_picked_up else { return true };
        let heading = c.dropoff_heading;
        let Some(e) = self.get_mut(car) else { return true };
        let mut delta = heading - atan_of_xy(e.base.matrix.forward.x, e.base.matrix.forward.y);
        while delta < -HALFPI {
            delta += PI;
        }
        while delta > HALFPI {
            delta -= PI;
        }
        let per_frame = CAR_ROTATION_SPEED * step;
        if delta.abs() <= per_frame {
            return true;
        }
        e.base.matrix.rotate_z(if delta < 0.0 { -per_frame } else { per_frame });
        false
    }

    /// `FindCarInSectorList` for the cranes' pickup area.
    fn crane_find_car(&mut self, i: usize, env: &mut CarEnv) {
        let (x1, x2, y1, y2) = {
            let c = &self.ents.cranes.cranes[i];
            (c.pickup_x1, c.pickup_x2, c.pickup_y1, c.pickup_y2)
        };
        let g = self.grid;
        let (xs, xe) = (g.index_x(x1).max(0), g.index_x(x2).min(g.num - 1));
        let (ys, ye) = (g.index_y(y1).max(0), g.index_y(y2).min(g.num - 1));
        self.advance_scan_code();
        let code = self.ents.scan_code;
        for x in xs..=xe {
            for y in ys..=ye {
                let Some(s) = self.sector(x, y).cloned() else { continue };
                for list in [super::VEHICLES, super::VEHICLES + 1] {
                    for &rv in &s.lists[list] {
                        let Some(v) = self.ents.pools.get_mut(rv) else { continue };
                        if v.base.scan_code == code {
                            continue;
                        }
                        v.base.scan_code = code;
                        let p = v.base.position();
                        let Kind::Vehicle(veh) = &mut v.kind else { continue };
                        if p.x < x1 || p.x > x2 || p.y < y1 || p.y > y2 || veh.driver.is_some() {
                            continue;
                        }
                        let s = v.phys.as_ref().map(|p| p.move_speed).unwrap_or(Vec3::ZERO);
                        if s.x.abs() >= CAR_MOVING_SPEED_THRESHOLD || s.y.abs() >= CAR_MOVING_SPEED_THRESHOLD || s.z.abs() >= CAR_MOVING_SPEED_THRESHOLD {
                            continue;
                        }
                        if !veh.is_car() || v.base.status == crate::entity::status::WRECKED || veh.health < 250.0 {
                            continue;
                        }
                        let model = v.base.model_index;
                        let c = &self.ents.cranes.cranes[i];
                        if !c.picks_up_car_type(model) || (c.is_military_crane && self.ents.cranes.military_has_this_one(model)) {
                            if !veh.flags.crane_message_done {
                                veh.flags.crane_message_done = true;
                                env.events.push(CarEvent::TriggerMessage(if !c.is_military_crane {
                                    "CR_1"
                                } else if c.picks_up_car_type(model) {
                                    "GA_20"
                                } else {
                                    "GA_19"
                                }));
                            }
                        } else {
                            let c = &mut self.ents.cranes.cranes[i];
                            c.vehicle_picked_up = Some(rv);
                            c.state = CraneState::GoingTowardsTarget;
                        }
                    }
                }
            }
        }
    }

    /// `CCrane::Update`.
    fn update_crane(&mut self, i: usize, env: &mut CarEnv, replay_playing: bool) {
        if replay_playing {
            return;
        }
        let timer = self.ents.timer.clone();
        let step = timer.time_step;
        let now = timer.time_in_ms;
        let active = {
            let c = &self.ents.cranes.cranes[i];
            ((c.status == CraneStatus::Activated || c.status == CraneStatus::Deactivated) && (env.player.game_cam_pos.x - c.position.x).abs() < CRANE_MOVEMENT_PROCESSING_RADIUS && (env.player.game_cam_pos.y - c.position.y).abs() < CRANE_MOVEMENT_PROCESSING_RADIUS) || c.state != CraneState::Idle
        };
        if active {
            match self.ents.cranes.cranes[i].state {
                CraneState::Idle => {
                    let c = &mut self.ents.cranes.cranes[i];
                    let (a, d, h) = (c.pickup_angle, c.pickup_distance, c.height_to_pickup());
                    if c.go_towards_target(a, d, h, 1.0, step) && now > c.time_for_next_check {
                        self.crane_find_car(i, env);
                    }
                }
                CraneState::GoingTowardsTarget => {
                    let pick = self.ents.cranes.cranes[i].vehicle_picked_up;
                    if let Some(rv) = pick {
                        let (x1, x2, y1, y2) = {
                            let c = &self.ents.cranes.cranes[i];
                            (c.pickup_x1, c.pickup_x2, c.pickup_y1, c.pickup_y2)
                        };
                        let max_z = self.get(rv).and_then(|e| self.ents.col_model(e.base.model_index)).map_or(0.0, |c| c.bounding_box.max.z);
                        let drop_it = match self.get(rv) {
                            Some(v) => {
                                let p = v.base.position();
                                let s = v.phys.as_ref().map(|p| p.move_speed).unwrap_or(Vec3::ZERO);
                                let has_driver = matches!(&v.kind, Kind::Vehicle(veh) if veh.driver.is_some());
                                p.x < x1 || p.x > x2 || p.y < y1 || p.y > y2 || has_driver || s.x.abs() > CAR_MOVING_SPEED_THRESHOLD || s.y.abs() > CAR_MOVING_SPEED_THRESHOLD || s.z.abs() > CAR_MOVING_SPEED_THRESHOLD || env.player.entering_vehicle == Some(rv)
                            }
                            None => true,
                        };
                        if drop_it {
                            let c = &mut self.ents.cranes.cranes[i];
                            c.vehicle_picked_up = None;
                            c.state = CraneState::Idle;
                        } else {
                            let p = self.get(rv).unwrap().base.position();
                            let (a, o, h) = self.ents.cranes.cranes[i].parameters_for_target(Vec3::new(p.x, p.y, p.z + max_z));
                            let c = &mut self.ents.cranes.cranes[i];
                            if c.go_towards_target(a, o, h, 1.0, step) {
                                let mut d = p - c.hook_cur_pos;
                                d.z += max_z;
                                if crate::math::magnitude_squared(d) < DISTANCE_FROM_HOOK_TO_VEHICLE_TO_COLLECT * DISTANCE_FROM_HOOK_TO_VEHICLE_TO_COLLECT {
                                    c.state = CraneState::GoingTowardsTargetOnlyHeight;
                                    c.hook_velocity *= 0.4;
                                    let crusher = c.is_crusher;
                                    if let Some(v) = self.get_mut(rv) {
                                        v.base.flags.uses_collision = false;
                                        if crusher {
                                            v.base.flags.collision_proof = true;
                                        }
                                        if let Kind::Vehicle(veh) = &mut v.kind {
                                            veh.flags.lights_on = false;
                                        }
                                    }
                                    env.events.push(CarEvent::Sound { sound: CarSound::CranePickup, volume: 0.0 });
                                }
                            }
                        }
                    } else {
                        self.ents.cranes.cranes[i].state = CraneState::Idle;
                    }
                }
                CraneState::LiftingTarget => {
                    self.rotate_carried_car(i, step);
                    let c = &mut self.ents.cranes.cranes[i];
                    let (a, d, h) = (c.dropoff_angle, c.dropoff_distance, c.height_to_dropoff());
                    if c.go_towards_target(a, d, h, CRANE_SLOWDOWN_MULTIPLIER, step) {
                        c.state = CraneState::RotatingTarget;
                    }
                    self.release_if_driven(i);
                }
                CraneState::GoingTowardsTargetOnlyHeight => {
                    self.rotate_carried_car(i, step);
                    let c = &mut self.ents.cranes.cranes[i];
                    let h = c.height_to_pickup_height();
                    if c.go_towards_height(h, CRANE_SLOWDOWN_MULTIPLIER, step) {
                        c.state = CraneState::LiftingTarget;
                    }
                    self.release_if_driven(i);
                }
                CraneState::RotatingTarget => {
                    let rotated = self.rotate_carried_car(i, step);
                    let c = &mut self.ents.cranes.cranes[i];
                    let (a, d, h) = (c.dropoff_angle, c.dropoff_distance, c.dropoff_height);
                    let moved = c.go_towards_target(a, d, h, 0.3, step);
                    if moved && rotated {
                        let pick = self.ents.cranes.cranes[i].vehicle_picked_up;
                        let dist = pick.and_then(|v| self.get(v)).map_or(0.0, |v| (Vec2::new(env.player.coors.x, env.player.coors.y) - Vec2::new(v.base.position().x, v.base.position().y)).length());
                        let was_military = self.ents.cranes.cranes[i].was_military_crane;
                        if dist > DISTANCE_FROM_PLAYER_TO_REMOVE_VEHICLE || !was_military {
                            self.ents.cranes.cranes[i].state = CraneState::DroppingTarget;
                            if let Some(rv) = pick {
                                let is_military = self.ents.cranes.cranes[i].is_military_crane;
                                let model = self.get(rv).map(|e| e.base.model_index).unwrap_or(-1);
                                if let Some(v) = self.get_mut(rv) {
                                    v.base.flags.uses_collision = true;
                                    if let Some(p) = v.phys.as_mut() {
                                        p.static_frames = 0;
                                    }
                                }
                                self.ents.cranes.cranes[i].vehicles_collected += 1;
                                if is_military {
                                    self.ents.cranes.register_car_for_military_crane(model);
                                    if !self.ents.cranes.have_all_cars_been_collected() {
                                        env.events.push(CarEvent::Money(CAR_REWARD_MILITARY_CRANE));
                                        env.events.push(CarEvent::TriggerMessage("GA_10"));
                                    }
                                    self.delete(rv);
                                }
                            }
                            self.ents.cranes.cranes[i].vehicle_picked_up = None;
                        }
                    }
                }
                CraneState::DroppingTarget => {
                    let c = &mut self.ents.cranes.cranes[i];
                    let (a, d, h) = (c.dropoff_angle, c.dropoff_distance, c.height_to_dropoff_height());
                    if c.go_towards_target(a, d, h, CRANE_SLOWDOWN_MULTIPLIER, step) {
                        c.state = CraneState::Idle;
                        c.time_for_next_check = now + 10000;
                    }
                }
            }

            // the hook swings after its target
            let c = &mut self.ents.cranes.cranes[i];
            let hook = c.hook_coordinates();
            c.hook_velocity += (Vec2::new(hook.x, hook.y) - Vec2::new(c.hook_cur_pos.x, c.hook_cur_pos.y)) * step * CRANE_MOVEMENT_SPEED;
            c.hook_velocity *= 0.98f32.powf(step);
            c.hook_cur_pos.x += c.hook_velocity.x * step;
            c.hook_cur_pos.y += c.hook_velocity.y * step;
            c.hook_cur_pos.z = hook.z;
            if matches!(c.state, CraneState::LiftingTarget | CraneState::GoingTowardsTargetOnlyHeight | CraneState::RotatingTarget)
                && let Some(rv) = c.vehicle_picked_up
            {
                let cur = c.hook_cur_pos;
                let max_z = self.get(rv).and_then(|e| self.ents.col_model(e.base.model_index)).map_or(0.0, |c| c.bounding_box.max.z);
                if let Some(v) = self.get_mut(rv) {
                    v.base.matrix.pos = Vec3::new(cur.x, cur.y, cur.z - max_z);
                    if let Some(p) = v.phys.as_mut() {
                        p.move_speed = Vec3::ZERO;
                    }
                    let up = normalise(Vec3::new(hook.x - cur.x, hook.y - cur.y, 20.0));
                    v.base.matrix.right = cross(v.base.matrix.forward, up);
                    v.base.matrix.forward = cross(up, v.base.matrix.right);
                    v.base.matrix.up = up;
                }
            }
        } else {
            // idle: the hook sways
            let c = &mut self.ents.cranes.cranes[i];
            let rnd = (c.random_seed as u32).wrapping_add(now >> 11) & 0xF;
            if rnd < 4 {
                c.hook_angle -= OSCILLATION_SPEED * step;
                if c.hook_angle < 0.0 {
                    c.hook_angle += PI * 2.0;
                }
            } else if rnd > 5 && rnd < 12 {
                c.hook_angle += OSCILLATION_SPEED * step;
                if c.hook_angle > PI * 2.0 {
                    c.hook_angle -= PI * 2.0;
                }
            }
            c.hook_cur_pos = c.hook_coordinates();
            c.hook_velocity = Vec2::ZERO;
        }

        // turn the crane building with the hook
        let (angle, entity) = {
            let c = &self.ents.cranes.cranes[i];
            (c.hook_angle, c.entity)
        };
        if let Some(e) = entity.and_then(|r| self.get_mut(r)) {
            let (cos, sin) = (angle.cos(), angle.sin());
            e.base.matrix.right.x = cos;
            e.base.matrix.forward.y = cos;
            e.base.matrix.right.y = sin;
            e.base.matrix.forward.x = -sin;
        }
        self.set_hook_matrix(i);
    }

    /// A carried car whose driver got in is dropped and the crane goes idle.
    fn release_if_driven(&mut self, i: usize) {
        let pick = self.ents.cranes.cranes[i].vehicle_picked_up;
        let driven = pick.is_none_or(|v| self.get(v).is_none_or(|e| matches!(&e.kind, Kind::Vehicle(veh) if veh.driver.is_some())));
        if driven {
            let c = &mut self.ents.cranes.cranes[i];
            c.vehicle_picked_up = None;
            c.state = CraneState::Idle;
        }
    }

    /// `CCranes::IsThisCarBeingCarriedByAnyCrane`.
    pub fn is_car_carried_by_crane(&self, car: EntityRef) -> bool {
        self.ents.cranes.is_car_being_carried(car)
    }

    /// Kinds helper kept for crane tests.
    pub fn crane_kinds() -> Kinds {
        Kinds { vehicles: true, ..Kinds::NONE }
    }
}
