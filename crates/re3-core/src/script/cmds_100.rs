//! `Script.cpp` commands 100 to 199: more variable arithmetic, creating and moving chars and cars, messages and the clock.
use super::checks::vehicle_ref;
use super::commands as c;
use super::{bits_of, f32_of, RunningScript, ScriptHooks};
use crate::entity::{EntityRef, PoolKind};
use crate::ped::ctx::Px;
use crate::ped::data::{objective, state as ped_state, MISSION_CHAR};
use crate::ped::types::ped_type;
use crate::vehicle::base::{car_mission, driving_style, CarLock, MISSION_VEHICLE};
use crate::world::World;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

/// A ped a script knows by its handle.
pub fn ped_ref(handle: i32) -> EntityRef {
    EntityRef { pool: PoolKind::Ped, handle }
}

impl World {
    /// The ped of a script handle, if it is still there (`GetPedPool()->GetAt`).
    pub(crate) fn script_ped(&self, handle: i32) -> Option<EntityRef> {
        let r = ped_ref(handle);
        self.get(r).map(|_| r)
    }

    /// The vehicle of a script handle, if it is still there.
    pub(crate) fn script_vehicle(&self, handle: i32) -> Option<EntityRef> {
        let r = vehicle_ref(handle);
        self.get(r).map(|_| r)
    }

    fn ent_mut(&mut self, r: EntityRef) -> &mut crate::entity::Ent {
        self.get_mut(r).expect("live entity")
    }

    /// A position from a script: a height at or below the limit means "on the ground here".
    fn ground_or_given(&mut self, mut pos: Vec3) -> Vec3 {
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        pos
    }

