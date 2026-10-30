//! `Script2.cpp`, commands 400 to 499: objectives, weapons, weather, cleanup marks, roads, cranes and numbered messages.
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::math::atan_of_xy;
use crate::ped::ctx::Px;
use crate::ped::data::objective;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// The ped a script command's first argument names, with its objective bookkeeping reset.
    fn objective_ped(&mut self, handle: i32) -> EntityRef {
        let ped = self.script_ped(handle).expect("objective command: no such ped");
        self.ped_mut(ped).flags.script_objective_completed = false;
        ped
    }

    /// A box given by two corners: its middle on the ground and how far its edge is.
    fn guard_area_of(&mut self, a: (f32, f32), b: (f32, f32)) -> (Vec3, f32) {
        let (inf_x, sup_x) = (a.0.min(b.0), a.0.max(b.0));
        let (inf_y, sup_y) = (a.1.min(b.1), a.1.max(b.1));
        let (x, y) = ((inf_x + sup_x) / 2.0, (inf_y + sup_y) / 2.0);
        let z = self.find_ground_z_for_coord(x, y);
        (Vec3::new(x, y, z), (x - inf_x).max(y - inf_y))
    }

    /// `COMMAND_TURN_CHAR_TO_FACE_COORD` and the player's.
    fn turn_to_face_coord(&mut self, ped: EntityRef) {
        let in_vehicle = self.ped(ped).in_vehicle;
        let pos = self.epos(self.ped(ped).my_vehicle.filter(|_| in_vehicle).unwrap_or(ped));
        let mut heading = atan_of_xy(pos.x - self.spf(1), pos.y - self.spf(2)) + FRAC_PI_2;
        if heading > TAU {
            heading -= TAU;
        }
        if !in_vehicle {
            let pd = self.ped_mut(ped);
            pd.rotation_cur = heading;
            pd.rotation_dest = heading;
            self.get_mut(ped).unwrap().base.set_heading(heading);
        }
    }

    /// The player's ped for the commands that name a player.
    fn the_player(&self) -> EntityRef {
        self.ents.player_ped.expect("script needs a player")
    }

    pub(crate) fn process_commands_400_to_499(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::ADD_UPSIDEDOWN_CAR_CHECK | c::REMOVE_UPSIDEDOWN_CAR_CHECK => {
                self.collect(s, 1);
                self.script_vehicle(self.sp(0)).expect("upside-down check: no such car");
                if command == c::ADD_UPSIDEDOWN_CAR_CHECK {
                    self.ents.script_checks.upside_down.add_car_to_check(self.sp(0));
                } else {
                    self.ents.script_checks.upside_down.remove_car_from_check(self.sp(0));
                }
            }
            c::SET_CHAR_OBJ_WAIT_ON_FOOT | c::SET_CHAR_OBJ_FLEE_ON_FOOT_TILL_SAFE | c::SET_CHAR_OBJ_WAIT_IN_CAR => {
                self.collect(s, 1);
                let ped = self.objective_ped(self.sp(0));
                let o = match command {
                    c::SET_CHAR_OBJ_WAIT_ON_FOOT => objective::WAIT_ON_FOOT,
                    c::SET_CHAR_OBJ_FLEE_ON_FOOT_TILL_SAFE => objective::FLEE_CHAR_ON_FOOT_TILL_SAFE,
                    _ => objective::WAIT_IN_CAR,
                };
                self.set_objective(px, ped, o);
            }
            c::SET_CHAR_OBJ_GUARD_SPOT => {
                self.collect(s, 4);
                let ped = self.objective_ped(self.sp(0));
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                self.set_objective_pos(ped, objective::GUARD_SPOT, pos);
            }
            c::SET_CHAR_OBJ_GUARD_AREA | c::SET_CHAR_OBJ_GOTO_AREA_ON_FOOT => {
                self.collect(s, 5);
                let ped = self.objective_ped(self.sp(0));
                let (pos, radius) = self.guard_area_of((self.spf(1), self.spf(2)), (self.spf(3), self.spf(4)));
                let o = if command == c::SET_CHAR_OBJ_GUARD_AREA { objective::GUARD_SPOT } else { objective::GOTO_AREA_ON_FOOT };
                self.set_objective_area(ped, o, pos, radius);
            }
            c::SET_CHAR_OBJ_KILL_CHAR_ON_FOOT
            | c::SET_CHAR_OBJ_KILL_CHAR_ANY_MEANS
            | c::SET_CHAR_OBJ_FLEE_CHAR_ON_FOOT_TILL_SAFE
            | c::SET_CHAR_OBJ_FLEE_CHAR_ON_FOOT_ALWAYS
            | c::SET_CHAR_OBJ_GOTO_CHAR_ON_FOOT
            | c::SET_CHAR_OBJ_KILL_PLAYER_ON_FOOT
            | c::SET_CHAR_OBJ_KILL_PLAYER_ANY_MEANS
            | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_TILL_SAFE
            | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_ALWAYS
            | c::SET_CHAR_OBJ_GOTO_PLAYER_ON_FOOT => {
                self.collect(s, 2);
                let ped = self.objective_ped(self.sp(0));
                let by_player = matches!(
                    command,
                    c::SET_CHAR_OBJ_KILL_PLAYER_ON_FOOT | c::SET_CHAR_OBJ_KILL_PLAYER_ANY_MEANS | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_TILL_SAFE | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_ALWAYS | c::SET_CHAR_OBJ_GOTO_PLAYER_ON_FOOT
                );
                let target = if by_player { Some(self.the_player()) } else { self.script_ped(self.sp(1)) };
                let o = match command {
                    c::SET_CHAR_OBJ_KILL_CHAR_ON_FOOT | c::SET_CHAR_OBJ_KILL_PLAYER_ON_FOOT => objective::KILL_CHAR_ON_FOOT,
                    c::SET_CHAR_OBJ_KILL_CHAR_ANY_MEANS | c::SET_CHAR_OBJ_KILL_PLAYER_ANY_MEANS => objective::KILL_CHAR_ANY_MEANS,
                    c::SET_CHAR_OBJ_FLEE_CHAR_ON_FOOT_TILL_SAFE | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_TILL_SAFE => objective::FLEE_CHAR_ON_FOOT_TILL_SAFE,
                    c::SET_CHAR_OBJ_FLEE_CHAR_ON_FOOT_ALWAYS | c::SET_CHAR_OBJ_FLEE_PLAYER_ON_FOOT_ALWAYS => objective::FLEE_CHAR_ON_FOOT_ALWAYS,
                    _ => objective::GOTO_CHAR_ON_FOOT,
                };
                if let Some(t) = target {
                    self.set_objective_entity(px, ped, o, t);
                }
            }
            c::SET_CHAR_OBJ_LEAVE_CAR | c::SET_CHAR_OBJ_ENTER_CAR_AS_PASSENGER | c::SET_CHAR_OBJ_ENTER_CAR_AS_DRIVER | c::SET_CHAR_OBJ_DESTROY_CAR => {
                self.collect(s, 2);
                let ped = self.objective_ped(self.sp(0));
                let o = match command {
                    c::SET_CHAR_OBJ_LEAVE_CAR => objective::LEAVE_CAR,
                    c::SET_CHAR_OBJ_ENTER_CAR_AS_PASSENGER => objective::ENTER_CAR_AS_PASSENGER,
                    c::SET_CHAR_OBJ_ENTER_CAR_AS_DRIVER => objective::ENTER_CAR_AS_DRIVER,
                    _ => objective::DESTROY_CAR,
                };
                if let Some(car) = self.script_vehicle(self.sp(1)) {
                    self.set_objective_entity(px, ped, o, car);
                }
            }
            c::SET_CHAR_AS_LEADER | c::SET_PLAYER_AS_LEADER => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_*_AS_LEADER: no such ped");
                let target = if command == c::SET_PLAYER_AS_LEADER { Some(self.the_player()) } else { self.script_ped(self.sp(1)) };
                if let Some(t) = target {
                    self.set_objective_entity(px, ped, objective::SET_LEADER, t);
                }
            }
            c::LEAVE_GROUP => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("LEAVE_GROUP: no such ped");
                self.clear_leader(px, ped);
            }
            c::SET_CHAR_OBJ_FOLLOW_ROUTE => {
                self.collect(s, 3);
                let ped = self.objective_ped(self.sp(0));
                self.set_objective_route(ped, objective::FOLLOW_ROUTE, self.sp(1) as i16, self.sp(2) as i16);
            }
            c::ADD_ROUTE_POINT => {
                self.collect(s, 4);
                let pos = self.spv(1);
                self.ents.ped_routes.add_point(self.sp(0) as i16, pos);
            }
            c::GIVE_WEAPON_TO_PLAYER => {
                self.collect(s, 3);
                let p = self.the_player();
                let slot = self.give_weapon(p, self.sp(1) as usize, self.sp(2));
                if let Some(pl) = self.ped_mut(p).player.as_deref_mut() {
                    pl.selected_wep_slot = slot;
                }
            }
            c::GIVE_WEAPON_TO_CHAR => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("GIVE_WEAPON_TO_CHAR: no such ped");
                let slot = self.give_weapon(ped, self.sp(1) as usize, self.sp(2));
                self.set_current_weapon(ped, slot);
                if self.ped(ped).in_vehicle {
                    self.remove_weapon_model(ped);
                }
            }
            c::SET_CURRENT_PLAYER_WEAPON | c::SET_CURRENT_CHAR_WEAPON => {
                self.collect(s, 2);
                let ped = if command == c::SET_CURRENT_PLAYER_WEAPON { self.the_player() } else { self.script_ped(self.sp(0)).expect("SET_CURRENT_CHAR_WEAPON: no such ped") };
                let want = self.sp(1) as usize;
                for slot in 0..wt::TOTAL_INVENTORY {
                    if self.ped(ped).weapons[slot].ty == want {
                        if command == c::SET_CURRENT_PLAYER_WEAPON {
                            if let Some(pl) = self.ped_mut(ped).player.as_deref_mut() {
                                pl.selected_wep_slot = slot;
                            }
                        } else {
                            self.set_current_weapon(ped, slot);
                        }
                    }
                }
            }
            c::SET_PLAYER_CONTROL => {
                self.collect(s, 2);
                self.script_set_player_control(px, sh, s);
            }
            c::FORCE_WEATHER => {
                self.collect(s, 1);
                sh.force_weather(self.sp(0));
            }
            c::FORCE_WEATHER_NOW => {
                self.collect(s, 1);
                sh.force_weather_now(self.sp(0));
            }
            c::RELEASE_WEATHER => sh.release_weather(),
            c::GET_OBJECT_COORDINATES => {
                self.collect(s, 1);
                let obj = self.script_object(self.sp(0)).expect("GET_OBJECT_COORDINATES: no such object");
                let pos = self.epos(obj);
                self.ents.scripts.set_param_v3(0, pos);
                self.store(s, 3);
            }
            c::SET_OBJECT_COORDINATES => {
                self.collect(s, 4);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_COORDINATES: no such object");
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                self.teleport_object(obj, pos);
                px.hooks.clear_space_for_mission_entity(pos, obj);
            }
            c::GET_GAME_TIMER => {
                self.ents.scripts.params[0] = self.now() as i32;
                self.store(s, 1);
            }
            c::TURN_CHAR_TO_FACE_COORD | c::TURN_PLAYER_TO_FACE_COORD => {
                self.collect(s, 4);
                let ped = if command == c::TURN_PLAYER_TO_FACE_COORD { self.the_player() } else { self.script_ped(self.sp(0)).expect("TURN_CHAR_TO_FACE_COORD: no such ped") };
                self.turn_to_face_coord(ped);
            }
            c::STORE_WANTED_LEVEL => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = self.player_wanted().map_or(0, |w| w.level);
                self.store(s, 1);
            }
            c::IS_CAR_STOPPED => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_STOPPED: no such car");
                s.update_compare_flag(self.is_vehicle_stopped(car));
            }
            c::MARK_CHAR_AS_NO_LONGER_NEEDED => {
                self.collect(s, 1);
                if let Some(p) = self.script_ped(self.sp(0)) {
                    self.clean_up_this_ped(px, p);
                }
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_CHAR);
                }
            }
            c::MARK_CAR_AS_NO_LONGER_NEEDED => {
                self.collect(s, 1);
                if let Some(v) = self.script_vehicle(self.sp(0)) {
                    self.clean_up_this_vehicle(v);
                }
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_CAR);
                }
            }
            c::MARK_OBJECT_AS_NO_LONGER_NEEDED => {
                self.collect(s, 1);
                if let Some(o) = self.script_object(self.sp(0)) {
                    self.clean_up_this_object(o);
                }
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_OBJECT);
                }
            }
            c::DONT_REMOVE_CHAR | c::DONT_REMOVE_CAR | c::DONT_REMOVE_OBJECT => {
                self.collect(s, 1);
                let kind = match command {
                    c::DONT_REMOVE_CHAR => super::CLEANUP_CHAR,
                    c::DONT_REMOVE_CAR => super::CLEANUP_CAR,
                    _ => super::CLEANUP_OBJECT,
                };
                self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), kind);
            }
            c::PRINT_WITH_NUMBER_BIG | c::PRINT_WITH_NUMBER | c::PRINT_WITH_NUMBER_NOW => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 3);
                let (number, time, flag) = (self.sp(0), self.sp(1) as u32, self.sp(2));
                match command {
                    c::PRINT_WITH_NUMBER_BIG => sh.print_big_with_number(&key, time, flag - 1, number),
                    c::PRINT_WITH_NUMBER => sh.print_with_number(&key, time, flag, number),
                    _ => sh.print_now_with_number(&key, time, flag, number),
                }
            }
            c::SWITCH_ROADS_ON | c::SWITCH_ROADS_OFF => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                self.ents.paths.switch_roads_off_in_area(a.min(b), a.max(b), command == c::SWITCH_ROADS_OFF);
            }
            c::GET_NUMBER_OF_PASSENGERS | c::GET_MAXIMUM_NUMBER_OF_PASSENGERS => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("passenger count: no such car");
                let v = self.try_veh(car).unwrap();
                self.ents.scripts.params[0] = if command == c::GET_NUMBER_OF_PASSENGERS { v.num_passengers } else { v.num_max_passengers } as i32;
                self.store(s, 1);
            }
            c::SET_CAR_DENSITY_MULTIPLIER => {
                self.collect(s, 1);
                self.ents.car_ctrl.car_density_multiplier = self.spf(0);
            }
            c::CLEAR_CHAR_THREAT_SEARCH => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("CLEAR_CHAR_THREAT_SEARCH: no such ped");
                self.ped_mut(ped).fear_flags = 0;
            }
            c::ACTIVATE_CRANE => {
                self.collect(s, 10);
                let (a, b) = ((self.spf(2), self.spf(3)), (self.spf(4), self.spf(5)));
                let drop = Vec3::new(self.spf(6), self.spf(7), self.spf(8));
                let (heading, px_, py_) = (self.spf(9).to_radians(), self.spf(0), self.spf(1));
                self.activate_crane(a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1), drop, heading, false, false, px_, py_);
            }
            c::DEACTIVATE_CRANE => {
                self.collect(s, 2);
                self.deactivate_crane(self.spf(0), self.spf(1));
            }
            c::SET_MAX_WANTED_LEVEL => {
                self.collect(s, 1);
                let level = self.sp(0);
                if let Some(w) = self.player_wanted_mut() {
                    w.set_maximum_wanted_level(level);
                }
            }
            c::IS_CAR_IN_AIR_PROPER => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_IN_AIR_PROPER: no such car");
                // the original's fix: the unique stunt jump script may not wait for a Dodo that is never "in the air"
                let usj_with_dodo = s.name_str() == "usj" && self.ent(car).base.model_index == crate::modelinfo::indices_gta3::mi::DODO;
                let none = self.ent(car).phys.as_ref().is_some_and(|p| p.collision_records.is_empty());
                s.update_compare_flag(none && !usj_with_dodo);
            }
            c::CREATE_CHAR_AS_PASSENGER => {
                self.collect(s, 4);
                let seat = (self.sp(3) >= 0).then(|| self.sp(3) as u8);
                self.script_create_char_in_car(px, s, Some(seat));
            }
            c::SET_CAR_HEAVY => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_HEAVY: no such car");
                let on = self.sp(1) != 0;
                self.ent_mut(car).phys.as_mut().expect("a car is physical").flags.is_heavy = on;
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// `COMMAND_SET_PLAYER_CONTROL`: control back after a short delay at the start, or taken away.
    fn script_set_player_control(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &RunningScript) {
        if self.sp(1) != 0 {
            let sc = &mut self.ents.scripts;
            if sh.playing_intro() || sc.delay_making_player_unsafe_this_time != 0 {
                sc.countdown_to_make_player_unsafe = 50;
                sc.delay_making_player_unsafe_this_time = sc.delay_making_player_unsafe_this_time.saturating_sub(1);
            } else {
                self.make_player_safe(px, false);
            }
        } else {
            self.make_player_safe(px, true);
            if s.name_str() == "camera" {
                let p = self.the_player();
                if let Some(ph) = self.get_mut(p).and_then(|e| e.phys.as_mut()) {
                    ph.move_speed = Vec3::ZERO;
                    ph.turn_speed = Vec3::ZERO;
                }
                sh.player_idle_for_camera_script();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::data::MISSION_CHAR;
    use crate::ped::types::ped_type;
    use crate::vehicle::base::{MISSION_VEHICLE, RANDOM_VEHICLE};
    use crate::world::tests::LANDSTAL_MODEL;

    #[derive(Default)]
    struct Spy {
        weather: Vec<i32>,
        housekeeping: u32,
        numbered: Vec<(String, u32, i32, i32)>,
    }
    impl ScriptHooks for Spy {
        fn force_weather(&mut self, w: i32) {
            self.weather.push(w);
        }
        fn end_of_mission_housekeeping(&mut self) {
            self.housekeeping += 1;
        }
        fn print_with_number(&mut self, key: &str, time: u32, flags: i32, number: i32) {
            self.numbered.push((key.to_owned(), time, flags, number));
        }
    }

    fn world(a: &Asm) -> World {
        let mut w = world_with_script(a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(5.0, 5.0, 0.0));
        w
    }

    fn civilian(w: &mut World) -> EntityRef {
        w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap()
    }

    fn g(w: &World, o: u32) -> i32 {
        w.ents.scripts.global(o)
    }

    #[test]
    fn the_stopped_and_on_foot_area_tests() {
        let mut a = Asm::new(2);
        a.cmd(c::IS_PLAYER_IN_AREA_ON_FOOT_2D, &[A::I8(0), A::F(10.0), A::F(10.0), A::F(0.0), A::F(0.0), A::I8(0)]);
        let j = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::IS_PLAYER_STOPPED_IN_AREA_3D, &[A::I8(0), A::F(0.0), A::F(0.0), A::F(-5.0), A::F(10.0), A::F(10.0), A::F(5.0), A::I8(0)]);
        let end = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[j..j + 4].copy_from_slice(&end.to_le_bytes());
        let mut w = world(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert!(w.ents.scripts.scripts[i].cond_result, "inside, on foot, standing still");
    }

    #[test]
    fn objectives_weapons_and_the_player_control_countdown() {
        let mut a = Asm::new(4);
        a.cmd(c::SET_CHAR_OBJ_GUARD_SPOT, &[A::G(8), A::F(1.0), A::F(2.0), A::F(0.0)]);
        a.cmd(c::GIVE_WEAPON_TO_PLAYER, &[A::I8(0), A::I8(2), A::I8(40)]);
        a.cmd(c::GIVE_WEAPON_TO_CHAR, &[A::G(8), A::I8(3), A::I8(100)]);
        a.cmd(c::SET_PLAYER_CONTROL, &[A::I8(0), A::I8(1)]);
        a.cmd(c::FORCE_WEATHER, &[A::I8(4)]);
        a.cmd(c::GET_GAME_TIMER, &[A::G(12)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        w.ents.timer.time_in_ms = 777;
        let ped = civilian(&mut w);
        w.ents.scripts.set_global(8, ped.handle);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(w.ped(ped).objective, objective::GUARD_SPOT);
        assert_eq!(w.ped(ped).weapons[3].ammo_total, 100);
        let p = w.ents.player_ped.unwrap();
        assert_eq!((w.ped(p).weapons[2].ammo_total, w.ped(p).player.as_deref().unwrap().selected_wep_slot), (40, 2));
        // the first time control is given back it is delayed 50 frames
        assert_eq!((w.ents.scripts.countdown_to_make_player_unsafe, w.ents.scripts.delay_making_player_unsafe_this_time), (50, 0));
        assert_eq!((spy.weather, g(&w, 12)), (vec![4], 777));
    }

    #[test]
    fn a_marked_mission_car_becomes_an_ordinary_one_and_the_mission_clean_up_does_it_for_the_rest() {
        let mut a = Asm::new(4);
        a.cmd(c::MARK_CAR_AS_NO_LONGER_NEEDED, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let mk = |w: &mut World| {
            let car = w.new_car(LANDSTAL_MODEL, MISSION_VEHICLE, &mut Rng::default()).unwrap();
            w.try_veh_mut(car).unwrap().flags.is_locked = true;
            car
        };
        let (marked, left) = (mk(&mut w), mk(&mut w));
        w.ents.scripts.set_global(8, marked.handle);
        w.ents.scripts.cleanup.add_entity_to_list(marked.handle, super::super::CLEANUP_CAR);
        w.ents.scripts.cleanup.add_entity_to_list(left.handle, super::super::CLEANUP_CAR);
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy::default());
        assert_eq!((w.try_veh(marked).unwrap().created_by, w.try_veh(marked).unwrap().flags.is_locked), (RANDOM_VEHICLE, false));
        assert_eq!((w.try_veh(left).unwrap().created_by, w.ents.scripts.cleanup.count), (MISSION_VEHICLE, 1));
        // the mission ends
        w.ents.car_ctrl.car_density_multiplier = 3.0;
        w.ents.onscreen_timer.disabled = true;
        w.ents.scripts.store_vehicle_index = 5;
        let mut spy = Spy::default();
        w.process_mission_cleanup(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, &mut spy);
        assert_eq!((w.try_veh(left).unwrap().created_by, w.ents.scripts.cleanup.count), (RANDOM_VEHICLE, 0));
        assert_eq!((w.ents.car_ctrl.car_density_multiplier, w.ents.onscreen_timer.disabled, w.ents.scripts.store_vehicle_index, spy.housekeeping), (1.0, false, -1, 1));
    }

    #[test]
    fn a_mission_ped_goes_back_to_ordinary_and_the_count_drops() {
        let mut w = world(&Asm::new(0));
        let ped = civilian(&mut w);
        w.ped_mut(ped).char_created_by = MISSION_CHAR;
        w.ped_mut(ped).flags.responds_to_threats = false;
        w.ents.population.total_mission_peds = 1;
        w.clean_up_this_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped);
        assert_eq!((w.ped(ped).char_created_by, w.ped(ped).flags.responds_to_threats, w.ents.population.total_mission_peds), (crate::ped::data::RANDOM_CHAR, true, 0));
        // an ordinary ped is left alone
        w.clean_up_this_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped);
        assert_eq!(w.ents.population.total_mission_peds, 0);
    }

    #[test]
    fn route_points_roads_passengers_and_numbered_messages() {
        let mut a = Asm::new(4);
        a.cmd(c::ADD_ROUTE_POINT, &[A::I8(3), A::F(1.0), A::F(2.0), A::F(3.0)]);
        a.cmd(c::GET_MAXIMUM_NUMBER_OF_PASSENGERS, &[A::G(8), A::G(12)]);
        a.bytes.extend_from_slice(&c::PRINT_WITH_NUMBER.to_le_bytes());
        a.bytes.extend_from_slice(b"HELLO\0\0\0");
        a.bytes.extend_from_slice(&[4, 9, 5, 0xB8, 0x0B, 4, 1]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.ents.scripts.set_global(8, car.handle);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(w.ents.ped_routes.point_position(w.ents.ped_routes.route_start(3)), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(g(&w, 12), w.try_veh(car).unwrap().num_max_passengers as i32);
        assert_eq!(spy.numbered, vec![("HELLO".to_owned(), 3000, 1, 9)]);
    }
}
