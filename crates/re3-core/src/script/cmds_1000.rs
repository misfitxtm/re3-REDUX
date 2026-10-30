//! `Script6.cpp`, commands 1000 to 1099: threats, levels, statistics, loading a mission and passenger seats.
use super::commands as c;
use super::hooks::ScriptStat;
use super::{RunningScript, ScriptHooks, SIZE_MAIN_SCRIPT, SIZE_MISSION_SCRIPT};
use crate::ped::ctx::Px;
use crate::ped::data::{objective, state as ped_state};
use crate::world::World;

const FEET_IN_METER: f32 = 3.33;
const METERS_IN_FOOT: f32 = 0.3;

impl World {
    fn stat_named(&mut self, sh: &mut dyn ScriptHooks, name: &'static str, values: &[i32]) {
        sh.stat(ScriptStat::Named(name, values.to_vec()));
    }

    /// `COMMAND_LOAD_AND_LAUNCH_MISSION_INTERNAL`: the mission's bytecode is read from the script file into the mission part of the space.
    fn script_launch_mission(&mut self, mission: usize) {
        let sc = &mut self.ents.scripts;
        let offset = sc.multi_script_array[mission] as usize;
        let file = &sc.file;
        let n = file.len().saturating_sub(offset).min(SIZE_MISSION_SCRIPT);
        sc.space[SIZE_MAIN_SCRIPT..SIZE_MAIN_SCRIPT + n].copy_from_slice(&file[offset..offset + n]);
        let new = sc.start_new_script(SIZE_MAIN_SCRIPT as u32);
        sc.scripts[new].is_mission_script = true;
        sc.scripts[new].mission_flag = true;
        sc.already_running_a_mission_script = true;
    }

