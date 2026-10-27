//! `CarCtrl.cpp`, part 5: making the random traffic and the emergency services cars.
use super::car_ctrl::{car_class as cc, GAME_SPEED_TO_CARAI_SPEED, LANE_WIDTH};
use super::curves::{calc_curve_point, calc_speed_scale_factor};
use super::pathfind::{PathEnv, PATH_CAR};
use crate::entity::status;
use crate::math::Matrix;
use crate::modelinfo::indices_gta3::mi;
use crate::ped::ctx::Px;
use crate::vehicle::base::{car_mission as m, driving_style, temp_action, VehicleData, RANDOM_VEHICLE};
use crate::world::{Kinds, World};
use glam::{Vec2, Vec3};

const INFINITE_Z: f32 = 1_000_000_000.0;

/// `TestCoorsCloseness` only asks the generation distance of the camera.
struct GenerationDistance(f32);

impl PathEnv for GenerationDistance {
    fn is_line_of_sight_clear(&mut self, _from: Vec3, _to: Vec3) -> bool {
        true
    }
    fn is_sphere_visible(&mut self, _pos: Vec3, _radius: f32, _cam: Option<&Matrix>) -> bool {
        false
    }
    fn find_ground_z(&mut self, _p: Vec3) -> (f32, bool) {
        (0.0, false)
    }
    fn generation_dist_multiplier(&self) -> f32 {
        self.0
    }
}

/// Where the traffic spawns this frame: the direction, how far, and the angle rule.
struct Spawn {
    front: Vec2,
    angle_limit: f32,
    invert_angle_limit_test: bool,
    preferred_distance: f32,
    test_for_collision: bool,
}

impl World {
    /// `GenerateRandomCars`.
    pub fn generate_random_cars(&mut self, px: &mut Px) {
        if px.hooks.cutscene_running() {
            return;
        }
        if self.ents.car_ctrl.num_random_cars < 30 {
            if self.ents.car_ctrl.count_down_to_cars_at_start == 0 {
                self.generate_one_random_car(px);
            } else {
                self.ents.car_ctrl.count_down_to_cars_at_start -= 1;
                if self.ents.car_ctrl.count_down_to_cars_at_start == 0 {
                    for _ in 0..50 {
                        self.generate_one_random_car(px);
                    }
                    self.ents.cargens.generate_even_if_player_is_close_counter = 20;
                }
            }
        }
        // about once per four seconds
        let t = &self.ents.timer;
        if t.time_in_ms & 0xFFFF_F000 != t.previous_time_in_ms & 0xFFFF_F000 {
            self.generate_emergency_services_car(px);
        }
    }

    /// How and where to look for a place to spawn, by what the player is doing.
    fn spawn_pattern(&self, px: &mut Px) -> Spawn {
        let cam = px.hooks.camera();
        let gdm = px.hooks.generation_dist_multiplier();
        let frame = self.ents.timer.frame_counter;
        let player_vehicle = self.find_player_vehicle();
        let speed = player_vehicle.and_then(|v| self.get(v)).and_then(|e| e.phys.as_ref()).map_or(Vec3::ZERO, |p| p.move_speed);
        let (vx, vy, vspeed) = (speed.x, speed.y, Vec2::new(speed.x, speed.y).length());
        let far = 120.0 * gdm;
        let s = |front: Vec2, angle_limit, invert, preferred_distance, test| Spawn { front, angle_limit, invert_angle_limit_test: invert, preferred_distance, test_for_collision: test };
        if cam.forward.z < -0.9 {
            // a top-down camera: spawn almost anywhere
            return s(Vec2::splat(0.707), -1.0, true, 40.0, false);
        }
        if player_vehicle.is_none() || vspeed <= 0.1 {
            // on foot or crawling: use where the camera looks
            return match frame & 1 {
                0 => s(cam.front_norm, 0.707, true, far, true),
                _ => s(cam.front_norm, 0.707, false, 40.0, true),
            };
        }
        let heading = Vec2::new(vx / vspeed, vy / vspeed);
        if vspeed > 0.4 {
            match frame & 3 {
                0 | 1 => s(heading, 0.85, true, far, false),
                2 => s(heading, 0.707, true, far, false),
                _ => s(heading, 0.707, false, 40.0, false),
            }
        } else {
            match frame & 3 {
                0 => s(heading, 0.85, true, far, false),
                1 => s(heading, 0.707, true, far, false),
                _ => s(heading, 0.707, false, 40.0, false),
            }
        }
    }

