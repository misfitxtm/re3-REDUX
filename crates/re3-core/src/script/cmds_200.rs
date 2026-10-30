//! `Script.cpp` commands 200 to 299 that stand on their own: AND/OR, tests of what the player and chars are in, score, wanted level and ammo.
use super::commands as c;
use super::{andor, RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::ped::ctx::Px;
use crate::ped::data::state as ped_state;
use crate::player_info::wb_state;
use crate::world::World;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// The player whose slot a script names; only slot 0 exists.
    pub(crate) fn script_player(&self) -> crate::entity::EntityRef {
        self.ents.player_ped.expect("script needs a player")
    }

    /// The car a mission script has stored a handle of: the one before it is let go if the script made it a mission car.
    fn store_car_in(&mut self, s: &RunningScript, car: EntityRef) {
        use crate::vehicle::base::{MISSION_VEHICLE, PARKED_VEHICLE, PERMANENT_VEHICLE, RANDOM_VEHICLE};
        let handle = car.handle;
        if handle != self.ents.scripts.store_vehicle_index && s.is_mission_script {
            if let Some(old) = self.script_vehicle(self.ents.scripts.store_vehicle_index) {
                self.ents.car_ctrl.remove_from_interesting_vehicle_list(old);
                if self.try_veh(old).is_some_and(|v| v.created_by == MISSION_VEHICLE) && self.ents.scripts.store_vehicle_was_random {
                    let v = self.try_veh_mut(old).unwrap();
                    v.created_by = RANDOM_VEHICLE;
                    v.flags.is_locked = false;
                    self.ents.car_ctrl.num_random_cars += 1;
                    self.ents.car_ctrl.num_mission_cars -= 1;
                    self.ents.scripts.cleanup.remove_entity_from_list(self.ents.scripts.store_vehicle_index, super::CLEANUP_CAR);
                }
            }
            self.ents.scripts.store_vehicle_index = handle;
            match self.try_veh(car).unwrap().created_by {
                RANDOM_VEHICLE | PARKED_VEHICLE => {
                    let was_random = self.try_veh(car).unwrap().created_by == RANDOM_VEHICLE;
                    self.try_veh_mut(car).unwrap().created_by = MISSION_VEHICLE;
                    let cc = &mut self.ents.car_ctrl;
                    cc.num_mission_cars += 1;
                    if was_random {
                        cc.num_random_cars -= 1;
                    } else {
                        cc.num_parked_cars -= 1;
                    }
                    self.ents.scripts.store_vehicle_was_random = true;
                    self.ents.scripts.cleanup.add_entity_to_list(handle, super::CLEANUP_CAR);
                }
                MISSION_VEHICLE | PERMANENT_VEHICLE => self.ents.scripts.store_vehicle_was_random = false,
                _ => {}
            }
        }
    }

    /// `COMMAND_CREATE_OBJECT`: a model by number, or by its place in the script's own name table when negative.
    fn script_create_object(&mut self, px: &mut Px, s: &mut RunningScript) {
        let wanted = self.sp(0);
        let model = if wanted >= 0 { wanted } else { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1) } as i16;
        let Some(obj) = self.new_object(model, crate::math::Matrix::default(), px.rng) else { return };
        if let Some(crate::entity::Kind::Object(o)) = self.get_mut(obj).map(|e| &mut e.kind) {
            o.created_by = crate::entity::object::CreatedBy::Mission;
        }
        let mut pos = self.spv(1);
        if pos.z <= -100.0 {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        pos.z += self.base_offset_of(obj);
        let e = self.get_mut(obj).unwrap();
        e.base.set_position(pos);
        e.base.set_orientation(0.0, 0.0, 0.0);
        px.hooks.clear_space_for_mission_entity(pos, obj);
        let _ = self.add(obj);
        self.ents.scripts.params[0] = obj.handle;
        self.store(s, 1);
        if s.is_mission_script {
            self.ents.scripts.cleanup.add_entity_to_list(obj.handle, super::CLEANUP_OBJECT);
        }
    }

    /// `Some(car)` when the ped is in one.
    fn car_of_ped(&self, ped: crate::entity::EntityRef) -> Option<crate::entity::EntityRef> {
        let pd = self.ped(ped);
        pd.my_vehicle.filter(|_| pd.in_vehicle)
    }

    pub(crate) fn process_commands_200_to_299(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::ANDOR => {
                self.collect(s, 1);
                s.and_or_state = self.sp(0) as u16;
                let st = s.and_or_state;
                if st == andor::NONE {
                    s.cond_result = false;
                } else if (andor::ANDS_1..=andor::ANDS_8).contains(&st) {
                    s.cond_result = true;
                    s.and_or_state += 1;
                } else if (andor::ORS_1..=andor::ORS_8).contains(&st) {
                    s.cond_result = false;
                    s.and_or_state += 1;
                } else {
                    panic!("ANDOR: invalid state {st}");
                }
            }
            c::MISSION_HAS_FINISHED => {
                if s.is_mission_script {
                    if s.name_str() == "love3" {
                        self.remove_all_floating_pickups();
                    }
                    self.process_mission_cleanup(px, sh);
                }
            }
            c::STORE_CAR_CHAR_IS_IN | c::STORE_CAR_PLAYER_IS_IN => {
                self.collect(s, 1);
                let ped = if command == c::STORE_CAR_PLAYER_IS_IN { self.script_player() } else { self.script_ped(self.sp(0)).expect("STORE_CAR_CHAR_IS_IN: no such ped") };
                match self.car_of_ped(ped) {
                    Some(car) => {
                        self.store_car_in(s, car);
                        self.ents.scripts.params[0] = self.ents.scripts.store_vehicle_index;
                        self.store(s, 1);
                    }
                    // the player command writes nothing when the player is on foot
                    None => assert!(command == c::STORE_CAR_PLAYER_IS_IN, "STORE_CAR_CHAR_IS_IN: the char is not in a car"),
                }
            }
            c::CREATE_OBJECT => {
                self.collect(s, 4);
                self.script_create_object(px, s);
            }
            c::DELETE_OBJECT => {
                self.collect(s, 1);
                if let Some(o) = self.script_object(self.sp(0)) {
                    self.remove_references_to(o);
                    self.delete(o);
                }
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_OBJECT);
                }
            }
            c::LAUNCH_MISSION => {
                self.collect(s, 1);
                let new = self.ents.scripts.start_new_script(self.sp(0) as u32);
                self.ents.scripts.scripts[new].is_mission_script = true;
            }
            c::IS_CHAR_IN_CAR => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_CAR: no such ped");
                let (actual, checked) = (self.car_of_ped(ped), self.script_vehicle(self.sp(1)));
                s.update_compare_flag(actual.is_some() && actual == checked);
            }
            c::IS_PLAYER_IN_CAR => {
                self.collect(s, 2);
                let (actual, checked) = (self.car_of_ped(self.script_player()), self.script_vehicle(self.sp(1)));
                s.update_compare_flag(actual.is_some() && actual == checked);
            }
            c::IS_CHAR_IN_MODEL | c::IS_PLAYER_IN_MODEL => {
                self.collect(s, 2);
                let ped = if command == c::IS_CHAR_IN_MODEL { self.script_ped(self.sp(0)).expect("IS_CHAR_IN_MODEL: no such ped") } else { self.script_player() };
                let model = self.sp(1);
                s.update_compare_flag(self.car_of_ped(ped).is_some_and(|v| self.ent(v).base.model_index as i32 == model));
            }
            c::IS_CHAR_IN_ANY_CAR => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_ANY_CAR: no such ped");
                s.update_compare_flag(self.ped(ped).in_vehicle);
            }
            c::IS_PLAYER_IN_ANY_CAR => {
                self.collect(s, 1);
                s.update_compare_flag(self.ped(self.script_player()).in_vehicle);
            }
            c::IS_BUTTON_PRESSED => {
                self.collect(s, 2);
                let value = sh.pad_state(self.sp(0), self.sp(1)) != 0;
                s.update_compare_flag(value || sh.intro_skip_pressed(self.sp(0), self.sp(1)));
            }
            c::GET_PAD_STATE => {
                self.collect(s, 2);
                self.ents.scripts.params[0] = i32::from(sh.pad_state(self.sp(0), self.sp(1)));
                self.store(s, 1);
            }
            c::ADD_SCORE => {
                self.collect(s, 2);
                self.ents.player_info.money += self.sp(1);
            }
            c::IS_SCORE_GREATER => {
                self.collect(s, 2);
                s.update_compare_flag(self.ents.player_info.money > self.sp(1));
            }
            c::STORE_SCORE => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = self.ents.player_info.money;
                self.store(s, 1);
            }
            c::ALTER_WANTED_LEVEL | c::ALTER_WANTED_LEVEL_NO_DROP | c::CLEAR_WANTED_LEVEL => {
                let level = if command == c::CLEAR_WANTED_LEVEL {
                    self.collect(s, 1);
                    0
                } else {
                    self.collect(s, 2);
                    self.sp(1)
                };
                let now = self.now();
                if let Some(w) = self.player_wanted_mut() {
                    if command == c::ALTER_WANTED_LEVEL_NO_DROP {
                        w.set_wanted_level_no_drop(level, now);
                    } else {
                        w.set_wanted_level(level, now);
                    }
                }
            }
            c::IS_WANTED_LEVEL_GREATER => {
                self.collect(s, 2);
                let level = self.player_wanted().map_or(0, |w| w.level);
                s.update_compare_flag(level > self.sp(1));
            }
            c::SET_DEATHARREST_STATE => {
                self.collect(s, 1);
                s.deatharrest_enabled = self.sp(0) == 1;
            }
            c::HAS_DEATHARREST_BEEN_EXECUTED => s.update_compare_flag(s.deatharrest_executed),
            c::ADD_AMMO_TO_PLAYER => {
                self.collect(s, 3);
                let p = self.script_player();
                self.grant_ammo(p, self.sp(1) as usize, self.sp(2));
            }
            c::ADD_AMMO_TO_CHAR => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("ADD_AMMO_TO_CHAR: no such ped");
                self.grant_ammo(ped, self.sp(1) as usize, self.sp(2));
            }
            c::IS_PLAYER_DEAD => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.player_info.wb_state == wb_state::WASTED);
            }
            c::IS_CHAR_DEAD => {
                self.collect(s, 1);
                let dead = self.script_ped(self.sp(0)).is_none_or(|p| matches!(self.ped(p).ped_state, ped_state::DIE | ped_state::DEAD));
                s.update_compare_flag(dead);
            }
            c::IS_CAR_DEAD => {
                self.collect(s, 1);
                let dead = self.script_vehicle(self.sp(0)).is_none_or(|v| {
                    let in_water = self.ent(v).phys.as_ref().is_some_and(|p| p.flags.is_in_water);
                    self.ent(v).base.status == crate::entity::status::WRECKED || !self.try_veh(v).is_some_and(|d| d.is_boat()) && in_water
                });
                s.update_compare_flag(dead);
            }
            c::SET_CHAR_THREAT_SEARCH => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_THREAT_SEARCH: no such ped");
                self.ped_mut(ped).fear_flags |= self.sp(1) as u32;
            }
            c::SET_CHAR_OBJ_NO_OBJ => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_NO_OBJ: no such ped");
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.clear_objective(px, ped);
            }
            c::IS_PLAYER_IN_ZONE => {
                self.collect(s, 1);
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let pos = self.player_info_pos();
                let zones = &mut self.ents.zones;
                let inside = zones.find_zone_by_label_and_return_index(&label).is_some_and(|z| crate::zones::point_lies_within_zone(pos, &zones.zone_array[z]));
                s.update_compare_flag(inside);
            }
            c::IS_PLAYER_PRESSING_HORN => {
                self.collect(s, 1);
                let driving = self.ped(self.script_player()).ped_state == ped_state::DRIVING;
                s.update_compare_flag(driving && sh.horn_pressed(self.sp(0)));
            }
            c::HAS_CHAR_SPOTTED_PLAYER => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("HAS_CHAR_SPOTTED_PLAYER: no such ped");
                let player = self.script_player();
                let seen = self.our_ped_can_see_this_one(ped, player);
                s.update_compare_flag(seen);
            }
            c::IS_CHAR_OBJECTIVE_PASSED => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_OBJECTIVE_PASSED: no such ped");
                s.update_compare_flag(self.ped(ped).flags.script_objective_completed);
            }
            c::GIVE_REMOTE_CONTROLLED_CAR_TO_PLAYER => {
                self.collect(s, 5);
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let heading = self.spf(4).to_radians();
                self.give_player_remote_controlled_car(px, pos.x, pos.y, pos.z, heading, crate::modelinfo::indices_gta3::mi::RCBANDIT);
            }
            c::WARP_PLAYER_FROM_CAR_TO_COORD => {
                self.collect(s, 4);
                let player = self.script_player();
                self.warp_ped_from_car_to_coord(px, player, self.spv(1), false);
            }
            c::CREATE_CHAR_INSIDE_CAR => {
                self.collect(s, 3);
                self.script_create_char_in_car(px, s, None);
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// `CREATE_CHAR_INSIDE_CAR` (`seat` is `None`: the driver) and `CREATE_CHAR_AS_PASSENGER` (a seat, or any free one).
    pub(crate) fn script_create_char_in_car(&mut self, px: &mut Px, s: &mut RunningScript, seat: Option<Option<u8>>) {
        let car = self.script_vehicle(self.sp(0)).expect("CREATE_CHAR_IN_CAR: no such car");
        let Some(ped) = self.script_new_ped(px, self.sp(1) as usize, self.sp(2)) else { return };
        let pd = self.ped_mut(ped);
        pd.char_created_by = crate::ped::data::MISSION_CHAR;
        pd.flags.responds_to_threats = false;
        pd.flags.allow_medics_to_revive_me = false;
        pd.ped_state = crate::ped::data::state::DRIVING;
        let at = self.epos(car);
        let e = self.ent_mut(ped);
        e.base.set_position(at);
        e.base.set_orientation(0.0, 0.0, 0.0);
        self.ents.population.total_mission_peds += 1;
        match seat {
            None => {
                assert!(self.try_veh(car).unwrap().driver.is_none(), "CREATE_CHAR_INSIDE_CAR: the car has a driver");
                self.vehicle_set_driver(car, ped);
            }
            Some(seat) => {
                self.vehicle_add_passenger(car, ped, seat);
            }
        }
        let pd = self.ped_mut(ped);
        (pd.my_vehicle, pd.in_vehicle) = (Some(car), true);
        self.ent_mut(car).base.status = crate::entity::status::PHYSICS;
        let boat = self.ents.indices.is_boat_model(self.ent(car).base.model_index);
        if seat.is_none() {
            let v = self.try_veh_mut(car).unwrap();
            if !boat {
                v.auto_pilot.car_mission = crate::vehicle::base::car_mission::CRUISE;
            }
            v.flags.engine_on = true;
        }
        self.ent_mut(ped).base.flags.uses_collision = false;
        let anim = self.driver_anim(car);
        self.ped_blend_anim(ped, crate::anim::assoc_group::STD, anim, 100.0);
        self.ped_mut(ped).vehicle_anim = Some(anim as i32);
        self.stop_non_partial_anims(ped);
        let level = self.ents.zones.get_level_from_position(at);
        if let Some(p) = self.ent_mut(ped).phys.as_mut() {
            p.zone_level = level as i8;
        }
        let _ = self.add(ped);
        self.ents.scripts.params[0] = ped.handle;
        self.store(s, 1);
        if s.is_mission_script {
            self.ents.scripts.cleanup.add_entity_to_list(ped.handle, super::CLEANUP_CHAR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;
    use crate::world::tests::LANDSTAL_MODEL;
    use glam::Vec3;

    #[derive(Default)]
    struct Spy;
    impl ScriptHooks for Spy {
        fn pad_state(&mut self, pad: i32, button: i32) -> i16 {
            if (pad, button) == (0, 16) { 255 } else { 0 }
        }
    }

    fn world_with_player(a: &Asm) -> World {
        let mut w = world_with_script(a);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w.get_mut(p).unwrap().base.set_position(Vec3::new(5.0, 5.0, 0.0));
        w
    }

    fn g(w: &World, o: u32) -> i32 {
        w.ents.scripts.global(o)
    }

    #[test]
    fn and_and_or_chains_join_the_tests_that_follow() {
        let mut a = Asm::new(4);
        // IF AND 2: 1 > 0 and 5 > 9 is false
        a.cmd(c::ANDOR, &[A::I8(1)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(8), A::I8(-1)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(8), A::I8(9)]);
        let j1 = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::SET_VAR_INT, &[A::G(12), A::I8(1)]);
        let l1 = a.here();
        // IF OR 2: the second is true
        a.cmd(c::ANDOR, &[A::I8(21)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(8), A::I8(9)]);
        a.cmd(c::IS_INT_VAR_GREATER_THAN_NUMBER, &[A::G(8), A::I8(-1)]);
        let j2 = a.here() as usize + 3;
        a.cmd(c::GOTO_IF_FALSE, &[A::I32(0)]);
        a.cmd(c::SET_VAR_INT, &[A::G(16), A::I8(1)]);
        let l2 = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[j1..j1 + 4].copy_from_slice(&l1.to_le_bytes());
        a.bytes[j2..j2 + 4].copy_from_slice(&l2.to_le_bytes());
        let mut w = world_with_script(&a);
        w.ents.scripts.set_global(8, 5);
        start(&mut w);
        run_frame(&mut w, &mut Spy);
        assert_eq!((g(&w, 12), g(&w, 16)), (0, 1));
    }

    #[test]
    fn score_wanted_level_ammo_and_the_state_of_the_player() {
        let mut a = Asm::new(6);
        a.cmd(c::ADD_SCORE, &[A::I8(0), A::I16(500)]);
        a.cmd(c::STORE_SCORE, &[A::I8(0), A::G(8)]);
        a.cmd(c::ALTER_WANTED_LEVEL_NO_DROP, &[A::I8(0), A::I8(3)]);
        a.cmd(c::IS_WANTED_LEVEL_GREATER, &[A::I8(0), A::I8(2)]);
        a.cmd(c::GET_PAD_STATE, &[A::I8(0), A::I8(16), A::G(12)]);
        a.cmd(c::SET_DEATHARREST_STATE, &[A::I8(0)]);
        a.cmd(c::ADD_AMMO_TO_PLAYER, &[A::I8(0), A::I8(2), A::I8(30)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy);
        assert_eq!((g(&w, 8), g(&w, 12)), (500, 255));
        assert_eq!(w.player_wanted().unwrap().level, 3);
        assert!(w.ents.scripts.scripts[i].cond_result);
        assert!(!w.ents.scripts.scripts[i].deatharrest_enabled);
        let p = w.ents.player_ped.unwrap();
        assert_eq!(w.ped(p).weapons[2].ammo_total, 30);
    }

    #[test]
    fn what_a_char_or_the_player_is_in() {
        let mut a = Asm::new(4);
        a.cmd(c::IS_PLAYER_IN_ANY_CAR, &[A::I8(0)]);
        a.cmd(c::IS_PLAYER_IN_MODEL, &[A::I8(0), A::I16(LANDSTAL_MODEL)]);
        a.cmd(c::IS_PLAYER_IN_CAR, &[A::I8(0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let car = w.new_car(LANDSTAL_MODEL, crate::vehicle::base::RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        let p = w.ents.player_ped.unwrap();
        let pd = w.ped_mut(p);
        (pd.in_vehicle, pd.my_vehicle) = (true, Some(car));
        w.ents.scripts.set_global(8, car.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy);
        assert!(w.ents.scripts.scripts[i].cond_result, "in that very car");
    }

    #[test]
    fn dead_chars_and_cars_gone_from_the_pool_count_as_dead() {
        let mut a = Asm::new(4);
        a.cmd(c::IS_CHAR_DEAD, &[A::G(8)]);
        a.cmd(c::IS_CAR_DEAD, &[A::G(12)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        w.ents.scripts.set_global(8, 12345);
        w.ents.scripts.set_global(12, 54321);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy);
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn a_launched_mission_is_a_mission_script_and_the_player_zone_test_reads_its_label() {
        let mut a = Asm::new(2);
        let mission_at = a.here() as usize + 3;
        a.cmd(c::LAUNCH_MISSION, &[A::I32(0)]);
        a.cmd(c::IS_PLAYER_IN_ZONE, &[A::I8(0)]);
        a.bytes.extend_from_slice(b"CITYZON\0");
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let m = a.here();
        a.cmd(c::WAIT, &[A::I32(100000)]);
        a.bytes[mission_at..mission_at + 4].copy_from_slice(&m.to_le_bytes());
        let mut w = world_with_player(&a);
        let parent = start(&mut w);
        run_frame(&mut w, &mut Spy);
        let child = w.ents.scripts.active_scripts()[0];
        assert_ne!(child, parent);
        assert!(w.ents.scripts.scripts[child].is_mission_script && w.ents.scripts.scripts[child].ip == m as u32);
        assert!(w.ents.scripts.scripts[parent].cond_result, "the player is inside the whole-city zone");
    }

    #[test]
    fn the_car_a_mission_script_stores_becomes_a_mission_car_and_the_one_before_is_released() {
        use crate::vehicle::base::{MISSION_VEHICLE, RANDOM_VEHICLE};
        let mut a = Asm::new(2);
        a.cmd(c::STORE_CAR_PLAYER_IS_IN, &[A::I8(0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(1)]);
        a.cmd(c::STORE_CAR_PLAYER_IS_IN, &[A::I8(0), A::G(12)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let mut rng = Rng::default();
        let (first, second) = (w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap(), w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap());
        w.ents.car_ctrl.num_random_cars = 2;
        let p = w.ents.player_ped.unwrap();
        let pd = w.ped_mut(p);
        (pd.in_vehicle, pd.my_vehicle) = (true, Some(first));
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy);
        assert_eq!((g(&w, 8), w.try_veh(first).unwrap().created_by, w.ents.car_ctrl.num_mission_cars, w.ents.car_ctrl.num_random_cars), (first.handle, MISSION_VEHICLE, 1, 1));
        let pd = w.ped_mut(p);
        pd.my_vehicle = Some(second);
        w.ents.timer.time_in_ms = 10;
        run_frame(&mut w, &mut Spy);
        assert_eq!((g(&w, 12), w.try_veh(first).unwrap().created_by, w.try_veh(second).unwrap().created_by), (second.handle, RANDOM_VEHICLE, MISSION_VEHICLE));
        assert_eq!(w.ents.scripts.cleanup.count, 1);
    }

    #[test]
    fn objects_are_made_by_number_or_by_the_scripts_own_names_and_deleted() {
        use crate::collision::{Box3, ColModel, Sphere};
        let mut a = Asm::new(2);
        a.cmd(c::CREATE_OBJECT, &[A::I16(3000), A::F(4.0), A::F(5.0), A::F(1.0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(1)]);
        a.cmd(c::DELETE_OBJECT, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let b = Box3 { min: Vec3::splat(-1.0), max: Vec3::splat(1.0), surface: 0, piece: 0 };
        w.ents.models.add_simple_model(3000).unwrap().col_model = Some(std::sync::Arc::new(ColModel { bounding_sphere: Sphere::new(1.7, Vec3::ZERO, 0, 0), bounding_box: b, boxes: vec![b], ..Default::default() }));
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy);
        let obj = crate::entity::EntityRef { pool: crate::entity::PoolKind::Object, handle: g(&w, 8) };
        assert_eq!((w.epos(obj), w.ents.scripts.cleanup.count), (Vec3::new(4.0, 5.0, 2.0), 1), "its centre is a metre above where it stands");
        assert!(matches!(&w.ent(obj).kind, crate::entity::Kind::Object(o) if o.created_by == crate::entity::object::CreatedBy::Mission));
        w.ents.timer.time_in_ms = 10;
        run_frame(&mut w, &mut Spy);
        assert!(w.get(obj).is_none() && w.ents.scripts.cleanup.count == 0);
    }

    #[test]
    fn a_finished_mission_cleans_up_after_itself() {
        let mut a = Asm::new(2);
        a.cmd(c::MISSION_HAS_FINISHED, &[]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        w.ents.car_ctrl.car_density_multiplier = 4.0;
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy);
        assert_eq!(w.ents.car_ctrl.car_density_multiplier, 1.0);
    }

    #[test]
    fn a_char_made_inside_a_car_drives_it_and_one_made_as_a_passenger_takes_a_seat() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CHAR_INSIDE_CAR, &[A::G(8), A::I8(crate::ped::types::ped_type::CIVMALE as i8), A::I8(-1), A::G(12)]);
        a.cmd(c::CREATE_CHAR_AS_PASSENGER, &[A::G(8), A::I8(crate::ped::types::ped_type::CIVMALE as i8), A::I8(-1), A::I8(-1), A::G(16)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let car = w.new_car(crate::world::tests::LANDSTAL_MODEL, crate::vehicle::base::RANDOM_VEHICLE, &mut crate::general::Rng::default()).unwrap();
        w.ents.scripts.set_global(8, car.handle);
        start(&mut w);
        run_frame(&mut w, &mut Spy);
        let (driver, passenger) = (w.script_ped(g(&w, 12)).unwrap(), w.script_ped(g(&w, 16)).unwrap());
        let v = w.try_veh(car).unwrap();
        assert_eq!((v.driver, v.passengers[0], v.flags.engine_on), (Some(driver), Some(passenger), true));
        assert_eq!(w.ent(car).base.status, crate::entity::status::PHYSICS);
        for p in [driver, passenger] {
            let pd = w.ped(p);
            assert_eq!((pd.in_vehicle, pd.my_vehicle, pd.ped_state, pd.char_created_by), (true, Some(car), ped_state::DRIVING, crate::ped::data::MISSION_CHAR));
        }
        assert_eq!(w.ents.population.total_mission_peds, 2);
    }
}
