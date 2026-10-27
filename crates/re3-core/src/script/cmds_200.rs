//! `Script.cpp` commands 200 to 299 that stand on their own: AND/OR, tests of what the player and chars are in, score, wanted level and ammo.
use super::commands as c;
use super::{andor, RunningScript, ScriptHooks};
use crate::ped::ctx::Px;
use crate::ped::data::state as ped_state;
use crate::player_info::wb_state;
use crate::world::World;

impl World {
    /// The player whose slot a script names; only slot 0 exists.
    fn script_player(&self) -> crate::entity::EntityRef {
        self.ents.player_ped.expect("script needs a player")
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
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
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
}
