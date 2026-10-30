//! `Script3.cpp`, commands 700 to 799: nodes, boats, random chars, cutscenes, collectables and numbers. (The pac-man commands and script fires are not ported yet.)
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::{mi, slot};
use crate::ped::ctx::Px;
use crate::ped::types::ped_type;
use crate::vehicle::base::car_mission;
use crate::world::World;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;
const PICKUP_PLACEMENT_OFFSET: f32 = 0.5;
const PED_FIND_Z_OFFSET: f32 = 5.0;
const GAME_SPEED_TO_METERS_PER_SECOND: f32 = 50.0;

/// `ThisIsAValidRandomPed`.
fn valid_random_ped(ty: usize) -> bool {
    use ped_type::*;
    matches!(ty, CIVMALE | CIVFEMALE | GANG1 | GANG2 | GANG3 | GANG4 | GANG5 | GANG6 | GANG7 | GANG8 | GANG9 | CRIMINAL | PROSTITUTE)
}

impl World {
    /// The ground under a point, or the point's own height when it is given.
    fn height_or_ground(&mut self, mut pos: Vec3, offset: f32) -> Vec3 {
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y) + offset;
        }
        pos
    }

    /// A pickup the script lays: model, type and quantity, placed on the ground when no height is given.
    fn script_new_pickup(&mut self, px: &mut Px, s: &mut RunningScript, pos: Vec3, what: (usize, u8, u32), store: bool) {
        let (model_slot, ty, quantity) = what;
        let pos = self.height_or_ground(pos, PICKUP_PLACEMENT_OFFSET);
        let model = self.ents.indices.gta3_slot(model_slot);
        let handle = self.generate_new_pickup(px, pos, model, ty, quantity);
        if store {
            self.ents.scripts.params[0] = handle;
            self.store(s, 1);
        }
    }

    /// `GET_RANDOM_CHAR_IN_AREA` and `_IN_ZONE`: an ordinary pedestrian near the player becomes the script's.
    fn script_random_char(&mut self, s: &RunningScript, inside: impl Fn(&World, Vec3) -> bool) -> i32 {
        let suits = |pd: &crate::ped::data::PedData, model: i16| pd.is_ped_in_control() && model != mi::SCUM_WOM && model != mi::SCUM_MAN && valid_random_ped(pd.ped_type);
        self.script_random_ped_where(s, suits, inside)
    }

    /// The search of `GET_RANDOM_CHAR_IN_*` and `GET_RANDOM_COP_IN_*` with what kind of ped suits left to the caller.
    pub(crate) fn script_random_ped_where(&mut self, s: &RunningScript, suits: impl Fn(&crate::ped::data::PedData, i16) -> bool, inside: impl Fn(&World, Vec3) -> bool) -> i32 {
        let player = self.player_info_pos();
        let refs = self.ped_refs();
        // the original walks the pool from the end, never reaching slot 0
        for r in refs.into_iter().rev() {
            let pd = self.ped(r);
            let p = self.epos(r);
            let model = self.ent(r).base.model_index;
            let ok = r.handle != self.ents.scripts.last_random_ped_id
                && pd.char_created_by == crate::ped::data::RANDOM_CHAR
                && suits(pd, model)
                && !self.ent(r).base.flags.remove_from_world
                && !pd.flags.fade_out
                && !(pd.flags.is_leader || pd.leader.is_some())
                && inside(self, p)
                && player.z - PED_FIND_Z_OFFSET <= p.z
                && player.z + PED_FIND_Z_OFFSET >= p.z;
            if !ok {
                continue;
            }
            self.ents.scripts.last_random_ped_id = r.handle;
            let pd = self.ped_mut(r);
            pd.char_created_by = crate::ped::data::MISSION_CHAR;
            pd.flags.responds_to_threats = false;
            self.ents.population.total_mission_peds += 1;
            if s.is_mission_script {
                self.ents.scripts.cleanup.add_entity_to_list(r.handle, super::CLEANUP_CHAR);
            }
            return r.handle;
        }
        -1
    }

    /// Whether a ped, car or object is on the screen: its bounding sphere is tested by the camera.
    fn on_screen(&self, sh: &mut dyn ScriptHooks, e: EntityRef) -> bool {
        sh.is_sphere_visible(self.bound_centre(e), self.bound_radius_of(e))
    }

    pub(crate) fn process_commands_700_to_799(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::SET_SWAT_REQUIRED | c::SET_FBI_REQUIRED | c::SET_ARMY_REQUIRED => {
                self.collect(s, 1);
                let on = self.sp(0) != 0;
                if let Some(w) = self.player_wanted_mut() {
                    match command {
                        c::SET_SWAT_REQUIRED => w.swat_required = on,
                        c::SET_FBI_REQUIRED => w.fbi_required = on,
                        _ => w.army_required = on,
                    }
                }
            }
            c::IS_CAR_IN_WATER => {
                self.collect(s, 1);
                let in_water = self.script_vehicle(self.sp(0)).is_some_and(|v| self.ent(v).phys.as_ref().is_some_and(|p| p.flags.is_in_water));
                s.update_compare_flag(in_water);
            }
            c::GET_CLOSEST_CHAR_NODE | c::GET_CLOSEST_CAR_NODE => {
                self.collect(s, 3);
                let pos = self.height_or_ground(self.spv(0), 0.0);
                let ty = usize::from(command == c::GET_CLOSEST_CHAR_NODE);
                let node = self.ents.paths.find_node_closest_to_coors(pos, ty, 999999.9, false, false);
                let at = self.ents.paths.nodes[node as usize].pos;
                self.ents.scripts.set_param_v3(0, at);
                self.store(s, 3);
            }
            c::CAR_GOTO_COORDINATES_ACCURATE => {
                self.collect(s, 4);
                let car = self.script_vehicle(self.sp(0)).expect("CAR_GOTO_COORDINATES_ACCURATE: no such car");
                let mut pos = self.height_or_ground(self.spv(1), 0.0);
                pos.z += self.base_offset_of(car);
                let straight = self.join_car_with_road_system_goto_coors(px, car, pos, false);
                let now = self.now();
                self.get_mut(car).unwrap().base.status = crate::entity::status::PHYSICS;
                let v = self.try_veh_mut(car).unwrap();
                v.auto_pilot.car_mission = if straight { car_mission::GOTO_COORDS_STRAIGHT_ACCURATE } else { car_mission::GOTOCOORDS_ACCURATE };
                v.flags.engine_on = true;
                v.auto_pilot.cruise_speed = v.auto_pilot.cruise_speed.max(6);
                v.auto_pilot.anti_reverse_timer = now;
            }
            c::IS_CAR_ON_SCREEN | c::IS_CHAR_ON_SCREEN | c::IS_OBJECT_ON_SCREEN => {
                self.collect(s, 1);
                let e = match command {
                    c::IS_CAR_ON_SCREEN => self.script_vehicle(self.sp(0)),
                    c::IS_CHAR_ON_SCREEN => self.script_ped(self.sp(0)),
                    _ => self.script_object(self.sp(0)),
                }
                .expect("IS_*_ON_SCREEN: no such entity");
                s.update_compare_flag(self.on_screen(sh, e));
            }
            c::GOSUB_FILE => {
                self.collect(s, 2);
                debug_assert!((s.stack_pointer as usize) < super::MAX_STACK_DEPTH);
                s.stack[s.stack_pointer as usize] = s.ip;
                s.stack_pointer += 1;
                s.ip = self.sp(0) as u32;
            }
            c::GET_GROUND_Z_FOR_3D_COORD => {
                self.collect(s, 3);
                let (z, _) = self.find_ground_z_for_3d_coord(self.spv(0));
                self.ents.scripts.set_param_f(0, z);
                self.store(s, 1);
            }
            c::SET_COMEDY_CONTROLS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_COMEDY_CONTROLS: no such car");
                let on = self.sp(1) != 0;
                self.try_veh_mut(car).unwrap().flags.comedy_controls = on;
            }
            c::BOAT_GOTO_COORDS => {
                self.collect(s, 4);
                let boat = self.script_vehicle(self.sp(0)).expect("BOAT_GOTO_COORDS: no such boat");
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT
                    && let Some(z) = self.ents.water.height(pos.x, pos.y, pos.z, false)
                {
                    pos.z = z;
                }
                let now = self.now();
                self.get_mut(boat).unwrap().base.status = crate::entity::status::PHYSICS;
                let v = self.try_veh_mut(boat).unwrap();
                v.auto_pilot.car_mission = car_mission::GOTOCOORDS_ASTHECROWSWIMS;
                v.auto_pilot.destination_coors = pos;
                v.flags.engine_on = true;
                v.auto_pilot.cruise_speed = v.auto_pilot.cruise_speed.max(6);
                v.auto_pilot.anti_reverse_timer = now;
            }
            c::BOAT_STOP => {
                self.collect(s, 1);
                let boat = self.script_vehicle(self.sp(0)).expect("BOAT_STOP: no such boat");
                self.get_mut(boat).unwrap().base.status = crate::entity::status::PHYSICS;
                let v = self.try_veh_mut(boat).unwrap();
                v.auto_pilot.car_mission = car_mission::NONE;
                v.flags.engine_on = false;
                v.auto_pilot.cruise_speed = 0;
            }
            c::SET_BOAT_CRUISE_SPEED => {
                self.collect(s, 2);
                let boat = self.script_vehicle(self.sp(0)).expect("SET_BOAT_CRUISE_SPEED: no such boat");
                let speed = self.spf(1) as u8;
                self.try_veh_mut(boat).unwrap().auto_pilot.cruise_speed = speed;
            }
            c::IS_PLAYER_SHOOTING_IN_AREA | c::IS_CHAR_SHOOTING_IN_AREA => {
                self.collect(s, 6);
                let ped = if command == c::IS_PLAYER_SHOOTING_IN_AREA { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CHAR_SHOOTING_IN_AREA: no such ped") };
                let (x1, y1, x2, y2) = (self.spf(1), self.spf(2), self.spf(3), self.spf(4));
                s.update_compare_flag(self.ped(ped).flags.is_shooting && self.ent(ped).base.is_within_area_2d(x1, y1, x2, y2));
                if self.sp(5) != 0 {
                    sh.highlight_important_area(s.ip, x1, y1, x2, y2, MAP_Z_LOW_LIMIT);
                }
            }
            c::IS_PLAYER_SHOOTING | c::IS_CHAR_SHOOTING => {
                self.collect(s, 1);
                let ped = if command == c::IS_PLAYER_SHOOTING { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CHAR_SHOOTING: no such ped") };
                s.update_compare_flag(self.ped(ped).flags.is_shooting);
            }
            c::IS_CURRENT_PLAYER_WEAPON | c::IS_CURRENT_CHAR_WEAPON => {
                self.collect(s, 2);
                let ped = if command == c::IS_CURRENT_PLAYER_WEAPON { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CURRENT_CHAR_WEAPON: no such ped") };
                let current = self.ped(ped).weapons[self.ped(ped).current_weapon as usize].ty;
                s.update_compare_flag(self.sp(1) as usize == current);
            }
            c::GET_RANDOM_CHAR_IN_AREA => {
                self.collect(s, 4);
                let (a, b) = ((self.spf(0), self.spf(1)), (self.spf(2), self.spf(3)));
                let handle = self.script_random_char(s, |_, p| p.x >= a.0.min(b.0) && p.x <= a.0.max(b.0) && p.y >= a.1.min(b.1) && p.y <= a.1.max(b.1));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::GET_RANDOM_CHAR_IN_ZONE => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let zone = self.ents.zones.find_zone_by_label_and_return_index(&label);
                let handle = self.script_random_char(s, |w, p| zone.is_some_and(|z| crate::zones::point_lies_within_zone(p, &w.ents.zones.zone_array[z])));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::IS_PLAYER_IN_TAXI => {
                self.collect(s, 1);
                let p = self.ents.player_ped.expect("player");
                let car = self.ped(p).my_vehicle.filter(|_| self.ped(p).in_vehicle);
                s.update_compare_flag(car.is_some_and(|v| matches!(self.ent(v).base.model_index, mi::TAXI | mi::CABBIE | mi::BORGNINE)));
            }
            c::CREATE_MONEY_PICKUP => {
                self.collect(s, 4);
                let (pos, quantity) = (self.spv(0), self.sp(3) as u32);
                self.script_new_pickup(px, s, pos, (slot::MONEY, crate::control::pickups::pickup_type::MONEY, quantity), true);
            }
            c::CREATE_COLLECTABLE1 => {
                self.collect(s, 3);
                let pos = self.spv(0);
                self.script_new_pickup(px, s, pos, (slot::COLLECTABLE1, crate::control::pickups::pickup_type::COLLECTABLE1, 0), false);
            }
            c::DROP_MINE => {
                self.collect(s, 3);
                let pos = self.spv(0);
                self.script_new_pickup(px, s, pos, (slot::CARMINE, crate::control::pickups::pickup_type::MINE_INACTIVE, 0), false);
            }
            c::DROP_NAUTICAL_MINE => {
                self.collect(s, 3);
                let pos = self.spv(0);
                self.script_new_pickup(px, s, pos, (slot::NAUTICALMINE, crate::control::pickups::pickup_type::MINE_INACTIVE, 0), false);
            }
            c::SET_COLLECTABLE1_TOTAL => {
                self.collect(s, 1);
                self.ents.player_info.total_packages = self.sp(0);
            }
            c::SET_CHAR_ACCURACY => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_ACCURACY: no such ped");
                self.ped_mut(ped).wep_accuracy = self.sp(1) as u8;
            }
            c::GET_CAR_SPEED => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_SPEED: no such car");
                let speed = self.ent(car).phys.as_ref().map_or(0.0, |p| crate::math::magnitude(p.move_speed));
                self.ents.scripts.set_param_f(0, speed * GAME_SPEED_TO_METERS_PER_SECOND);
                self.store(s, 1);
            }
            c::LOAD_CUTSCENE => {
                let name = self.ents.scripts.read_text_label(&mut s.ip);
                sh.load_cutscene(&name);
            }
            c::CREATE_CUTSCENE_OBJECT => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = sh.create_cutscene_object(self.sp(0));
                self.store(s, 1);
            }
            c::SET_CUTSCENE_ANIM => {
                self.collect(s, 1);
                let name = self.ents.scripts.read_text_label(&mut s.ip);
                sh.set_cutscene_anim(&name, self.sp(0));
            }
            c::START_CUTSCENE => sh.start_cutscene(),
            c::GET_CUTSCENE_TIME => {
                self.ents.scripts.params[0] = sh.cutscene_time_in_milliseconds();
                self.store(s, 1);
            }
            c::HAS_CUTSCENE_FINISHED => s.update_compare_flag(sh.has_cutscene_finished()),
            c::CLEAR_CUTSCENE => sh.clear_cutscene(),
            c::RESTORE_CAMERA_JUMPCUT => px.hooks.camera_restore_with_jump_cut(),
            c::IS_PROJECTILE_IN_AREA | c::DESTROY_PROJECTILES_IN_AREA => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                let found = self.projectiles_in_range(px, a.min(b), a.max(b), command == c::DESTROY_PROJECTILES_IN_AREA);
                s.update_compare_flag(found);
            }
            c::IS_CHAR_MODEL => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_MODEL: no such ped");
                s.update_compare_flag(self.sp(1) == i32::from(self.ent(ped).base.model_index));
            }
            c::LOAD_SPECIAL_MODEL => {
                self.collect(s, 1);
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.request_special_model(self.sp(0), &name);
            }
            c::CREATE_CUTSCENE_HEAD => {
                self.collect(s, 2);
                self.ents.scripts.params[0] = sh.create_cutscene_head(self.sp(0), self.sp(1));
                self.store(s, 1);
            }
            c::SET_CUTSCENE_HEAD_ANIM => {
                self.collect(s, 1);
                let name = self.ents.scripts.read_text_label(&mut s.ip);
                sh.set_cutscene_head_anim(&name, self.sp(0));
            }
            c::SIN | c::COS => {
                self.collect(s, 1);
                let a = self.spf(0).to_radians();
                self.ents.scripts.set_param_f(0, if command == c::SIN { a.sin() } else { a.cos() });
                self.store(s, 1);
            }
            c::GET_CAR_FORWARD_X | c::GET_CAR_FORWARD_Y => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_FORWARD: no such car");
                let f = self.ent(car).base.matrix.forward;
                let flat = f.x.hypot(f.y);
                self.ents.scripts.set_param_f(0, if command == c::GET_CAR_FORWARD_X { f.x / flat } else { f.y / flat });
                self.store(s, 1);
            }
            c::CHANGE_GARAGE_TYPE => {
                self.collect(s, 2);
                self.ents.garages.change_garage_type(self.sp(0) as usize, self.sp(1) as u8, 0);
            }
            c::ACTIVATE_CRUSHER_CRANE => {
                self.collect(s, 10);
                let (a, b) = ((self.spf(2), self.spf(3)), (self.spf(4), self.spf(5)));
                let drop = Vec3::new(self.spf(6), self.spf(7), self.spf(8));
                let (heading, pick_x, pick_y) = (self.spf(9).to_radians(), self.spf(0), self.spf(1));
                self.activate_crane(a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1), drop, heading, true, false, pick_x, pick_y);
            }
            c::PRINT_WITH_2_NUMBERS..=c::PRINT_WITH_6_NUMBERS_SOON => {
                // three commands (queued, now, soon) for each count of numbers from two to six
                let index = command - c::PRINT_WITH_2_NUMBERS;
                let (count, kind) = (2 + (index / 3) as usize, (index % 3) as u8);
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, count + 2);
                let p = self.ents.scripts.params;
                sh.print_with_numbers(kind, &key, p[count] as u32, p[count + 1], &p[..count]);
            }
            _ => return self.process_commands_700_to_799_rest(px, sh, s, command),
        }
        0
    }
}