    /// `GenerateOneRandomCar`: one car appears on the road at the edge of the player's view, if everything fits.
    pub fn generate_one_random_car(&mut self, px: &mut Px) {
        let now = self.now();
        let target = self.find_player_centre_of_world(px);
        let zone = self.ents.zones.get_zone_info_for_time_of_day(target, &self.ents.clock);
        let player_speed = self.find_player_speed();
        let traffic = (self.ents.player_info.road_density * zone.car_density as f32) as i16;
        self.ents.player_info.traffic_multiplier = traffic;
        let (ctrl_cars, mult) = (&self.ents.car_ctrl, self.ents.car_ctrl.car_density_multiplier * self.ents.car_ctrl.car_number_multiplier);
        if ctrl_cars.num_random_cars as f32 >= traffic as f32 * mult {
            return;
        }
        let c = self.ents.car_ctrl.counts();
        if c.num_firetrucks_on_duty + c.num_ambulances_on_duty + c.num_parked_cars + c.num_mission_cars + c.num_law_enforcer_cars + c.num_random_cars >= c.max_cars_in_use {
            return;
        }
        let w = self.player_wanted().cloned().unwrap_or_default();
        let last_law = self.ents.car_ctrl.last_time_law_enforcer_created;
        let (car_model, car_class);
        if w.level > 1
            && c.num_law_enforcer_cars < w.max_law_enforcer_vehicles as i32
            && w.current_cops < w.max_cops
            && (w.level > 3 || (w.level > 2 && now > last_law + 5000) || (w.level > 1 && now > last_law + 8000))
        {
            car_class = cc::COPS;
            car_model = self.choose_police_car_model(px);
        } else {
            (car_model, car_class) = self.choose_model(px, &zone);
            if car_class == cc::COPS && w.level >= 1 {
                // all cop spawns with a wanted level are decided above
                return;
            }
        }
        let sp = self.spawn_pattern(px);
        let ignore_disabled = car_class == cc::COPS && w.level >= 1;
        let paths = std::mem::take(&mut self.ents.paths);
        let found = paths.new_generate_car_creation_coors(px.rng, target.x, target.y, sp.front.x, sp.front.y, sp.preferred_distance, sp.angle_limit, sp.invert_angle_limit_test, ignore_disabled);
        self.ents.paths = paths;
        let Some((spawn_pos, cur_node, next_node, mut between)) = found else { return };
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        if !self.find_objects_kinda_colliding(spawn_pos, 10.0, true, 2, kinds).is_empty() {
            return;
        }
        let gdm = px.hooks.generation_dist_multiplier();
        if !self.ents.paths.test_coors_closeness(&GenerationDistance(gdm), target, PATH_CAR, spawn_pos) {
            return;
        }
        let (cur_pn, next_pn) = (self.ents.paths.nodes[cur_node], self.ents.paths.nodes[next_node]);
        let first = cur_pn.first_link as usize;
        let id_in_node = (0..cur_pn.num_links as usize).find(|&i| self.ents.paths.connected_node(i + first) == next_node).unwrap_or(cur_pn.num_links as usize);
        let connection_id = self.ents.paths.car_path_connections[id_in_node + first] as usize;
        let link = self.ents.paths.car_path_links[connection_id];
        let lanes = if link.path_node_index as usize == next_node { link.num_left_lanes } else { link.num_right_lanes } as i32;
        let is_bike = matches!(self.ents.models.get(car_model).map(|m| &m.kind), Some(crate::modelinfo::Kind::Vehicle(v)) if v.vehicle_type == crate::vehicle::base::VehicleType::Bike as i32);
        if lanes == 0 || is_bike {
            // a one-way road against its direction, and bikes don't exist in the final game
            return;
        }
        let Some(r) = self.new_car(car_model as i16, RANDOM_VEHICLE, px.rng) else { return };
        let col = self.col_model_of(r).expect("car collision model");
        let mut ap = self.try_veh(r).expect("vehicle").auto_pilot;
        (ap.prev_route_node, ap.current_route_node, ap.next_route_node) = (0, cur_node as i32, next_node as i32);
        let mut siren = false;
        let mut colours_black = false;
        match car_class {
            cc::COPS => {
                ap.temp_action = temp_action::NONE;
                if w.level != 0 {
                    let speed = self.find_police_car_speed_for_wanted_level(px, r);
                    ap.cruise_speed = speed as u8;
                    ap.max_traffic_speed = (ap.cruise_speed / 2) as f32;
                    ap.car_mission = self.find_police_car_mission_for_wanted_level(px);
                    ap.driving_style = driving_style::AVOID_CARS;
                } else {
                    ap.cruise_speed = px.rng.range_i32(12, 16) as u8;
                    ap.max_traffic_speed = ap.cruise_speed as f32;
                    ap.driving_style = driving_style::STOP_FOR_CARS;
                    ap.car_mission = m::CRUISE;
                }
                // FBI cars are grey in the colour table but black when they come for the player
                colours_black = car_model == mi::FBICAR as i32;
            }
            _ => {
                ap.cruise_speed = px.rng.range_i32(9, 14) as u8;
                if car_class == cc::EXEC {
                    ap.cruise_speed = px.rng.range_i32(12, 18) as u8;
                } else if car_class == cc::POOR || car_class == cc::SPECIAL {
                    ap.cruise_speed = px.rng.range_i32(7, 10) as u8;
                }
                if col.bounding_box.max.y - col.bounding_box.min.y > 10.0 || car_class == cc::BIG {
                    ap.cruise_speed = (ap.cruise_speed as u32 * 3 / 4) as u8;
                }
                ap.max_traffic_speed = ap.cruise_speed as f32;
                ap.car_mission = m::CRUISE;
                ap.temp_action = temp_action::NONE;
                ap.driving_style = driving_style::STOP_FOR_CARS;
            }
        }
        if car_model == mi::MRWHOOP as i32 {
            siren = true;
        }
        ap.next_path_node_info = connection_id as u32;
        let lane = (px.rng.number() as i32 % lanes) as i8;
        (ap.next_lane, ap.current_lane) = (lane, lane);
        let car_length = 1.0 + (col.bounding_box.max.y - col.bounding_box.min.y) / 2.0;
        let dist_between_nodes = Vec2::new(cur_pn.pos.x - next_pn.pos.x, cur_pn.pos.y - next_pn.pos.y).length();
        // a car too long for the gap goes in the middle, else it is kept clear of both nodes
        if dist_between_nodes / 2.0 < car_length {
            between = 0.5;
        } else {
            between = (1.0 - car_length / dist_between_nodes).min((car_length / dist_between_nodes).max(between));
        }
        ap.next_direction = if cur_node >= next_node { 1 } else { -1 };
        if cur_pn.num_links == 1 {
            // nowhere to go
            self.delete(r);
            return;
        }
        let mut next_connection = ap.next_path_node_info;
        let mut new_link = 0usize;
        while next_connection == ap.next_path_node_info {
            new_link = (px.rng.number() as u32 % cur_pn.num_links as u32) as usize;
            next_connection = self.ents.paths.car_path_connections[new_link + first] as u32;
        }
        ap.current_path_node_info = next_connection;
        ap.current_direction = if self.ents.paths.connected_node(new_link + first) >= cur_node { 1 } else { -1 };
        let between_nodes = Vec2::new(next_pn.pos.x - cur_pn.pos.x, next_pn.pos.y - cur_pn.pos.y);
        let dist = between_nodes.length();
        let forward = if dist_between_nodes == 0.0 { Vec2::new(1.0, 0.0) } else { between_nodes / dist };
        {
            let e = self.get_mut(r).expect("vehicle");
            e.base.matrix.forward = Vec3::new(forward.x, forward.y, 0.0);
            e.base.matrix.right = Vec3::new(forward.y, -forward.x, 0.0);
            e.base.matrix.up = Vec3::Z;
        }
        let paths = &self.ents.paths;
        let cur_link_info = paths.car_path_links[ap.current_path_node_info as usize];
        let next_link_info = paths.car_path_links[ap.next_path_node_info as usize];
        let fwd_of = |l: &super::pathfind::CarPathLink, dir: i8| Vec2::new(dir as f32 * l.dir.x, dir as f32 * l.dir.y);
        // the forward directions the original works out before it may move the car on to the next node
        let (cur_fwd0, next_fwd0) = (fwd_of(&cur_link_info, ap.current_direction), fwd_of(&next_link_info, ap.next_direction));
        let lane_pos = |l: &super::pathfind::CarPathLink, lane: i8, f: Vec2| Vec3::new(l.pos.x + ((lane as f32 + l.one_way_lane_offset()) * LANE_WIDTH) * f.y, l.pos.y - ((lane as f32 + l.one_way_lane_offset()) * LANE_WIDTH) * f.x, 0.0);
        let (on_cur, on_next, dir_cur, dir_next);
        if between < 0.5 {
            on_cur = lane_pos(&cur_link_info, ap.current_lane, cur_fwd0);
            on_next = lane_pos(&next_link_info, ap.next_lane, next_fwd0);
            (dir_cur, dir_next) = (fwd_of(&cur_link_info, ap.current_direction), fwd_of(&next_link_info, ap.next_direction));
            ap.time_to_spend_on_current_curve = (calc_speed_scale_factor(on_cur, on_next, dir_cur, dir_next) * (1000.0 / ap.max_traffic_speed)) as i32;
            ap.time_entered_curve = now.wrapping_sub(((0.5 + between) * ap.time_to_spend_on_current_curve as f32) as u32);
        } else {
            let turned = super::car_nodes::pick_next_node_randomly(&self.ents.paths, &mut ap, px.rng);
            let _ = turned;
            ap.time_entered_curve = now.wrapping_sub(((between - 0.5) * ap.time_to_spend_on_current_curve as f32) as u32);
            let paths = &self.ents.paths;
            let (cl, nl) = (paths.car_path_links[ap.current_path_node_info as usize], paths.car_path_links[ap.next_path_node_info as usize]);
            on_cur = lane_pos(&cl, ap.current_lane, cur_fwd0);
            on_next = lane_pos(&nl, ap.next_lane, next_fwd0);
            (dir_cur, dir_next) = (fwd_of(&cl, ap.current_direction), fwd_of(&nl, ap.next_direction));
        }
        let along = now.wrapping_sub(ap.time_entered_curve) as f32 / ap.time_to_spend_on_current_curve as f32;
        let (pos_curve, dir_curve) = calc_curve_point(on_cur, on_next, Vec3::new(dir_cur.x, dir_cur.y, 0.0), Vec3::new(dir_next.x, dir_next.y, 0.0), along, ap.time_to_spend_on_current_curve);
        let between_vec = cur_pn.pos - next_pn.pos;
        let mut final_pos = pos_curve + between_vec * 2.0 / between_vec.length();
        final_pos.z = between * next_pn.pos.z + (1.0 - between) * cur_pn.pos.z;
        let road = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        let mut ground = INFINITE_Z;
        if let Some(h) = self.process_vertical_line(final_pos, 1000.0, road, false, None) {
            ground = h.point.point.z;
        }
        if let Some(h) = self.process_vertical_line(final_pos, -1000.0, road, false, None)
            && (h.point.point.z - final_pos.z).abs() < (ground - final_pos.z).abs()
        {
            ground = h.point.point.z;
        }
        if ground == INFINITE_Z || (ground - final_pos.z).abs() > 7.0 {
            // no ground, or none near where the road is
            self.delete(r);
            return;
        }
        final_pos.z = ground + VehicleData::height_above_road(&col);
        {
            let e = self.get_mut(r).expect("vehicle");
            e.base.set_position(final_pos);
            e.phys.as_mut().expect("physical").move_speed = dir_curve / GAME_SPEED_TO_CARAI_SPEED;
        }
        let speed_diff = self.ent(r).phys.as_ref().expect("physical").move_speed.truncate() - Vec2::new(player_speed.x, player_speed.y);
        let distance_to_target = Vec2::new(pos_curve.x - target.x, pos_curve.y - target.y);
        {
            let v = self.try_veh_mut(r).expect("vehicle");
            v.auto_pilot = ap;
            v.siren_or_alarm |= siren;
            v.clump_alpha = 0;
            if colours_black {
                v.current_colour1 = 0;
                v.current_colour2 = 0;
            }
        }
        if car_class == cc::COPS {
            self.get_mut(r).expect("vehicle").base.status = if ap.car_mission == m::CRUISE { status::SIMPLE } else { status::PHYSICS };
            self.change_law_enforcer_state(r, true);
        } else {
            self.get_mut(r).expect("vehicle").base.status = status::SIMPLE;
        }
        let pos = self.epos(r);
        let flat_dist = Vec2::new(target.x - pos.x, target.y - pos.y).length();
        let gdm = px.hooks.generation_dist_multiplier();
        if !px.hooks.is_on_screen(r) {
            if flat_dist > 50.0 {
                // too far away and unseen: not needed
                self.delete(r);
                return;
            }
        } else if flat_dist > gdm * 130.0 || flat_dist < gdm * 110.0 {
            self.delete(r);
            return;
        } else {
            let cam = px.hooks.camera_position();
            if Vec2::new(cam.x - pos.x, cam.y - pos.y).length() < 90.0 * gdm {
                self.delete(r);
                return;
            }
        }
        let radius = col.bounding_sphere.radius;
        let cars_only = Kinds { vehicles: true, ..Kinds::NONE };
        if sp.test_for_collision && !self.find_objects_kinda_colliding(pos, radius + 20.0, true, 2, cars_only).is_empty() {
            self.delete(r);
            return;
        }
        if !self.find_objects_kinda_colliding(pos, radius, true, 2, cars_only).is_empty() {
            self.delete(r);
            return;
        }
        if speed_diff.dot(distance_to_target) >= 0.0 {
            self.delete(r);
            return;
        }
        // not the same colours as the last car of this model
        let (mut c1, mut c2) = {
            let v = self.try_veh(r).expect("vehicle");
            (v.current_colour1, v.current_colour2)
        };
        if let Some(crate::modelinfo::Kind::Vehicle(mdata)) = self.ents.models.get_mut(car_model).map(|m| &mut m.kind) {
            mdata.avoid_same_colour(&mut c1, &mut c2, px.rng);
        }
        {
            let v = self.try_veh_mut(r).expect("vehicle");
            v.current_colour1 = c1;
            v.current_colour2 = c2;
        }
        let _ = self.add(r);
        if car_class == cc::COPS {
            self.add_police_car_occupants(px, r);
        } else {
            self.set_up_driver(px, r);
        }
        // one car in 64 drives like a madman
        if px.rng.number() & 0x3F == 0 {
            self.get_mut(r).expect("vehicle").base.status = status::PHYSICS;
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.driving_style = driving_style::AVOID_CARS;
            ap.cruise_speed = ap.cruise_speed.wrapping_add(10);
        }
        if car_class == cc::COPS {
            self.ents.car_ctrl.last_time_law_enforcer_created = now;
        }
    }

