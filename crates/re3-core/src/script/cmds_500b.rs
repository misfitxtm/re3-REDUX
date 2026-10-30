//! `Script3.cpp`, the second half of commands 500 to 599: looking, gangs, special characters, models, phones and the clock.
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::ped::ctx::Px;
use crate::ped::data::{objective, state as ped_state};
use crate::player_info::wb_state;
use crate::world::World;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// A model by number, or by the script's own name table when negative.
    fn script_model(&self, wanted: i32) -> i16 {
        if wanted < 0 { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1 as i16) } else { wanted as i16 }
    }

    /// `SetLookFlag(target, true)` for a long look.
    fn look_at_for_a_minute(&mut self, source: EntityRef, target: EntityRef) {
        self.set_look_flag_entity(source, target, true);
        self.set_look_timer(source, 60000);
    }

    fn stop_looking(&mut self, px: &mut Px, ped: EntityRef) {
        self.clear_look_flag(px, ped);
        self.ped_mut(ped).flags.keep_trying_to_look = false;
        if matches!(self.ped(ped).ped_state, ped_state::LOOK_HEADING | ped_state::LOOK_ENTITY) {
            self.restore_previous_state(px, ped);
        }
    }

    fn six_strings(key: String) -> [Option<String>; 6] {
        [Some(key), None, None, None, None, None]
    }

    /// The rest of `ProcessCommands500To599`.
    pub(crate) fn process_commands_500_to_599_rest(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::IS_CAR_ARMED_WITH_BOMB => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_ARMED_WITH_BOMB: no such car");
                s.update_compare_flag(bomb_number(self.car_ref(car).expect("a car").bomb_type) == self.sp(1));
            }
            c::CHANGE_CAR_COLOUR => {
                self.collect(s, 3);
                let car = self.script_vehicle(self.sp(0)).expect("CHANGE_CAR_COLOUR: no such car");
                let (a, b) = (self.sp(1) as u8, self.sp(2) as u8);
                let v = self.try_veh_mut(car).unwrap();
                (v.current_colour1, v.current_colour2) = (a, b);
            }
            c::SWITCH_PED_ROADS_ON | c::SWITCH_PED_ROADS_OFF => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                self.ents.paths.switch_ped_roads_off_in_area(a.min(b), a.max(b), command == c::SWITCH_PED_ROADS_OFF);
            }
            c::CHAR_LOOK_AT_CHAR_ALWAYS | c::CHAR_LOOK_AT_PLAYER_ALWAYS | c::PLAYER_LOOK_AT_CHAR_ALWAYS => {
                self.collect(s, 2);
                let player = self.ents.player_ped.expect("player");
                let source = if command == c::PLAYER_LOOK_AT_CHAR_ALWAYS { player } else { self.script_ped(self.sp(0)).expect("LOOK_AT: no such ped") };
                let target = if command == c::CHAR_LOOK_AT_PLAYER_ALWAYS { player } else { self.script_ped(self.sp(1)).expect("LOOK_AT: no such target ped") };
                self.look_at_for_a_minute(source, target);
            }
            c::STOP_CHAR_LOOKING | c::STOP_PLAYER_LOOKING => {
                self.collect(s, 1);
                let ped = if command == c::STOP_PLAYER_LOOKING { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("STOP_CHAR_LOOKING: no such ped") };
                self.stop_looking(px, ped);
            }
            c::SWITCH_HELICOPTER => {
                self.collect(s, 1);
                sh.activate_heli(self.sp(0) != 0);
            }
            c::SET_GANG_CAR_MODEL => {
                self.collect(s, 2);
                self.ents.gangs.0[self.sp(0) as usize].vehicle_mi = self.sp(1);
            }
            c::SET_GANG_WEAPONS => {
                self.collect(s, 3);
                let g = &mut self.ents.gangs.0[self.sp(0) as usize];
                (g.weapon1, g.weapon2) = (self.ents.scripts.params[1], self.ents.scripts.params[2]);
            }
            c::SET_CHAR_OBJ_RUN_TO_AREA => {
                self.collect(s, 5);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_RUN_TO_AREA: no such ped");
                let (a, b) = ((self.spf(1), self.spf(2)), (self.spf(3), self.spf(4)));
                let (inf_x, sup_x, inf_y, sup_y) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
                let (x, y) = ((inf_x + sup_x) / 2.0, (inf_y + sup_y) / 2.0);
                let pos = Vec3::new(x, y, self.find_ground_z_for_coord(x, y));
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective_area(ped, objective::RUN_TO_AREA, pos, (x - inf_x).max(y - inf_y));
            }
            c::SET_CHAR_OBJ_RUN_TO_COORD => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_RUN_TO_COORD: no such ped");
                let (x, y) = (self.spf(1), self.spf(2));
                let pos = Vec3::new(x, y, self.find_ground_z_for_coord(x, y));
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective_pos(ped, objective::RUN_TO_AREA, pos);
            }
            c::IS_PLAYER_TOUCHING_OBJECT_ON_FOOT | c::IS_CHAR_TOUCHING_OBJECT_ON_FOOT => {
                self.collect(s, 2);
                let ped = if command == c::IS_PLAYER_TOUCHING_OBJECT_ON_FOOT { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CHAR_TOUCHING_OBJECT_ON_FOOT: no such ped") };
                let obj = self.script_object(self.sp(1)).expect("IS_*_TOUCHING_OBJECT_ON_FOOT: no such object");
                let touching = !self.ped(ped).in_vehicle && self.ent(ped).phys.as_ref().is_some_and(|p| p.has_collided_with(true, obj));
                s.update_compare_flag(touching);
            }
            c::LOAD_SPECIAL_CHARACTER => {
                self.collect(s, 1);
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.request_special_character(self.sp(0) - 1, &name);
            }
            c::HAS_SPECIAL_CHARACTER_LOADED => {
                self.collect(s, 1);
                s.update_compare_flag(sh.has_special_character_loaded(self.sp(0) - 1));
            }
            c::FLASH_CAR | c::FLASH_CHAR | c::FLASH_OBJECT => {
                self.collect(s, 2);
                let e = match command {
                    c::FLASH_CAR => self.script_vehicle(self.sp(0)),
                    c::FLASH_CHAR => self.script_ped(self.sp(0)),
                    _ => self.script_object(self.sp(0)),
                }
                .expect("FLASH_*: no such entity");
                let on = self.sp(1) != 0;
                self.get_mut(e).unwrap().base.flags.has_blip = on;
            }
            c::IS_PLAYER_IN_REMOTE_MODE => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.player_info.remote_vehicle.is_some());
            }
            c::ARM_CAR_WITH_BOMB => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("ARM_CAR_WITH_BOMB: no such car");
                let (bomb, rigger) = (bomb_of(self.sp(1)), self.ents.player_ped);
                let c = self.car_mut(car).expect("a car");
                c.bomb_type = bomb;
                c.bomb_rigger = rigger;
            }
            c::SET_CHAR_PERSONALITY => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_PERSONALITY: no such ped");
                self.set_ped_stats(ped, self.sp(1) as usize);
            }
            c::SET_CUTSCENE_OFFSET => {
                self.collect(s, 3);
                sh.set_cutscene_offset(self.spv(0));
            }
            c::SET_ANIM_GROUP_FOR_CHAR | c::SET_ANIM_GROUP_FOR_PLAYER => {
                self.collect(s, 2);
                let ped = if command == c::SET_ANIM_GROUP_FOR_PLAYER { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("SET_ANIM_GROUP_FOR_CHAR: no such ped") };
                self.ped_mut(ped).anim_group = self.sp(1) as _;
            }
            c::REQUEST_MODEL | c::HAS_MODEL_LOADED | c::MARK_MODEL_AS_NO_LONGER_NEEDED => {
                self.collect(s, 1);
                let model = self.script_model(self.sp(0));
                match command {
                    c::REQUEST_MODEL => sh.request_model_for_script(model),
                    c::HAS_MODEL_LOADED => s.update_compare_flag(sh.has_model_loaded_for_script(model)),
                    _ => sh.mark_model_as_no_longer_needed(model),
                }
            }
            c::GRAB_PHONE => {
                self.collect(s, 2);
                let (x, y) = (self.spf(0), self.spf(1));
                self.ents.scripts.params[0] = self.ents.phones.grab_phone(x, y).map_or(-1, |p| p as i32);
                self.store(s, 1);
            }
            c::SET_REPEATED_PHONE_MESSAGE | c::SET_PHONE_MESSAGE => {
                self.collect(s, 1);
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                let phone = self.sp(0) as usize;
                let m = Self::six_strings(key);
                if command == c::SET_REPEATED_PHONE_MESSAGE {
                    self.ents.phones.set_phone_message_repeatedly(phone, m);
                } else {
                    self.ents.phones.set_phone_message_just_once(phone, m);
                }
            }
            c::HAS_PHONE_DISPLAYED_MESSAGE => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.phones.has_message_been_displayed(self.sp(0) as usize));
            }
            c::TURN_PHONE_OFF => {
                self.collect(s, 1);
                self.ents.phones.set_phone_message_just_once(self.sp(0) as usize, Default::default());
            }
            c::DRAW_CORONA => {
                self.collect(s, 9);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let p = self.ents.scripts.params;
                sh.draw_corona(s.ip, pos, self.spf(3), p[4], p[5], [p[6], p[7], p[8]]);
            }
            c::DRAW_LIGHT => {
                self.collect(s, 6);
                let p = self.ents.scripts.params;
                sh.draw_light(self.spv(0), [p[3] as f32 / 255.0, p[4] as f32 / 255.0, p[5] as f32 / 255.0]);
            }
            c::STORE_WEATHER => sh.store_weather(),
            c::RESTORE_WEATHER => sh.restore_weather(),
            c::STORE_CLOCK => self.ents.clock.store(),
            c::RESTORE_CLOCK => self.ents.clock.restore(),
            c::RESTART_CRITICAL_MISSION => {
                self.collect(s, 4);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                self.ents.restart.override_next_restart(pos, self.spf(3));
                self.player_failed_critical_mission();
            }
            c::IS_PLAYER_PLAYING => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.player_info.wb_state == wb_state::PLAYING);
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }
}