impl World {
    /// The rest of `ProcessCommands700To799`: formations, statistics and what was damaged by what.
    fn process_commands_700_to_799_rest(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        use super::hooks::ScriptStat as St;
        match command {
            c::SET_CHAR_OBJ_FOLLOW_CHAR_IN_FORMATION => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_FOLLOW_CHAR_IN_FORMATION: no such ped");
                self.ped_mut(ped).flags.script_objective_completed = false;
                if let Some(t) = self.script_ped(self.sp(1)) {
                    self.set_objective_entity(px, ped, crate::ped::data::objective::FOLLOW_CHAR_IN_FORMATION, t);
                }
                self.set_formation(ped, self.sp(2) as u8);
            }
            c::PLAYER_MADE_PROGRESS => {
                self.collect(s, 1);
                sh.stat(St::ProgressMade(self.sp(0)));
            }
            c::SET_PROGRESS_TOTAL => {
                self.collect(s, 1);
                sh.stat(St::TotalProgress(self.sp(0)));
            }
            c::REGISTER_JUMP_DISTANCE | c::REGISTER_JUMP_HEIGHT => {
                self.collect(s, 1);
                let v = self.spf(0);
                sh.stat(if command == c::REGISTER_JUMP_DISTANCE { St::JumpDistance(v) } else { St::JumpHeight(v) });
            }
            c::REGISTER_JUMP_FLIPS | c::REGISTER_JUMP_SPINS | c::REGISTER_JUMP_STUNT => {
                self.collect(s, 1);
                let v = self.sp(0);
                sh.stat(match command {
                    c::REGISTER_JUMP_FLIPS => St::JumpFlips(v),
                    c::REGISTER_JUMP_SPINS => St::JumpSpins(v),
                    _ => St::StuntJump(v),
                });
            }
            c::REGISTER_UNIQUE_JUMP_FOUND => sh.stat(St::UniqueJumpFound),
            c::SET_UNIQUE_JUMPS_TOTAL => {
                self.collect(s, 1);
                sh.stat(St::UniqueJumpsTotal(self.sp(0)));
            }
            c::REGISTER_PASSENGER_DROPPED_OFF_TAXI => sh.stat(St::PassengerDroppedOffWithTaxi),
            c::REGISTER_MONEY_MADE_TAXI => {
                self.collect(s, 1);
                sh.stat(St::MoneyMadeWithTaxi(self.sp(0)));
            }
            c::REGISTER_MISSION_GIVEN => sh.stat(St::MissionGiven),
            c::REGISTER_MISSION_PASSED => {
                let name = self.ents.scripts.read_text_label(&mut s.ip);
                sh.stat(St::MissionPassed(name));
                px.hooks.stat(crate::ped::ctx::StatEvent::CheckPointReachedSuccessfully);
            }
            c::SET_CHAR_RUNNING => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_RUNNING: no such ped");
                self.ped_mut(ped).flags.is_running = self.sp(1) != 0;
            }
            c::IS_FIRST_CAR_COLOUR | c::IS_SECOND_CAR_COLOUR => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_*_CAR_COLOUR: no such car");
                let v = self.try_veh(car).unwrap();
                let colour = if command == c::IS_FIRST_CAR_COLOUR { v.current_colour1 } else { v.current_colour2 };
                s.update_compare_flag(i32::from(colour) == self.sp(1));
            }
            c::HAS_CHAR_BEEN_DAMAGED_BY_WEAPON => {
                self.collect(s, 2);
                let damaged = self.script_ped(self.sp(0)).is_some_and(|p| i32::from(self.ped(p).last_wep_dam) == self.sp(1));
                s.update_compare_flag(damaged);
            }
            c::HAS_CAR_BEEN_DAMAGED_BY_WEAPON => {
                self.collect(s, 2);
                let damaged = self.script_vehicle(self.sp(0)).is_some_and(|v| i32::from(self.try_veh(v).unwrap().last_weapon_damage) == self.sp(1));
                s.update_compare_flag(damaged);
            }
            c::IS_CHAR_IN_CHARS_GROUP => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_CHARS_GROUP: no such ped");
                let leader = self.script_ped(self.sp(1)).expect("IS_CHAR_IN_CHARS_GROUP: no such leader");
                s.update_compare_flag(self.ped(ped).leader == Some(leader));
            }
            c::START_PACMAN_RACE => {
                self.collect(s, 1);
                self.start_pacman_race(px, self.sp(0));
            }
            c::START_PACMAN_RECORD => self.start_pacman_record(),
            c::GET_NUMBER_OF_POWER_PILLS_EATEN => {
                self.ents.scripts.params[0] = self.ents.pacman.pills_eaten_in_race;
                self.store(s, 1);
            }
            c::CLEAR_PACMAN => self.clear_pm_pickups(),
            c::START_PACMAN_SCRAMBLE => {
                self.collect(s, 5);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let (spread, count) = (self.spf(3), self.sp(4) as i16);
                self.generate_pm_pickups(px, pos, spread, count, crate::control::pacman::PacManType::Scramble);
            }
            c::GET_NUMBER_OF_POWER_PILLS_CARRIED => {
                self.ents.scripts.params[0] = self.power_pills_carried_by_player();
                self.store(s, 1);
            }
            c::CLEAR_NUMBER_OF_POWER_PILLS_CARRIED => self.reset_power_pills_carried_by_player(),
            c::CLEAR_NUMBER_OF_POWER_PILLS_EATEN => self.ents.pacman.pills_eaten_in_race = 0,
            c::ADD_POWER_PILL => {
                self.collect(s, 3);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                self.generate_one_pm_pickup(pos);
            }
            c::START_SCRIPT_FIRE => {
                self.collect(s, 3);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                self.ents.scripts.params[0] = self.start_script_fire(px, pos, None, 0.8, true);
                self.store(s, 1);
            }
            c::IS_SCRIPT_FIRE_EXTINGUISHED => {
                self.collect(s, 1);
                s.update_compare_flag(!self.ents.fires.fires[self.sp(0) as usize].is_ongoing);
            }
            c::REMOVE_SCRIPT_FIRE => {
                self.collect(s, 1);
                self.remove_script_fire(px, self.sp(0) as usize);
            }
            c::REMOVE_ALL_SCRIPT_FIRES => self.remove_all_script_fires(px),
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// `CProjectileInfo::IsProjectileInRange`.
    fn projectiles_in_range(&mut self, px: &mut Px, min: Vec3, max: Vec3, remove: bool) -> bool {
        let camera = px.hooks.camera_position();
        let mut projectiles = std::mem::take(&mut self.ents.projectiles);
        let found = projectiles.in_range(&mut crate::ped::fire::WorldProjectiles { world: self, rng: px.rng, camera, smoke: vec![] }, min, max, remove);
        self.ents.projectiles = projectiles;
        found
    }
}