    /// `GenerateEmergencyServicesCar`: an ambulance for an accident or a fire engine for a fire near the player.
    pub fn generate_emergency_services_car(&mut self, px: &mut Px) {
        if self.player_wanted().is_some_and(|w| w.level > 3) {
            return;
        }
        let c = self.ents.car_ctrl.counts();
        if c.num_firetrucks_on_duty + c.num_ambulances_on_duty + c.num_parked_cars + c.num_mission_cars + c.num_law_enforcer_cars + c.num_random_cars > c.max_cars_in_use {
            return;
        }
        let now = self.now();
        let player = self.player_info_pos();
        if self.ents.car_ctrl.num_ambulances_on_duty == 0 {
            if self.ents.accidents.count_active_accidents() < 2 {
                if px.hooks.model_loaded(mi::AMBULAN) {
                    px.hooks.set_model_is_deletable(mi::MEDIC);
                }
            } else {
                let (nearest, distance) = self.find_nearest_accident(player);
                if let Some(a) = nearest.filter(|_| distance < 30.0)
                    && self.count_cars_of_type(mi::AMBULAN) < 2
                    && now > self.ents.car_ctrl.last_time_ambulance_created + 30000
                {
                    px.hooks.request_model(mi::AMBULAN);
                    px.hooks.request_model(mi::MEDIC);
                    if px.hooks.model_loaded(mi::AMBULAN) && px.hooks.model_loaded(mi::MEDIC) {
                        let victim_pos = self.ents.accidents.accidents[a].victim.map_or(Vec3::ZERO, |v| self.epos(v));
                        if self.generate_one_emergency_services_car(px, mi::AMBULAN, victim_pos) {
                            self.ents.car_ctrl.last_time_ambulance_created = now;
                        }
                    }
                }
            }
        }
        if self.ents.car_ctrl.num_firetrucks_on_duty == 0 {
            if self.ents.fires.total_fires < 3 {
                if px.hooks.model_loaded(mi::FIRETRUCK) {
                    px.hooks.set_model_is_deletable(mi::FIREMAN);
                }
            } else {
                let (nearest, distance) = self.ents.fires.find_nearest(player);
                if let Some(f) = nearest.filter(|_| distance < 30.0)
                    && self.count_cars_of_type(mi::FIRETRUCK) < 2
                    && now > self.ents.car_ctrl.last_time_fire_truck_created + 35000
                {
                    px.hooks.request_model(mi::FIRETRUCK);
                    px.hooks.request_model(mi::FIREMAN);
                    if px.hooks.model_loaded(mi::FIRETRUCK) && px.hooks.model_loaded(mi::FIREMAN) {
                        let pos = self.ents.fires.fires[f].pos;
                        if self.generate_one_emergency_services_car(px, mi::FIRETRUCK, pos) {
                            self.ents.car_ctrl.last_time_fire_truck_created = now;
                        }
                    }
                }
            }
        }
    }