/// The number of a bomb type, as the scripts compare it.
fn bomb_number(b: crate::vehicle::automobile::BombType) -> i32 {
    use crate::vehicle::automobile::BombType::*;
    match b {
        None => 0,
        Timed => 1,
        OnIgnition => 2,
        Remote => 3,
        TimedActive => 4,
        OnIgnitionActive => 5,
    }
}

fn bomb_of(n: i32) -> crate::vehicle::automobile::BombType {
    use crate::vehicle::automobile::BombType::*;
    [None, Timed, OnIgnition, Remote, TimedActive, OnIgnitionActive].get(n as usize).copied().unwrap_or(None)
}

#[cfg(test)]
mod tests {
    use super::super::process::tests::{run_frame, start, world_with_script, Asm, A};
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::NoPedHooks;
    use crate::ped::types::ped_type;
    use crate::vehicle::base::RANDOM_VEHICLE;
    use crate::world::tests::LANDSTAL_MODEL;

    #[derive(Default)]
    struct Spy {
        requested: Vec<i16>,
        coronas: Vec<(f32, i32)>,
    }
    impl ScriptHooks for Spy {
        fn request_model_for_script(&mut self, model: i16) {
            self.requested.push(model);
        }
        fn draw_corona(&mut self, _id: u32, _pos: Vec3, size: f32, ty: i32, _flare: i32, _rgb: [i32; 3]) {
            self.coronas.push((size, ty));
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

    #[test]
    fn bombs_gangs_colours_the_clock_and_the_player_state() {
        let mut a = Asm::new(4);
        a.cmd(c::ARM_CAR_WITH_BOMB, &[A::G(8), A::I8(2)]);
        a.cmd(c::IS_CAR_ARMED_WITH_BOMB, &[A::G(8), A::I8(2)]);
        a.cmd(c::CHANGE_CAR_COLOUR, &[A::G(8), A::I8(7), A::I8(9)]);
        a.cmd(c::SET_GANG_CAR_MODEL, &[A::I8(1), A::I16(LANDSTAL_MODEL)]);
        a.cmd(c::SET_TIME_OF_DAY, &[A::I8(5), A::I8(5)]);
        a.cmd(c::STORE_CLOCK, &[]);
        a.cmd(c::SET_TIME_OF_DAY, &[A::I8(22), A::I8(0)]);
        a.cmd(c::RESTORE_CLOCK, &[]);
        a.cmd(c::IS_PLAYER_PLAYING, &[A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.ents.scripts.set_global(8, car.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let c = w.car_ref(car).unwrap();
        assert_eq!((c.bomb_type, c.bomb_rigger), (crate::vehicle::automobile::BombType::OnIgnition, w.ents.player_ped));
        assert_eq!((w.try_veh(car).unwrap().current_colour1, w.try_veh(car).unwrap().current_colour2), (7, 9));
        assert_eq!(w.ents.gangs.0[1].vehicle_mi, LANDSTAL_MODEL as i32);
        assert_eq!((w.ents.clock.hours, w.ents.clock.minutes), (5, 5));
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn phones_models_and_coronas() {
        let mut a = Asm::new(4);
        a.cmd(c::REQUEST_MODEL, &[A::I16(150)]);
        a.cmd(c::DRAW_CORONA, &[A::F(1.0), A::F(2.0), A::F(3.0), A::F(0.5), A::I8(2), A::I8(1), A::I16(255), A::I8(0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!((spy.requested, spy.coronas), (vec![150], vec![(0.5, 2)]));
    }
}