#[cfg(test)]
mod tests {
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::vehicle::base::RANDOM_VEHICLE;
    use crate::world::tests::LANDSTAL_MODEL;

    #[derive(Default)]
    struct Spy {
        stats: Vec<super::super::ScriptStat>,
        numbered: Vec<(u8, String, u32, i32, Vec<i32>)>,
    }
    impl ScriptHooks for Spy {
        fn stat(&mut self, stat: super::super::ScriptStat) {
            self.stats.push(stat);
        }
        fn print_with_numbers(&mut self, kind: u8, key: &str, time: u32, flags: i32, numbers: &[i32]) {
            self.numbered.push((kind, key.to_owned(), time, flags, numbers.to_vec()));
        }
    }

    fn world(a: &Asm) -> World {
        let mut w = world_with_script(a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w
    }

    fn g(w: &World, o: u32) -> i32 {
        w.ents.scripts.global(o)
    }

    #[test]
    fn sines_forward_vectors_speed_and_colours() {
        let mut a = Asm::new(6);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(8), A::F(90.0)]);
        a.cmd(c::SIN, &[A::G(8), A::G(8)]);
        a.cmd(c::GET_CAR_FORWARD_Y, &[A::G(16), A::G(12)]);
        a.cmd(c::IS_FIRST_CAR_COLOUR, &[A::G(16), A::I8(3)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.try_veh_mut(car).unwrap().current_colour1 = 3;
        w.ents.scripts.set_global(16, car.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(super::super::f32_of(g(&w, 8)), 1.0);
        assert_eq!(super::super::f32_of(g(&w, 12)), 1.0, "a new car faces along y");
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn pickups_numbered_messages_and_statistics() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_MONEY_PICKUP, &[A::F(10.0), A::F(0.0), A::F(0.0), A::I16(250), A::G(8)]);
        a.cmd(c::SET_COLLECTABLE1_TOTAL, &[A::I8(7)]);
        a.cmd(c::REGISTER_JUMP_HEIGHT, &[A::F(12.5)]);
        a.cmd(c::REGISTER_MONEY_MADE_TAXI, &[A::I8(30)]);
        a.cmd(c::REGISTER_MISSION_GIVEN, &[]);
        a.bytes.extend_from_slice(&c::PRINT_WITH_3_NUMBERS_NOW.to_le_bytes());
        a.bytes.extend_from_slice(b"NUMS\0\0\0\0");
        a.bytes.extend_from_slice(&[4, 1, 4, 2, 4, 3, 5, 0xB8, 0x0B, 4, 1]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(w.ents.pickups.pickups[0].ty, crate::control::pickups::pickup_type::MONEY);
        assert_eq!(w.ents.pickups.pickups[0].quantity, 250);
        assert_eq!(w.ents.player_info.total_packages, 7);
        use super::super::ScriptStat::*;
        assert_eq!(spy.stats, vec![JumpHeight(12.5), MoneyMadeWithTaxi(30), MissionGiven]);
        assert_eq!(spy.numbered, vec![(1, "NUMS".to_owned(), 3000, 1, vec![1, 2, 3])]);
    }

    #[test]
    fn a_random_ordinary_pedestrian_becomes_the_scripts() {
        let mut a = Asm::new(4);
        a.cmd(c::GET_RANDOM_CHAR_IN_AREA, &[A::F(-50.0), A::F(-50.0), A::F(50.0), A::F(50.0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        // slot 0 is never reached; make two peds so that the second one is found
        let _first = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
        let ped = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
        w.get_mut(ped).unwrap().base.set_position(Vec3::new(5.0, 5.0, 0.0));
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(g(&w, 8), ped.handle);
        assert_eq!((w.ped(ped).char_created_by, w.ents.population.total_mission_peds, w.ents.scripts.cleanup.count), (crate::ped::data::MISSION_CHAR, 1, 1));
    }

    #[test]
    fn a_script_fire_burns_until_the_script_removes_it() {
        let mut a = Asm::new(4);
        a.cmd(c::START_SCRIPT_FIRE, &[A::F(5.0), A::F(6.0), A::F(7.0), A::G(8)]);
        a.cmd(c::IS_SCRIPT_FIRE_EXTINGUISHED, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(0)]);
        a.cmd(c::REMOVE_SCRIPT_FIRE, &[A::G(8)]);
        a.cmd(c::IS_SCRIPT_FIRE_EXTINGUISHED, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let fire = &w.ents.fires.fires[g(&w, 8) as usize];
        assert!(fire.is_ongoing && fire.is_script_fire && fire.pos == Vec3::new(5.0, 6.0, 7.0));
        assert!(!w.ents.scripts.scripts[i].cond_result);
        w.ents.timer.time_in_ms += 10;
        run_frame(&mut w, &mut Spy::default());
        let fire = &w.ents.fires.fires[g(&w, 8) as usize];
        assert!(!fire.is_ongoing && !fire.is_script_fire);
        assert!(w.ents.scripts.scripts[i].cond_result);
    }
}