    /// `GenerateOneEmergencyServicesCar`: the car appears far down the road and heads for `pos`.
    pub fn generate_one_emergency_services_car(&mut self, px: &mut Px, model: i16, pos: Vec3) -> bool {
        let player = self.find_player_centre_of_world(px);
        let mut found = None;
        let mut attempts = 0;
        while found.is_none() && attempts < 5 {
            let paths = std::mem::take(&mut self.ents.paths);
            let spawn = paths.new_generate_car_creation_coors(px.rng, player.x, player.y, 0.707, 0.707, 120.0, -1.0, true, false);
            self.ents.paths = paths;
            if let Some(s) = spawn
                && self.find_objects_kinda_colliding(s.0, 10.0, true, 2, Kinds { vehicles: true, peds: true, ..Kinds::NONE }).is_empty()
            {
                found = Some(s);
            }
            attempts += 1;
        }
        let Some((mut spawn_pos, cur_node, next_node, between)) = found else { return false };
        let Some(r) = self.new_car(model, RANDOM_VEHICLE, px.rng) else { return false };
        self.get_mut(r).expect("vehicle").base.set_position(spawn_pos);
        let none = self.join_car_with_road_system_goto_coors(px, r, pos, false);
        {
            let ap = &mut self.try_veh_mut(r).expect("vehicle").auto_pilot;
            ap.destination_coors = pos;
            ap.car_mission = if none { m::GOTOCOORDS_STRAIGHT } else { m::GOTOCOORDS };
            ap.cruise_speed = 25;
            ap.max_traffic_speed = 25.0;
            ap.temp_action = temp_action::NONE;
            ap.driving_style = driving_style::AVOID_CARS;
        }
        let direction = Vec2::new(pos.x - spawn_pos.x, pos.y - spawn_pos.y).normalize_or_zero();
        {
            let e = self.get_mut(r).expect("vehicle");
            e.base.matrix.forward = Vec3::new(direction.x, direction.y, 0.0);
            e.base.matrix.right = Vec3::new(direction.y, -direction.x, 0.0);
            e.base.matrix.up = Vec3::Z;
        }
        let (cur, next) = (self.ents.paths.nodes[cur_node].pos, self.ents.paths.nodes[next_node].pos);
        // the original weights the nodes the other way round here
        spawn_pos.z = between * cur.z + (1.0 - between) * next.z;
        let road = Kinds { buildings: true, dummies: true, ..Kinds::NONE };
        let mut ground = INFINITE_Z;
        if let Some(h) = self.process_vertical_line(spawn_pos, 1000.0, road, false, None) {
            ground = h.point.point.z;
        }
        if let Some(h) = self.process_vertical_line(spawn_pos, -1000.0, road, false, None)
            && (h.point.point.z - spawn_pos.z).abs() < (ground - spawn_pos.z).abs()
        {
            ground = h.point.point.z;
        }
        if ground == INFINITE_Z {
            self.delete(r);
            return false;
        }
        let col = self.col_model_of(r).expect("car collision model");
        spawn_pos.z = ground - col.bounding_box.min.z;
        {
            let e = self.get_mut(r).expect("vehicle");
            e.base.set_position(spawn_pos);
            e.phys.as_mut().expect("physical").move_speed = Vec3::ZERO;
            e.base.status = status::PHYSICS;
        }
        match model {
            mi::FIRETRUCK => {
                self.try_veh_mut(r).expect("vehicle").flags.is_fire_truck_on_duty = true;
                self.ents.car_ctrl.num_firetrucks_on_duty += 1;
                self.add_firetruck_occupants(px, r);
            }
            mi::AMBULAN => {
                self.try_veh_mut(r).expect("vehicle").flags.is_ambulance_on_duty = true;
                self.ents.car_ctrl.num_ambulances_on_duty += 1;
                self.add_ambulance_occupants(px, r);
            }
            _ => {}
        }
        self.try_veh_mut(r).expect("vehicle").siren_or_alarm = true;
        let _ = self.add(r);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::{NoPedHooks, PedHooks};
    use crate::ped::types::ped_type;

    struct Cutscene;
    impl PedHooks for Cutscene {
        fn cutscene_running(&mut self) -> bool {
            true
        }
    }

    fn world_with_player() -> World {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        let p = w.create_ped(&mut px, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w
    }

    #[test]
    fn the_cars_at_the_start_wait_for_the_countdown_and_a_cutscene_stops_everything() {
        let mut w = world_with_player();
        let (mut rng, mut hooks) = (Rng::default(), Cutscene);
        w.generate_random_cars(&mut Px { rng: &mut rng, hooks: &mut hooks });
        assert_eq!(w.ents.car_ctrl.count_down_to_cars_at_start, 2);
        let mut hooks = NoPedHooks;
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.generate_random_cars(&mut px);
        assert_eq!((w.ents.car_ctrl.count_down_to_cars_at_start, w.ents.cargens.generate_even_if_player_is_close_counter), (1, 0));
        w.generate_random_cars(&mut px);
        // the countdown ended: the start burst ran (nothing fits in an empty world) and car generators are released
        assert_eq!((w.ents.car_ctrl.count_down_to_cars_at_start, w.ents.cargens.generate_even_if_player_is_close_counter), (0, 20));
        assert_eq!(w.ents.pools.vehicles.used(), 0);
    }

    #[test]
    fn an_empty_zone_asks_for_no_traffic() {
        let mut w = world_with_player();
        w.ents.zones.zone_info_array[0].car_density = 0;
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        w.generate_one_random_car(&mut Px { rng: &mut rng, hooks: &mut hooks });
        assert_eq!((w.ents.player_info.traffic_multiplier, w.ents.pools.vehicles.used()), (0, 0));
    }

    #[test]
    fn no_emergency_car_comes_without_an_accident_or_with_a_high_wanted_level() {
        let mut w = world_with_player();
        let (mut rng, mut hooks) = (Rng::default(), NoPedHooks);
        let mut px = Px { rng: &mut rng, hooks: &mut hooks };
        w.generate_emergency_services_car(&mut px);
        assert_eq!(w.ents.pools.vehicles.used(), 0);
        w.player_wanted_mut().unwrap().level = 4;
        w.generate_emergency_services_car(&mut px);
        assert_eq!(w.ents.accidents.count_active_accidents(), 0);
    }
}