    pub(crate) fn process_commands_100_to_199(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::ADD_TIMED_VAL_TO_FLOAT_VAR..=c::SUB_TIMED_FLOAT_VAR_FROM_FLOAT_LVAR => self.timed_arithmetic(s, command),
            c::SET_VAR_INT_TO_VAR_INT..=c::SET_LVAR_INT_TO_VAR_INT => {
                let sc = &mut self.ents.scripts;
                let (dest, src) = (sc.read_operand(&mut s.ip), sc.read_operand(&mut s.ip));
                let v = sc.value_of(s, src);
                sc.store_operand(s, dest, v);
            }
            c::CSET_VAR_INT_TO_VAR_FLOAT..=c::CSET_LVAR_FLOAT_TO_LVAR_INT => {
                // even commands turn a float into an int, odd ones an int into a float
                let sc = &mut self.ents.scripts;
                let (dest, src) = (sc.read_operand(&mut s.ip), sc.read_operand(&mut s.ip));
                let v = sc.value_of(s, src);
                let r = if (command - c::CSET_VAR_INT_TO_VAR_FLOAT).is_multiple_of(2) { f32_of(v) as i32 } else { bits_of(v as f32) };
                sc.store_operand(s, dest, r);
            }
            c::ABS_VAR_INT..=c::ABS_LVAR_FLOAT => {
                let sc = &mut self.ents.scripts;
                let dest = sc.read_operand(&mut s.ip);
                let v = sc.value_of(s, dest);
                let r = if command <= c::ABS_LVAR_INT { v.wrapping_abs() } else { bits_of(f32_of(v).abs()) };
                sc.store_operand(s, dest, r);
            }
            c::GENERATE_RANDOM_FLOAT => {
                let dest = self.ents.scripts.read_operand(&mut s.ip);
                // three draws "to make it extra random", then the original's FIX_BUGS range
                for _ in 0..3 {
                    px.rng.number();
                }
                let r = px.rng.range_f32(0.0, 1.0);
                self.ents.scripts.store_operand(s, dest, bits_of(r));
            }
            c::GENERATE_RANDOM_INT => {
                let dest = self.ents.scripts.read_operand(&mut s.ip);
                let r = px.rng.number() as i32;
                self.ents.scripts.store_operand(s, dest, r);
            }
            c::CREATE_CHAR => {
                self.collect(s, 5);
                self.script_create_char(px, s);
            }
            c::DELETE_CHAR => {
                self.collect(s, 1);
                self.script_delete_char(px, s);
            }
            c::CHAR_WANDER_DIR => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("CHAR_WANDER_DIR: no such ped");
                self.clear_all(px, ped);
                let mut path = self.sp(1);
                if !(0..=7).contains(&path) {
                    path = px.rng.range_i32(0, 8);
                }
                self.set_wander_path(px, ped, path as i8);
            }
            c::CHAR_FOLLOW_PATH => {
                self.collect(s, 4);
                let ped = self.script_ped(self.sp(0)).expect("CHAR_FOLLOW_PATH: no such ped");
                let pos = self.ground_or_given(self.spv(1));
                self.clear_all(px, ped);
                self.set_follow_path(ped, pos);
            }
            c::CHAR_SET_IDLE => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("CHAR_SET_IDLE: no such ped");
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective(px, ped, objective::WAIT_ON_FOOT);
            }
            c::GET_CHAR_COORDINATES => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("GET_CHAR_COORDINATES: no such ped");
                let at = self.ped_vehicle_or_self(ped);
                let pos = self.epos(at);
                self.ents.scripts.set_param_v3(0, pos);
                self.store(s, 3);
            }
            c::SET_CHAR_COORDINATES => {
                self.collect(s, 4);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_COORDINATES: no such ped");
                let mut pos = self.ground_or_given(self.spv(1));
                let at = self.ped_vehicle_or_self(ped);
                pos.z += self.base_offset_of(at);
                self.teleport_any(at, pos);
                px.hooks.clear_space_for_mission_entity(pos, at);
            }
            c::IS_CHAR_STILL_ALIVE => {
                self.collect(s, 1);
                let alive = self.script_ped(self.sp(0)).is_some_and(|p| !matches!(self.ped(p).ped_state, ped_state::DEAD | ped_state::DIE));
                s.update_compare_flag(alive);
            }
            c::IS_CHAR_IN_AREA_2D => {
                self.collect(s, 6);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_AREA_2D: no such ped");
                let (x1, y1, x2, y2) = (self.spf(1), self.spf(2), self.spf(3), self.spf(4));
                let at = self.ped_vehicle_or_self(ped);
                s.update_compare_flag(self.ent(at).base.is_within_area_2d(x1, y1, x2, y2));
                if self.sp(5) != 0 {
                    sh.highlight_important_area(s.ip, x1, y1, x2, y2, MAP_Z_LOW_LIMIT);
                }
            }
            c::IS_CHAR_IN_AREA_3D => {
                self.collect(s, 8);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_AREA_3D: no such ped");
                let (a, b) = (Vec3::new(self.spf(1), self.spf(2), self.spf(3)), Vec3::new(self.spf(4), self.spf(5), self.spf(6)));
                let at = self.ped_vehicle_or_self(ped);
                s.update_compare_flag(self.ent(at).base.is_within_area(a, b));
                if self.sp(7) != 0 {
                    sh.highlight_important_area(s.ip, a.x, a.y, b.x, b.y, (a.z + b.z) / 2.0);
                }
            }
            c::CREATE_CAR => {
                self.collect(s, 4);
                self.script_create_car(px, s);
            }
            c::DELETE_CAR => {
                self.collect(s, 1);
                if let Some(car) = self.script_vehicle(self.sp(0)) {
                    self.remove_references_to(car);
                    self.delete(car);
                }
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_CAR);
                }
            }
            c::CAR_GOTO_COORDINATES => {
                self.collect(s, 4);
                let car = self.script_vehicle(self.sp(0)).expect("CAR_GOTO_COORDINATES: no such car");
                let mut pos = self.ground_or_given(self.spv(1));
                pos.z += self.base_offset_of(car);
                let straight = self.join_car_with_road_system_goto_coors(px, car, pos, false);
                let now = self.now();
                self.ent_mut(car).base.status = crate::entity::status::PHYSICS;
                let v = self.try_veh_mut(car).unwrap();
                v.auto_pilot.car_mission = if straight { car_mission::GOTOCOORDS_STRAIGHT } else { car_mission::GOTOCOORDS };
                v.flags.engine_on = true;
                v.auto_pilot.cruise_speed = v.auto_pilot.cruise_speed.max(6);
                v.auto_pilot.anti_reverse_timer = now;
            }
            c::CAR_WANDER_RANDOMLY => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("CAR_WANDER_RANDOMLY: no such car");
                self.join_car_with_road_system(px, car);
                let now = self.now();
                let v = self.try_veh_mut(car).unwrap();
                v.auto_pilot.car_mission = car_mission::CRUISE;
                v.flags.engine_on = true;
                v.auto_pilot.cruise_speed = v.auto_pilot.cruise_speed.max(6);
                v.auto_pilot.anti_reverse_timer = now;
            }
            c::CAR_SET_IDLE => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("CAR_SET_IDLE: no such car");
                self.try_veh_mut(car).unwrap().auto_pilot.car_mission = car_mission::NONE;
            }
            c::GET_CAR_COORDINATES => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_COORDINATES: no such car");
                let pos = self.epos(car);
                self.ents.scripts.set_param_v3(0, pos);
                self.store(s, 3);
            }
            c::SET_CAR_COORDINATES => {
                self.collect(s, 4);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_COORDINATES: no such car");
                self.script_set_car_coordinates(px, car);
            }
            c::IS_CAR_STILL_ALIVE => {
                self.collect(s, 1);
                let alive = self.script_vehicle(self.sp(0)).is_some_and(|v| {
                    let e = self.ent(v);
                    e.base.status != crate::entity::status::WRECKED && (self.try_veh(v).is_some_and(|d| d.is_boat()) || !e.phys.as_ref().is_some_and(|p| p.flags.is_in_water))
                });
                s.update_compare_flag(alive);
            }
            c::SET_CAR_CRUISE_SPEED => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_CRUISE_SPEED: no such car");
                let limit = 60.0 * self.ents.handling.get(self.try_veh(car).unwrap().handling).transmission.max_cruise_velocity;
                let speed = self.spf(1).min(limit);
                self.try_veh_mut(car).unwrap().auto_pilot.cruise_speed = speed as u8;
            }
            c::SET_CAR_DRIVING_STYLE => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_DRIVING_STYLE: no such car");
                self.try_veh_mut(car).unwrap().auto_pilot.driving_style = self.sp(1) as u8;
            }
            c::SET_CAR_MISSION => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_MISSION: no such car");
                let (mission, now) = (self.sp(1) as u8, self.now());
                let v = self.try_veh_mut(car).unwrap();
                v.auto_pilot.car_mission = mission;
                v.auto_pilot.anti_reverse_timer = now;
                v.flags.engine_on = true;
            }
            c::IS_CAR_IN_AREA_2D => {
                self.collect(s, 6);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_IN_AREA_2D: no such car");
                let (x1, y1, x2, y2) = (self.spf(1), self.spf(2), self.spf(3), self.spf(4));
                s.update_compare_flag(self.ent(car).base.is_within_area_2d(x1, y1, x2, y2));
                if self.sp(5) != 0 {
                    sh.highlight_important_area(s.ip, x1, y1, x2, y2, MAP_Z_LOW_LIMIT);
                }
            }
            c::IS_CAR_IN_AREA_3D => {
                self.collect(s, 8);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_IN_AREA_3D: no such car");
                let (a, b) = (Vec3::new(self.spf(1), self.spf(2), self.spf(3)), Vec3::new(self.spf(4), self.spf(5), self.spf(6)));
                s.update_compare_flag(self.ent(car).base.is_within_area(a, b));
                if self.sp(7) != 0 {
                    sh.highlight_important_area(s.ip, a.x, a.y, b.x, b.y, (a.z + b.z) / 2.0);
                }
            }
            c::SPECIAL_0..=c::SPECIAL_7 => {}
            c::PRINT_BIG..=c::PRINT_SOON => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 2);
                let (time, flag) = (self.sp(0), self.sp(1));
                match command {
                    c::PRINT_BIG => sh.print_big(&key, time as u32, flag - 1),
                    c::PRINT => sh.print(&key, time as u32, flag),
                    c::PRINT_NOW => sh.print_now(&key, time as u32, flag),
                    _ => sh.print_soon(&key, time as u32, flag),
                }
            }
            c::CLEAR_PRINTS => sh.clear_messages(),
            c::GET_TIME_OF_DAY => {
                let clock = &self.ents.clock;
                (self.ents.scripts.params[0], self.ents.scripts.params[1]) = (clock.hours as i32, clock.minutes as i32);
                self.store(s, 2);
            }
            c::SET_TIME_OF_DAY => {
                self.collect(s, 2);
                let now = self.now();
                self.ents.clock.set_game_clock(self.sp(0) as u8, self.sp(1) as u8, now);
            }
            c::GET_MINUTES_TO_TIME_OF_DAY => {
                self.collect(s, 2);
                self.ents.scripts.params[0] = self.ents.clock.minutes_until(self.sp(0) as u8, self.sp(1) as u8);
                self.store(s, 1);
            }
            c::IS_POINT_ON_SCREEN => {
                self.collect(s, 4);
                let pos = self.ground_or_given(self.spv(0));
                let radius = self.spf(3);
                s.update_compare_flag(sh.is_sphere_visible(pos, radius));
            }
            c::DEBUG_ON | c::DEBUG_OFF => {}
            c::RETURN_TRUE => s.update_compare_flag(true),
            c::RETURN_FALSE => s.update_compare_flag(false),
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// The car of a ped that is in one, or the ped.
    fn ped_vehicle_or_self(&self, ped: EntityRef) -> EntityRef {
        let pd = self.ped(ped);
        if pd.in_vehicle { pd.my_vehicle.unwrap_or(ped) } else { ped }
    }

    /// `Teleport` of a ped, car or boat.
    pub(crate) fn teleport_any(&mut self, r: EntityRef, pos: Vec3) {
        if matches!(self.ent(r).kind, crate::entity::Kind::Ped(_)) {
            self.teleport(r, pos);
        } else if self.ents.indices.is_boat_model(self.ent(r).base.model_index) {
            self.teleport_boat(r, pos);
        } else {
            self.teleport_car(r, pos);
        }
    }

    /// The timed additions (commands 120 to 131): the number is per frame, so it is scaled by the time step.
    fn timed_arithmetic(&mut self, s: &mut RunningScript, command: u16) {
        let step = self.ents.timer.time_step;
        let sub = command >= c::SUB_TIMED_VAL_FROM_FLOAT_VAR;
        let sc = &mut self.ents.scripts;
        let dest = sc.read_operand(&mut s.ip);
        let src = sc.read_operand(&mut s.ip);
        let (a, b) = (f32_of(sc.value_of(s, dest)), f32_of(sc.value_of(s, src)));
        let r = if sub { a - step * b } else { a + step * b };
        sc.store_operand(s, dest, bits_of(r));
    }

    /// `COMMAND_CREATE_CHAR`.
    fn script_create_char(&mut self, px: &mut Px, s: &mut RunningScript) {
        use crate::modelinfo::indices_gta3::mi;
        let (ty, mut model) = (self.sp(0) as usize, self.sp(1));
        if ty == ped_type::COP {
            model = match model as i16 {
                mi::COP => crate::ped::cop::cop_type::STREET as i32,
                mi::SWAT => crate::ped::cop::cop_type::SWAT as i32,
                mi::FBI => crate::ped::cop::cop_type::FBI as i32,
                mi::ARMY => crate::ped::cop::cop_type::ARMY as i32,
                _ => model,
            };
        }
        let ped = if ty == ped_type::COP {
            let clump = px.hooks.load_ped_clump(crate::ped::population::cop_model(model as u8));
            self.create_cop(px, model as u8, clump)
        } else if ty == ped_type::EMERGENCY || ty == ped_type::FIREMAN {
            let clump = px.hooks.load_ped_clump(if ty == ped_type::EMERGENCY { mi::MEDIC } else { mi::FIREMAN });
            self.create_emergency(px, ty, clump)
        } else {
            let clump = px.hooks.load_ped_clump(model as i16);
            self.create_civilian(px, ty, model as i16, clump)
        };
        let Some(ped) = ped else { return };
        let pd = self.ped_mut(ped);
        pd.char_created_by = MISSION_CHAR;
        pd.flags.responds_to_threats = false;
        pd.flags.allow_medics_to_revive_me = false;
        let mut pos = self.ground_or_given(self.spv(2));
        pos.z += 1.0;
        let e = self.ent_mut(ped);
        e.base.set_position(pos);
        e.base.set_orientation(0.0, 0.0, 0.0);
        px.hooks.clear_space_for_mission_entity(pos, ped);
        let _ = self.add(ped);
        let level = self.ents.zones.get_level_from_position(pos);
        if let Some(p) = self.ent_mut(ped).phys.as_mut() {
            p.zone_level = level as i8;
        }
        self.ents.population.total_mission_peds += 1;
        self.ents.scripts.params[0] = ped.handle;
        self.store(s, 1);
        if s.is_mission_script {
            self.ents.scripts.cleanup.add_entity_to_list(ped.handle, super::CLEANUP_CHAR);
        }
    }

    /// `COMMAND_DELETE_CHAR`: a ped in a car leaves it first.
    fn script_delete_char(&mut self, px: &mut Px, s: &RunningScript) {
        let handle = self.sp(0);
        if let Some(ped) = self.script_ped(handle) {
            let pd = self.ped(ped);
            if pd.in_vehicle
                && let Some(car) = pd.my_vehicle
            {
                let is_cop = pd.ped_type == ped_type::COP;
                if self.try_veh(car).is_some_and(|v| v.driver == Some(ped)) {
                    let status = self.ent(car).base.status;
                    let v = self.try_veh_mut(car).unwrap();
                    let new = v.remove_driver(status);
                    if v.door_lock == CarLock::LockedInitially {
                        v.door_lock = CarLock::Unlocked;
                    }
                    let law = v.flags.is_law_enforcer;
                    self.ent_mut(car).base.status = new;
                    if is_cop && law {
                        self.change_law_enforcer_state(car, false);
                    }
                } else if let Some(v) = self.try_veh_mut(car) {
                    v.remove_passenger(ped);
                }
            }
            self.remove_references_to(ped);
            self.destroy_ped(px, ped);
            self.ents.population.total_mission_peds -= 1;
        }
        if s.is_mission_script {
            self.ents.scripts.cleanup.remove_entity_from_list(handle, super::CLEANUP_CHAR);
        }
    }

    /// `COMMAND_CREATE_CAR`.
    fn script_create_car(&mut self, px: &mut Px, s: &mut RunningScript) {
        let model = self.sp(0) as i16;
        let mut pos = self.ground_or_given(self.spv(1));
        let boat = self.ents.indices.is_boat_model(model);
        let created = if boat { self.new_boat(model, MISSION_VEHICLE, px.rng) } else { self.new_car(model, MISSION_VEHICLE, px.rng) };
        let Some(car) = created else { return };
        pos.z += self.base_offset_of(car);
        self.ent_mut(car).base.set_position(pos);
        px.hooks.clear_space_for_mission_entity(pos, car);
        self.ent_mut(car).base.status = crate::entity::status::ABANDONED;
        if boat {
            let v = self.try_veh_mut(car).unwrap();
            v.flags.is_locked = true;
            v.auto_pilot.car_mission = car_mission::NONE;
            v.auto_pilot.temp_action = 0;
            v.auto_pilot.cruise_speed = 20;
            v.auto_pilot.max_traffic_speed = 20.0;
        } else {
            self.try_veh_mut(car).unwrap().flags.is_locked = true;
            self.join_car_with_road_system(px, car);
            let level = self.ents.zones.get_level_from_position(pos);
            let v = self.try_veh_mut(car).unwrap();
            v.auto_pilot.car_mission = car_mission::NONE;
            v.auto_pilot.temp_action = 0;
            v.auto_pilot.driving_style = driving_style::STOP_FOR_CARS;
            v.auto_pilot.cruise_speed = 9;
            v.auto_pilot.max_traffic_speed = 9.0;
            (v.auto_pilot.current_lane, v.auto_pilot.next_lane) = (0, 0);
            v.flags.engine_on = false;
            v.flags.has_been_owned_by_player = true;
            if let Some(p) = self.ent_mut(car).phys.as_mut() {
                p.zone_level = level as i8;
            }
        }
        let _ = self.add(car);
        self.ents.scripts.params[0] = car.handle;
        self.store(s, 1);
        if s.is_mission_script {
            self.ents.scripts.cleanup.add_entity_to_list(car.handle, super::CLEANUP_CAR);
        }
    }

    /// `COMMAND_SET_CAR_COORDINATES`: the car is moved and finds its place on the road network again.
    fn script_set_car_coordinates(&mut self, px: &mut Px, car: EntityRef) {
        let mut pos = self.ground_or_given(self.spv(1));
        pos.z += self.base_offset_of(car);
        self.ent_mut(car).base.flags.is_static = false;
        let boat = self.ents.indices.is_boat_model(self.ent(car).base.model_index);
        self.teleport_any(car, pos);
        px.hooks.clear_space_for_mission_entity(pos, car);
        if boat {
            return;
        }
        let v = self.try_veh(car).unwrap();
        let (mission, dest, target) = (v.auto_pilot.car_mission, v.auto_pilot.destination_coors, v.auto_pilot.target_car);
        use car_mission as m;
        match mission {
            m::CRUISE => self.join_car_with_road_system(px, car),
            m::RAMPLAYER_FARAWAY | m::RAMPLAYER_CLOSE | m::BLOCKPLAYER_FARAWAY | m::BLOCKPLAYER_CLOSE | m::BLOCKPLAYER_HANDBRAKESTOP => {
                let player = self.player_info_pos();
                self.join_car_with_road_system_goto_coors(px, car, player, false);
            }
            m::GOTOCOORDS | m::GOTOCOORDS_STRAIGHT | m::GOTOCOORDS_ACCURATE | m::GOTO_COORDS_STRAIGHT_ACCURATE => {
                self.join_car_with_road_system_goto_coors(px, car, dest, false);
            }
            m::RAMCAR_FARAWAY | m::RAMCAR_CLOSE | m::BLOCKCAR_FARAWAY | m::BLOCKCAR_CLOSE | m::BLOCKCAR_HANDBRAKESTOP => {
                if let Some(t) = target {
                    let tp = self.epos(t);
                    self.join_car_with_road_system_goto_coors(px, car, tp, false);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::world::tests::LANDSTAL_MODEL;

    #[derive(Default)]
    struct Spy {
        prints: Vec<(String, u32, i32)>,
        big: Vec<(String, u32, i32)>,
    }
    impl ScriptHooks for Spy {
        fn print(&mut self, key: &str, time: u32, flags: i32) {
            self.prints.push((key.to_owned(), time, flags));
        }
        fn print_big(&mut self, key: &str, time: u32, style: i32) {
            self.big.push((key.to_owned(), time, style));
        }
        fn is_sphere_visible(&mut self, _pos: Vec3, _radius: f32) -> bool {
            true
        }
    }

    fn g(w: &World, offset: u32) -> i32 {
        w.ents.scripts.global(offset)
    }

    fn gf(w: &World, offset: u32) -> f32 {
        f32_of(g(w, offset))
    }

    fn run(a: &Asm, w: &mut World) -> Spy {
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        start(w);
        let mut spy = Spy::default();
        run_frame(w, &mut spy);
        spy
    }

    #[test]
    fn copies_conversions_abs_timed_and_random_numbers() {
        let mut a = Asm::new(8);
        a.cmd(c::SET_VAR_INT, &[A::G(8), A::I8(-7)]);
        a.cmd(c::SET_VAR_INT_TO_VAR_INT, &[A::G(12), A::G(8)]);
        a.cmd(c::ABS_VAR_INT, &[A::G(12)]);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(16), A::F(-2.75)]);
        a.cmd(c::CSET_VAR_INT_TO_VAR_FLOAT, &[A::G(20), A::G(16)]);
        a.cmd(c::CSET_VAR_FLOAT_TO_VAR_INT, &[A::G(24), A::G(8)]);
        a.cmd(c::ABS_VAR_FLOAT, &[A::G(16)]);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(28), A::F(1.0)]);
        a.cmd(c::ADD_TIMED_VAL_TO_FLOAT_VAR, &[A::G(28), A::F(4.0)]);
        a.cmd(c::SUB_TIMED_VAL_FROM_FLOAT_VAR, &[A::G(28), A::F(1.0)]);
        a.cmd(c::GENERATE_RANDOM_INT, &[A::G(32)]);
        a.cmd(c::GENERATE_RANDOM_FLOAT, &[A::G(36)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        w.ents.timer.time_step = 0.5;
        run(&a, &mut w);
        assert_eq!((g(&w, 12), gf(&w, 16), g(&w, 20), gf(&w, 24)), (7, 2.75, -2, -7.0));
        // 1 + 0.5 * 4 - 0.5 * 1
        assert_eq!(gf(&w, 28), 2.5);
        assert!((0.0..1.0).contains(&gf(&w, 36)));
    }

    #[test]
    fn a_mission_char_is_made_remembered_and_deleted() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CHAR, &[A::I8(4), A::I16(10), A::F(5.0), A::F(6.0), A::F(1.0), A::G(8)]);
        a.cmd(c::IS_CHAR_STILL_ALIVE, &[A::G(8)]);
        a.cmd(c::GET_CHAR_COORDINATES, &[A::G(8), A::G(12), A::G(16), A::G(20)]);
        a.cmd(c::IS_CHAR_IN_AREA_2D, &[A::G(8), A::F(0.0), A::F(0.0), A::F(10.0), A::F(10.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(1)]);
        a.cmd(c::DELETE_CHAR, &[A::G(8)]);
        a.cmd(c::IS_CHAR_STILL_ALIVE, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        let handle = g(&w, 8);
        let ped = ped_ref(handle);
        assert_eq!(w.ent(ped).base.matrix.pos, Vec3::new(5.0, 6.0, 2.0), "a metre above the given height");
        assert_eq!((w.ped(ped).char_created_by, w.ped(ped).flags.responds_to_threats, w.ents.population.total_mission_peds), (MISSION_CHAR, false, 1));
        assert!(w.ents.scripts.scripts[i].cond_result, "inside the area");
        assert_eq!(w.ents.scripts.cleanup.count, 1);
        assert_eq!((gf(&w, 12), gf(&w, 16)), (5.0, 6.0));
        w.ents.timer.time_in_ms = 10;
        run_frame(&mut w, &mut spy);
        assert!(w.get(ped).is_none() && !w.ents.scripts.scripts[i].cond_result);
        assert_eq!((w.ents.population.total_mission_peds, w.ents.scripts.cleanup.count), (0, 0));
    }

    #[test]
    fn a_mission_car_is_made_parked_and_locked_and_can_be_sent_places() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CAR, &[A::I16(LANDSTAL_MODEL), A::F(10.0), A::F(20.0), A::F(1.0), A::G(8)]);
        a.cmd(c::SET_CAR_CRUISE_SPEED, &[A::G(8), A::F(500.0)]);
        a.cmd(c::SET_CAR_DRIVING_STYLE, &[A::G(8), A::I8(3)]);
        a.cmd(c::SET_CAR_MISSION, &[A::G(8), A::I8(8)]);
        a.cmd(c::GET_CAR_COORDINATES, &[A::G(8), A::G(12), A::G(16), A::G(20)]);
        a.cmd(c::IS_CAR_STILL_ALIVE, &[A::G(8)]);
        a.cmd(c::IS_CAR_IN_AREA_3D, &[A::G(8), A::F(0.0), A::F(0.0), A::F(0.0), A::F(30.0), A::F(30.0), A::F(30.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        run(&a, &mut w);
        let car = vehicle_ref(g(&w, 8));
        let v = w.try_veh(car).unwrap();
        assert_eq!((v.created_by, v.flags.is_locked, v.flags.has_been_owned_by_player), (MISSION_VEHICLE, true, true));
        assert_eq!((v.auto_pilot.driving_style, v.auto_pilot.car_mission, v.flags.engine_on), (3, 8, true));
        // limited by what the car can do: 60 * max cruise velocity of the handling line
        let limit = 60.0 * w.ents.handling.get(v.handling).transmission.max_cruise_velocity;
        assert_eq!(v.auto_pilot.cruise_speed, limit as u8);
        assert_eq!(w.ent(car).base.status, crate::entity::status::ABANDONED);
        assert_eq!(gf(&w, 12), 10.0);
    }

    #[test]
    fn messages_and_the_clock() {
        let mut a = Asm::new(4);
        a.cmd(c::SET_TIME_OF_DAY, &[A::I8(7), A::I8(30)]);
        a.cmd(c::GET_TIME_OF_DAY, &[A::G(8), A::G(12)]);
        a.cmd(c::GET_MINUTES_TO_TIME_OF_DAY, &[A::I8(8), A::I8(0), A::G(16)]);
        // a text key is written inline, eight characters
        a.bytes.extend_from_slice(&c::PRINT_BIG.to_le_bytes());
        a.bytes.extend_from_slice(b"M_PASS\0\0");
        a.bytes.extend_from_slice(&[4, 100, 4, 3]);
        a.bytes.extend_from_slice(&c::PRINT.to_le_bytes());
        a.bytes.extend_from_slice(b"HELLO\0\0\0");
        a.bytes.extend_from_slice(&[5, 0xB8, 0x0B, 4, 1]);
        a.cmd(c::IS_POINT_ON_SCREEN, &[A::F(0.0), A::F(0.0), A::F(10.0), A::F(2.0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_script(&a);
        let spy = run(&a, &mut w);
        assert_eq!((g(&w, 8), g(&w, 12), g(&w, 16)), (7, 30, 30));
        assert_eq!(spy.big, vec![("M_PASS".to_owned(), 100, 2)]);
        assert_eq!(spy.prints, vec![("HELLO".to_owned(), 3000, 1)]);
    }
}