    pub(crate) fn process_commands_1000_to_1099(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::IS_CHAR_IN_CONTROL => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_CONTROL: no such ped");
                s.update_compare_flag(self.ped(ped).is_ped_in_control());
            }
            c::SET_GENERATE_CARS_AROUND_CAMERA => {
                self.collect(s, 1);
                self.ents.car_ctrl.cars_generated_around_camera = self.sp(0) != 0;
            }
            c::CLEAR_SMALL_PRINTS => sh.clear_small_messages_only(),
            c::HAS_MILITARY_CRANE_COLLECTED_ALL_CARS => s.update_compare_flag(self.ents.cranes.have_all_cars_been_collected()),
            c::SET_UPSIDEDOWN_CAR_NOT_DAMAGED => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_UPSIDEDOWN_CAR_NOT_DAMAGED: no such car");
                let on = self.sp(1) != 0;
                self.car_mut(car).expect("a car").flags.not_damaged_upside_down = on;
            }
            c::CAN_PLAYER_START_MISSION => {
                self.collect(s, 1);
                let p = self.ents.player_ped.expect("player");
                s.update_compare_flag(self.ped(p).is_ped_in_control() || self.ped(p).ped_state == crate::ped::data::state::DRIVING);
            }
            c::MAKE_PLAYER_SAFE_FOR_CUTSCENE => {
                self.collect(s, 1);
                sh.disable_player_controls_for_cutscene();
                self.make_player_safe(px, true);
                sh.start_cutscene_processing();
            }
            c::USE_TEXT_COMMANDS => {
                self.collect(s, 1);
                self.ents.scripts.use_text_commands = if self.sp(0) != 0 { 2 } else { 1 };
            }
            c::SET_THREAT_FOR_PED_TYPE | c::CLEAR_THREAT_FOR_PED_TYPE => {
                self.collect(s, 2);
                let (ty, threat) = (self.sp(0) as usize, self.sp(1) as u32);
                if command == c::SET_THREAT_FOR_PED_TYPE {
                    self.ents.ped_types.add_threat(ty, threat);
                } else {
                    self.ents.ped_types.remove_threat(ty, threat);
                }
            }
            c::IS_THREAT_FOR_PED_TYPE => {
                self.collect(s, 2);
                s.update_compare_flag(self.ents.ped_types.is_threat(self.sp(0) as usize, self.sp(1) as u32));
            }
            c::GET_CAR_COLOURS => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_COLOURS: no such car");
                let v = self.try_veh(car).unwrap();
                (self.ents.scripts.params[0], self.ents.scripts.params[1]) = (i32::from(v.current_colour1), i32::from(v.current_colour2));
                self.store(s, 2);
            }
            c::SET_ALL_CARS_CAN_BE_DAMAGED => {
                self.collect(s, 1);
                let on = self.sp(0) != 0;
                self.set_all_cars_can_be_damaged(on);
                if !on {
                    let at = self.player_info_pos();
                    self.extinguish_all_car_fires_in_area(at, 4000.0);
                }
            }
            c::SET_CAR_CAN_BE_DAMAGED => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_CAN_BE_DAMAGED: no such car");
                let on = self.sp(1) != 0;
                self.try_veh_mut(car).unwrap().flags.can_be_damaged = on;
                if !on {
                    self.extinguish_car_fire(px, car);
                }
            }
            c::LOAD_COLLISION => {
                self.collect(s, 1);
                sh.load_collision(self.sp(0));
            }
            c::GET_BODY_CAST_HEALTH => {
                self.ents.scripts.params[0] = i32::from(self.ents.objects.body_cast_health);
                self.store(s, 1);
            }
            c::SET_CHARS_CHATTING => {
                self.collect(s, 3);
                let a = self.script_ped(self.sp(0)).expect("SET_CHARS_CHATTING: no such ped");
                let b = self.script_ped(self.sp(1)).expect("SET_CHARS_CHATTING: no such ped");
                let time = self.sp(2) as u32;
                self.set_chat(a, b, time);
                self.set_chat(b, a, time);
            }
            c::SET_CAR_STAYS_IN_CURRENT_LEVEL | c::SET_CHAR_STAYS_IN_CURRENT_LEVEL => {
                self.collect(s, 2);
                let e = if command == c::SET_CAR_STAYS_IN_CURRENT_LEVEL { self.script_vehicle(self.sp(0)) } else { self.script_ped(self.sp(0)) }.expect("SET_*_STAYS_IN_CURRENT_LEVEL: no such entity");
                let level = if self.sp(1) != 0 { self.ents.zones.get_level_from_position(self.epos(e)) } else { 0 };
                if let Some(p) = self.get_mut(e).and_then(|e| e.phys.as_mut()) {
                    p.zone_level = level as i8;
                }
            }
            c::REGISTER_4X4_ONE_TIME | c::REGISTER_4X4_TWO_TIME | c::REGISTER_4X4_THREE_TIME | c::REGISTER_4X4_MAYHEM_TIME => {
                self.collect(s, 1);
                let name = match command {
                    c::REGISTER_4X4_ONE_TIME => "4x4_one",
                    c::REGISTER_4X4_TWO_TIME => "4x4_two",
                    c::REGISTER_4X4_THREE_TIME => "4x4_three",
                    _ => "4x4_mayhem",
                };
                self.stat_named(sh, name, &[self.sp(0)]);
            }
            c::REGISTER_LIFE_SAVED => self.stat_named(sh, "life_saved", &[]),
            c::REGISTER_CRIMINAL_CAUGHT => self.stat_named(sh, "criminal_caught", &[]),
            c::REGISTER_FIRE_EXTINGUISHED => self.stat_named(sh, "fire_extinguished", &[]),
            c::REGISTER_AMBULANCE_LEVEL | c::REGISTER_LONGEST_DODO_FLIGHT | c::REGISTER_DEFUSE_BOMB_TIME | c::SET_TOTAL_NUMBER_OF_KILL_FRENZIES | c::SET_TOTAL_NUMBER_OF_MISSIONS => {
                self.collect(s, 1);
                let name = match command {
                    c::REGISTER_AMBULANCE_LEVEL => "ambulance_level",
                    c::REGISTER_LONGEST_DODO_FLIGHT => "longest_dodo_flight",
                    c::REGISTER_DEFUSE_BOMB_TIME => "defuse_bomb_time",
                    c::SET_TOTAL_NUMBER_OF_KILL_FRENZIES => "total_kill_frenzies",
                    _ => "total_missions",
                };
                self.stat_named(sh, name, &[self.sp(0)]);
            }
            c::REGISTER_FASTEST_TIME | c::REGISTER_HIGHEST_SCORE => {
                self.collect(s, 2);
                self.stat_named(sh, if command == c::REGISTER_FASTEST_TIME { "fastest_time" } else { "highest_score" }, &[self.sp(0), self.sp(1)]);
            }
            c::REGISTER_KILL_FRENZY_PASSED => px.hooks.stat(crate::ped::ctx::StatEvent::KillFrenzyPassed),
            c::TURN_PHONE_ON => {
                self.collect(s, 1);
                let phone = self.sp(0) as usize;
                self.ents.phones.phones[phone].state = crate::control::phones::phone_state::STATE_9;
            }
            c::BLOW_UP_RC_BUGGY => self.blow_up_rc_buggy(),
            c::REMOVE_CAR_FROM_CHASE => {
                self.collect(s, 1);
                sh.remove_car_from_chase(self.sp(0));
            }
            c::IS_FRENCH_GAME => s.update_compare_flag(sh.is_french_game()),
            c::IS_GERMAN_GAME => s.update_compare_flag(sh.is_german_game()),
            c::CLEAR_MISSION_AUDIO => sh.clear_mission_audio(),
            c::SET_FADE_IN_AFTER_NEXT_ARREST => {
                self.collect(s, 1);
                self.ents.restart.fade_in_after_next_arrest = self.sp(0) != 0;
            }
            c::SET_FADE_IN_AFTER_NEXT_DEATH => {
                self.collect(s, 1);
                self.ents.restart.fade_in_after_next_death = self.sp(0) != 0;
            }
            c::SET_GANG_PED_MODEL_PREFERENCE => {
                self.collect(s, 2);
                self.ents.gangs.set_ped_model_override(self.sp(0) as usize, self.sp(1) as i8);
            }
            c::SET_CHAR_USE_PEDNODE_SEEK => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_USE_PEDNODE_SEEK: no such ped");
                let on = self.sp(1) != 0;
                let pd = self.ped_mut(ped);
                if on {
                    pd.next_path_node = None;
                }
                pd.flags.use_ped_node_seek = on;
            }
            c::SWITCH_VEHICLE_WEAPONS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SWITCH_VEHICLE_WEAPONS: no such car");
                let off = self.sp(1) == 0;
                self.try_veh_mut(car).unwrap().flags.gun_switched_off = off;
            }
            c::SET_GET_OUT_OF_JAIL_FREE | c::SET_FREE_HEALTH_CARE => {
                self.collect(s, 2);
                let on = self.sp(1) != 0;
                if command == c::SET_GET_OUT_OF_JAIL_FREE {
                    self.ents.player_info.get_out_of_jail_free = on;
                } else {
                    self.ents.player_info.get_out_of_hospital_free = on;
                }
            }
            c::IS_CAR_DOOR_CLOSED => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_DOOR_CLOSED: no such car");
                let door = self.sp(1) as usize;
                let c = self.car_ref(car).expect("a car");
                s.update_compare_flag(!c.is_door_missing(door) && c.is_door_closed(door));
            }
            c::LOAD_AND_LAUNCH_MISSION => {}
            c::LOAD_AND_LAUNCH_MISSION_INTERNAL => {
                self.collect(s, 1);
                self.script_launch_mission(self.sp(0) as usize);
            }
            c::SET_OBJECT_DRAW_LAST => {
                self.collect(s, 2);
                // cutscene objects belong to the cutscene manager, so a handle may not be in the pool
                if let Some(obj) = self.script_object(self.sp(0)) {
                    let on = self.sp(1) != 0;
                    self.ent_mut(obj).base.flags.draw_last = on;
                }
            }
            c::GET_AMMO_IN_PLAYER_WEAPON | c::GET_AMMO_IN_CHAR_WEAPON => {
                self.collect(s, 2);
                let ped = if command == c::GET_AMMO_IN_PLAYER_WEAPON { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("GET_AMMO_IN_CHAR_WEAPON: no such ped") };
                let slot = self.sp(1) as usize;
                let w = &self.ped(ped).weapons[slot];
                self.ents.scripts.params[0] = if w.ty == slot { w.ammo_total } else { 0 };
                self.store(s, 1);
            }
            c::SET_CHAR_SAY => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_SAY: no such ped");
                use crate::audio::{script_sound as ss, sound as snd};
                let say = match self.sp(1) as u16 {
                    ss::CHUNKY_RUN_SHOUT | ss::SECURITY_GUARD_AWAY_SHOUT => Some(snd::PED_FLEE_RUN),
                    ss::SWAT_PED_SHOUT => Some(snd::PED_PURSUIT_SWAT),
                    ss::AMMUNATION_CHAT_1 => Some(snd::AMMUNATION_WELCOME_1),
                    ss::AMMUNATION_CHAT_2 => Some(snd::AMMUNATION_WELCOME_2),
                    ss::AMMUNATION_CHAT_3 => Some(snd::AMMUNATION_WELCOME_3),
                    _ => None,
                };
                if let Some(sound) = say {
                    self.say(px, ped, sound);
                }
            }
            c::SET_NEAR_CLIP => {
                self.collect(s, 1);
                sh.set_near_clip(self.spf(0));
            }
            c::SET_RADIO_CHANNEL => {
                self.collect(s, 2);
                sh.set_radio_channel(self.sp(0), self.sp(1));
            }
            c::OVERRIDE_HOSPITAL_LEVEL | c::OVERRIDE_POLICE_STATION_LEVEL => {
                self.collect(s, 1);
                if command == c::OVERRIDE_HOSPITAL_LEVEL {
                    self.ents.restart.override_hospital_level = self.sp(0);
                } else {
                    self.ents.restart.override_police_station_level = self.sp(0);
                }
            }
            c::FORCE_RAIN => {
                self.collect(s, 1);
                sh.force_rain(self.sp(0) != 0);
            }
            c::DOES_GARAGE_CONTAIN_CAR => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(1)).expect("DOES_GARAGE_CONTAIN_CAR: no such car");
                s.update_compare_flag(self.is_this_car_within_garage_area(self.sp(0) as usize, car));
            }
            c::SET_CAR_TRACTION => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_TRACTION: no such car");
                let t = self.spf(1);
                self.car_mut(car).expect("a car").traction = t;
            }
            c::ARE_MEASUREMENTS_IN_METRES => s.update_compare_flag(sh.are_measurements_in_metres()),
            c::CONVERT_METRES_TO_FEET => {
                self.collect(s, 1);
                let feet = self.spf(0) / METERS_IN_FOOT;
                self.ents.scripts.set_param_f(0, feet);
                self.store(s, 1);
            }
            c::CONVERT_METRES_TO_FEET_INT => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = (self.sp(0) as f32 * FEET_IN_METER) as i32;
                self.store(s, 1);
            }
            c::MARK_ROADS_BETWEEN_LEVELS | c::MARK_PED_ROADS_BETWEEN_LEVELS => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                if command == c::MARK_ROADS_BETWEEN_LEVELS {
                    self.ents.paths.mark_roads_between_levels_in_area(a.min(b), a.max(b));
                } else {
                    self.ents.paths.ped_mark_roads_between_levels_in_area(a.min(b), a.max(b));
                }
            }
            c::SET_CAR_AVOID_LEVEL_TRANSITIONS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_AVOID_LEVEL_TRANSITIONS: no such car");
                let on = self.sp(1) != 0;
                self.try_veh_mut(car).unwrap().auto_pilot.stay_in_current_level = on;
            }
            c::SET_CHAR_AVOID_LEVEL_TRANSITIONS => {
                self.collect(s, 2);
                // the original checks the second argument as a ped and then does nothing
            }
            c::CLEAR_AREA_OF_CHARS => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                let (lo, hi) = (a.min(b), a.max(b));
                for ped in self.ped_refs() {
                    let at = self.epos(ped);
                    if !self.ped(ped).is_player() && self.ped(ped).can_be_deleted() && at.cmpge(lo).all() && at.cmple(hi).all() {
                        self.remove_ped(px, ped);
                    }
                }
            }
            c::IS_CAR_PASSENGER_SEAT_FREE => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_PASSENGER_SEAT_FREE: no such car");
                let (seat, v) = (self.sp(1), self.try_veh(car).unwrap());
                s.update_compare_flag((seat as usize) < v.num_max_passengers as usize && v.passengers[seat as usize].is_none());
            }
            _ => return self.process_commands_1000_to_1099_rest(px, sh, s, command),
        }
        0
    }

    /// The ped is driving the car (any car when `car` is `None`).
    fn is_sitting_in(&self, ped: crate::entity::EntityRef, car: Option<crate::entity::EntityRef>) -> bool {
        let p = self.ped(ped);
        p.ped_state == ped_state::DRIVING && car.is_none_or(|c| p.my_vehicle == Some(c))
    }

    /// Not in a car and not about to get into one.
    fn is_on_foot(&self, ped: crate::entity::EntityRef) -> bool {
        let p = self.ped(ped);
        !p.in_vehicle && !matches!(p.objective, objective::ENTER_CAR_AS_PASSENGER | objective::ENTER_CAR_AS_DRIVER)
    }

    /// The rest of `ProcessCommands1000To1099`.
    fn process_commands_1000_to_1099_rest(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::GET_CHAR_IN_CAR_PASSENGER_SEAT => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CHAR_IN_CAR_PASSENGER_SEAT: no such car");
                let seat = self.sp(1);
                let v = self.try_veh(car).unwrap();
                assert!((0..v.passengers.len() as i32).contains(&seat), "GET_CHAR_IN_CAR_PASSENGER_SEAT: seat {seat}");
                self.ents.scripts.params[0] = v.passengers[seat as usize].map_or(-1, |p| p.handle);
                self.store(s, 1);
            }
            c::SET_CHAR_IS_CHRIS_CRIMINAL => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_IS_CHRIS_CRIMINAL: no such ped");
                self.ped_mut(ped).flags.chris_criminal = self.sp(1) != 0;
            }
            c::START_CREDITS => sh.start_credits(),
            c::STOP_CREDITS => sh.stop_credits(),
            c::ARE_CREDITS_FINISHED => s.update_compare_flag(sh.credits_finished()),
            c::CREATE_SINGLE_PARTICLE => {
                self.collect(s, 8);
                sh.create_single_particle(self.sp(0), self.spv(1), self.spv(4), self.spf(7));
            }
            c::SET_CHAR_IGNORE_LEVEL_TRANSITIONS => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_IGNORE_LEVEL_TRANSITIONS: no such ped");
                self.set_ignore_level_transitions(ped, self.sp(1) != 0);
            }
            c::GET_CHASE_CAR => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = sh.turn_chase_car_into_script_car(self.sp(0));
                self.store(s, 1);
                if s.is_mission_script {
                    self.ents.scripts.cleanup.add_entity_to_list(self.ents.scripts.params[0], super::CLEANUP_CAR);
                }
            }
            c::START_BOAT_FOAM_ANIMATION => sh.start_boat_foam_animation(),
            c::UPDATE_BOAT_FOAM_ANIMATION => {
                self.collect(s, 1);
                let obj = self.script_object(self.sp(0)).expect("UPDATE_BOAT_FOAM_ANIMATION: no such object");
                sh.update_boat_foam_animation(&self.ent(obj).base.matrix);
            }
            c::SET_MUSIC_DOES_FADE => {
                self.collect(s, 1);
                sh.set_music_does_fade(self.sp(0) != 0);
            }
            c::SET_INTRO_IS_PLAYING => {
                self.collect(s, 1);
                sh.set_intro_is_playing(self.sp(0) != 0);
            }
            c::SET_PLAYER_HOOKER => {
                self.collect(s, 2);
                let now = self.now();
                let hooker = (self.sp(1) >= 0).then(|| self.script_ped(self.sp(1)).expect("SET_PLAYER_HOOKER: no such ped"));
                let info = &mut self.ents.player_info;
                info.hooker = hooker;
                (info.next_sex_frequency_update_time, info.next_sex_money_update_time) = if hooker.is_some() { (now + 1000, now + 3000) } else { (0, 0) };
            }
            c::PLAY_END_OF_GAME_TUNE => sh.play_end_of_game_tune(),
            c::STOP_END_OF_GAME_TUNE => sh.stop_end_of_game_tune(),
            c::GET_CAR_MODEL => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_MODEL: no such car");
                self.ents.scripts.params[0] = self.ent(car).base.model_index as i32;
                self.store(s, 1);
            }
            c::IS_PLAYER_SITTING_IN_CAR | c::IS_CHAR_SITTING_IN_CAR => {
                self.collect(s, 2);
                let ped = if command == c::IS_PLAYER_SITTING_IN_CAR { self.script_player() } else { self.script_ped(self.sp(0)).expect("IS_CHAR_SITTING_IN_CAR: no such ped") };
                let car = self.script_vehicle(self.sp(1)).expect("IS_x_SITTING_IN_CAR: no such car");
                s.update_compare_flag(self.is_sitting_in(ped, Some(car)));
            }
            c::IS_PLAYER_SITTING_IN_ANY_CAR | c::IS_CHAR_SITTING_IN_ANY_CAR => {
                self.collect(s, 1);
                let ped = if command == c::IS_PLAYER_SITTING_IN_ANY_CAR { self.script_player() } else { self.script_ped(self.sp(0)).expect("IS_CHAR_SITTING_IN_ANY_CAR: no such ped") };
                s.update_compare_flag(self.is_sitting_in(ped, None));
            }
            c::SET_SCRIPT_FIRE_AUDIO => {
                self.collect(s, 2);
                let (fire, on) = (self.sp(0) as usize, self.sp(1) != 0);
                self.ents.fires.fires[fire].audio_set = on;
            }
            c::ARE_ANY_CAR_CHEATS_ACTIVATED => s.update_compare_flag(sh.car_cheats_active()),
            c::SET_CHAR_SUFFERS_CRITICAL_HITS => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_SUFFERS_CRITICAL_HITS: no such ped");
                // the original tests the ped handle here; the second argument is what is meant
                self.ped_mut(ped).flags.no_critical_hits = self.sp(1) == 0;
            }
            c::IS_PLAYER_LIFTING_A_PHONE => {
                self.collect(s, 1);
                s.update_compare_flag(self.ped(self.script_player()).ped_state == ped_state::MAKE_CALL);
            }
            c::IS_PLAYER_ON_FOOT => {
                self.collect(s, 1);
                s.update_compare_flag(self.is_on_foot(self.script_player()));
            }
            c::IS_CHAR_ON_FOOT => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_ON_FOOT: no such ped");
                s.update_compare_flag(self.is_on_foot(ped));
            }
            _ => {
                let _ = px;
                panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip)
            }
        }
        0
    }

    /// `m_nZoneLevel = LEVEL_IGNORE`, or the level of where the entity is.
    pub(crate) fn set_ignore_level_transitions(&mut self, r: crate::entity::EntityRef, ignore: bool) {
        const LEVEL_IGNORE: i8 = -1;
        let level = if ignore { LEVEL_IGNORE } else { self.ents.zones.get_level_from_position(self.epos(r)) as i8 };
        if let Some(p) = self.ent_mut(r).phys.as_mut() {
            p.zone_level = level;
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

    fn world(a: &Asm) -> World {
        let mut w = world_with_script(a);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        w
    }

    fn g(w: &World, o: u32) -> i32 {
        w.ents.scripts.global(o)
    }

    use crate::ped::data::{objective, state};

    fn run(a: &Asm, setup: impl FnOnce(&mut World)) -> (World, usize) {
        let mut w = world(a);
        setup(&mut w);
        let i = start(&mut w);
        run_frame(&mut w, &mut super::super::hooks::NoScriptHooks);
        (w, i)
    }

    #[test]
    fn sitting_in_a_car_and_being_on_foot_are_different_questions() {
        let mut sitting = Asm::new(0);
        sitting.cmd(c::IS_PLAYER_SITTING_IN_ANY_CAR, &[A::I8(0)]);
        sitting.cmd(c::WAIT, &[A::I32(100000)]);
        let drive = |w: &mut World| {
            let p = w.ents.player_ped.unwrap();
            w.ped_mut(p).ped_state = state::DRIVING;
        };
        let (w, i) = run(&sitting, drive);
        assert!(w.ents.scripts.scripts[i].cond_result);
        let (w, i) = run(&sitting, |_| {});
        assert!(!w.ents.scripts.scripts[i].cond_result);

        let mut on_foot = Asm::new(0);
        on_foot.cmd(c::IS_PLAYER_ON_FOOT, &[A::I8(0)]);
        on_foot.cmd(c::WAIT, &[A::I32(100000)]);
        let (w, i) = run(&on_foot, |_| {});
        assert!(w.ents.scripts.scripts[i].cond_result);
        // heading for a car does not count as being on foot
        let (w, i) = run(&on_foot, |w| {
            let p = w.ents.player_ped.unwrap();
            w.ped_mut(p).objective = objective::ENTER_CAR_AS_DRIVER;
        });
        assert!(!w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn the_hooker_is_set_and_cleared_with_its_timers() {
        let mut set = Asm::new(2);
        set.cmd(c::SET_PLAYER_HOOKER, &[A::I8(0), A::G(8)]);
        set.cmd(c::WAIT, &[A::I32(100000)]);
        let ped = std::cell::Cell::new(None);
        let (w, _) = run(&set, |w| {
            let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PROSTITUTE, -1, None).unwrap();
            w.ents.scripts.set_global(8, p.handle);
            w.ents.timer.time_in_ms = 5000;
            ped.set(Some(p));
        });
        let info = &w.ents.player_info;
        assert_eq!((info.hooker, info.next_sex_frequency_update_time, info.next_sex_money_update_time), (ped.get(), 6000, 8000));

        let mut clear = Asm::new(0);
        clear.cmd(c::SET_PLAYER_HOOKER, &[A::I8(0), A::I8(-1)]);
        clear.cmd(c::WAIT, &[A::I32(100000)]);
        let (w, _) = run(&clear, |w| {
            w.ents.player_info.next_sex_money_update_time = 99;
        });
        assert_eq!((w.ents.player_info.hooker, w.ents.player_info.next_sex_money_update_time), (None, 0));
    }

    #[test]
    fn ped_flags_and_the_model_of_a_car() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CAR, &[A::I16(LANDSTAL_MODEL), A::F(10.0), A::F(10.0), A::F(0.0), A::G(8)]);
        a.cmd(c::GET_CAR_MODEL, &[A::G(8), A::G(12)]);
        a.cmd(c::SET_CHAR_IS_CHRIS_CRIMINAL, &[A::G(16), A::I8(1)]);
        a.cmd(c::SET_CHAR_SUFFERS_CRITICAL_HITS, &[A::G(16), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let ped = std::cell::Cell::new(None);
        let (w, _) = run(&a, |w| {
            let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
            w.ents.scripts.set_global(16, p.handle);
            ped.set(Some(p));
        });
        let flags = &w.ped(ped.get().unwrap()).flags;
        assert_eq!((g(&w, 12), flags.chris_criminal, flags.no_critical_hits), (LANDSTAL_MODEL as i32, true, true));
    }
}
