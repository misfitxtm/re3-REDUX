//! `Script4.cpp`, commands 900 to 999: phone messages, sprites and rectangles, roads at an angle, car flags and the script's name.
use super::commands as c;
use super::cmds_300::BLIP_DISPLAY_BOTH;
use super::hooks::ScriptStat;
use super::{RunningScript, ScriptHooks};
use crate::control::onscreen_timer::CounterDisplay;
use crate::math::atan_of_xy;
use crate::ped::ctx::Px;
use crate::ped::types::ped_type;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

const MAP_Z_LOW_LIMIT: f32 = -100.0;
const METERS_PER_SECOND_TO_GAME_SPEED: f32 = 1.0 / 50.0;

impl World {
    /// `SET_n_[REPEATED_]PHONE_MESSAGES`: the phone and then n text keys.
    pub(crate) fn phone_messages_command(&mut self, s: &mut RunningScript, command: u16) -> bool {
        let (count, repeated) = match command {
            c::SET_2_REPEATED_PHONE_MESSAGES => (2, true),
            c::SET_2_PHONE_MESSAGES => (2, false),
            c::SET_3_REPEATED_PHONE_MESSAGES => (3, true),
            c::SET_3_PHONE_MESSAGES => (3, false),
            c::SET_4_REPEATED_PHONE_MESSAGES => (4, true),
            c::SET_4_PHONE_MESSAGES => (4, false),
            c::SET_5_REPEATED_PHONE_MESSAGES => (5, true),
            c::SET_5_PHONE_MESSAGES => (5, false),
            c::SET_6_REPEATED_PHONE_MESSAGES => (6, true),
            c::SET_6_PHONE_MESSAGES => (6, false),
            _ => return false,
        };
        self.collect(s, 1);
        let phone = self.sp(0) as usize;
        let mut messages: [Option<String>; 6] = Default::default();
        for m in messages.iter_mut().take(count) {
            *m = Some(self.ents.scripts.read_text_label(&mut s.ip));
        }
        if repeated {
            self.ents.phones.set_phone_message_repeatedly(phone, messages);
        } else {
            self.ents.phones.set_phone_message_just_once(phone, messages);
        }
        true
    }

    pub(crate) fn process_commands_900_to_999(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        if self.phone_messages_command(s, command) {
            return 0;
        }
        match command {
            c::PRINT_STRING_IN_STRING_NOW => {
                let (text, inner) = (self.ents.scripts.read_text_label(&mut s.ip), self.ents.scripts.read_text_label(&mut s.ip));
                self.collect(s, 2);
                sh.print_string_in_string(1, &text, self.sp(0) as u32, self.sp(1), &inner);
            }
            c::LOAD_ALL_MODELS_NOW => sh.load_all_models_now(),
            c::ADD_TO_OBJECT_VELOCITY => {
                self.collect(s, 4);
                let obj = self.script_object(self.sp(0)).expect("ADD_TO_OBJECT_VELOCITY: no such object");
                let add = self.spv(1) * METERS_PER_SECOND_TO_GAME_SPEED;
                if let Some(p) = self.get_mut(obj).and_then(|e| e.phys.as_mut()) {
                    p.move_speed += add;
                }
            }
            c::DRAW_SPRITE | c::DRAW_RECT => {
                let sprite = command == c::DRAW_SPRITE;
                self.collect(s, if sprite { 9 } else { 8 });
                let o = if sprite { 1 } else { 0 };
                let (x, y, w, h) = (self.spf(o), self.spf(o + 1), self.spf(o + 2), self.spf(o + 3));
                let p = self.ents.scripts.params;
                let sc = &mut self.ents.scripts;
                let i = sc.number_of_intro_rectangles_this_frame.min(super::text::MAX_NUM_INTRO_RECTANGLES - 1);
                sc.intro_rectangles[i] = super::text::IntroRectangle {
                    used: true,
                    before_fade: false,
                    texture_id: if sprite { p[0] - 1 } else { -1 },
                    rect: [x, y, x + w, y + h],
                    colour: [p[o + 4] as u8, p[o + 5] as u8, p[o + 6] as u8, p[o + 7] as u8],
                };
                sc.number_of_intro_rectangles_this_frame += 1;
            }
            c::LOAD_SPRITE => {
                self.collect(s, 1);
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.load_sprite(self.sp(0) - 1, &name);
            }
            c::LOAD_TEXTURE_DICTIONARY => {
                let name = self.ents.scripts.read_text_label(&mut s.ip);
                sh.load_script_texture_dictionary(&name);
            }
            c::REMOVE_TEXTURE_DICTIONARY => sh.remove_script_texture_dictionary(),
            c::SET_OBJECT_DYNAMIC => {
                self.collect(s, 2);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_DYNAMIC: no such object");
                let want_static = self.sp(1) == 0;
                if self.ent(obj).base.flags.is_static != want_static {
                    self.remove(obj);
                    self.get_mut(obj).unwrap().base.flags.is_static = want_static;
                    let _ = self.add(obj);
                }
            }
            c::SET_CHAR_ANIM_SPEED => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_ANIM_SPEED: no such ped");
                let speed = self.spf(1);
                if let Some(a) = self.ped_assoc_at(ped, 0) {
                    a.speed = speed;
                }
            }
            c::PLAY_MISSION_PASSED_TUNE => {
                self.collect(s, 1);
                sh.play_mission_passed_tune(self.sp(0));
            }
            c::FREEZE_ONSCREEN_TIMER => {
                self.collect(s, 1);
                self.ents.onscreen_timer.disabled = self.sp(0) != 0;
            }
            c::SWITCH_CAR_SIREN => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SWITCH_CAR_SIREN: no such car");
                let on = self.sp(1) != 0;
                self.try_veh_mut(car).unwrap().siren_or_alarm = on;
            }
            c::SWITCH_PED_ROADS_ON_ANGLED | c::SWITCH_PED_ROADS_OFF_ANGLED | c::SWITCH_ROADS_ON_ANGLED | c::SWITCH_ROADS_OFF_ANGLED => {
                self.collect(s, 7);
                let (a, b, length) = (self.spv(0), self.spv(3), self.spf(6));
                let (ty, mode) = match command {
                    c::SWITCH_PED_ROADS_ON_ANGLED => (0, 1),
                    c::SWITCH_PED_ROADS_OFF_ANGLED => (0, 0),
                    c::SWITCH_ROADS_ON_ANGLED => (1, 1),
                    _ => (1, 0),
                };
                self.ents.paths.switch_roads_in_angled_area(a, b, length, ty, mode);
            }
            c::SET_CAR_WATERTIGHT => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_WATERTIGHT: no such car");
                let on = self.sp(1) != 0;
                self.car_mut(car).expect("a car").flags.water_tight = on;
            }
            c::ADD_MOVING_PARTICLE_EFFECT => {
                self.collect(s, 12);
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let p = self.ents.scripts.params;
                sh.add_moving_particle_effect(p[0], pos, self.spv(4), self.spf(7).max(0.0), p[11], [p[8], p[9], p[10]]);
            }
            c::SET_CHAR_CANT_BE_DRAGGED_OUT => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_CANT_BE_DRAGGED_OUT: no such ped");
                self.ped_mut(ped).flags.dont_drag_me_out_car = self.sp(1) != 0;
            }
            c::TURN_CAR_TO_FACE_COORD => {
                self.collect(s, 3);
                let car = self.script_vehicle(self.sp(0)).expect("TURN_CAR_TO_FACE_COORD: no such car");
                let pos = self.epos(car);
                let mut heading = atan_of_xy(pos.x - self.spf(1), pos.y - self.spf(2)) + FRAC_PI_2;
                if heading > TAU {
                    heading -= TAU;
                }
                self.get_mut(car).unwrap().base.set_heading(heading);
            }
            c::DRAW_SPHERE => {
                self.collect(s, 4);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                sh.draw_sphere_marker(s.ip, pos, self.spf(3));
            }
            c::SET_CAR_STATUS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_STATUS: no such car");
                self.get_mut(car).unwrap().base.status = self.sp(1) as u8;
            }
            c::IS_CHAR_MALE => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_MALE: no such ped");
                s.update_compare_flag(!matches!(self.ped(ped).ped_type, ped_type::CIVFEMALE | ped_type::PROSTITUTE));
            }
            c::SCRIPT_NAME => {
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                s.name = [0; 8];
                s.name[..name.len().min(8)].copy_from_slice(&name.as_bytes()[..name.len().min(8)]);
            }
            c::CHANGE_GARAGE_TYPE_WITH_CAR_MODEL => {
                self.collect(s, 3);
                self.ents.garages.change_garage_type(self.sp(0) as usize, self.sp(1) as u8, self.sp(2));
            }
            c::SAVE_INT_TO_DEBUG_FILE | c::SAVE_FLOAT_TO_DEBUG_FILE => self.collect(s, 1),
            c::SAVE_NEWLINE_TO_DEBUG_FILE => {}
            c::POLICE_RADIO_MESSAGE => {
                self.collect(s, 3);
                sh.police_radio_message(self.spv(0));
            }
            c::SET_CAR_STRONG => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_STRONG: no such car");
                let on = self.sp(1) != 0;
                self.try_veh_mut(car).unwrap().flags.take_less_damage = on;
            }
            c::REMOVE_ROUTE => {
                self.collect(s, 1);
                self.ents.ped_routes.remove_route(self.sp(0) as i16);
            }
            c::SWITCH_RUBBISH => {
                self.collect(s, 1);
                sh.switch_rubbish(self.sp(0) != 0);
            }
            _ => return self.process_commands_900_to_999_rest(px, sh, s, command),
        }
        0
    }
}

impl World {
    /// The blip a pickup's object gets; `-1` is a pickup that has gone.
    fn pickup_blip(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, style: (i32, i32), sprite: Option<i32>) {
        let slot = self.ents.pickups.get_actual_pickup_index(self.sp(0));
        let object = usize::try_from(slot).ok().and_then(|i| self.ents.pickups.pickups[i].object);
        let blip = sh.add_entity_blip(super::cmds_300::blip::OBJECT, object.map_or(-1, |o| o.handle), style.0, style.1);
        if let Some(sprite) = sprite {
            sh.set_blip_sprite(blip, sprite);
        }
        self.ents.scripts.params[0] = blip;
        self.store(s, 1);
    }

    /// The car a ped is in, or `-1`; the handle is not remembered by the script.
    fn car_handle_of_ped(&self, ped: Option<crate::entity::EntityRef>) -> i32 {
        ped.and_then(|p| self.try_ped(p)).filter(|p| p.in_vehicle).and_then(|p| p.my_vehicle).map_or(-1, |v| v.handle)
    }

    /// A car that may be deleted goes, and the peds in it are removed by the population.
    fn remove_car_if_deletable(&mut self, px: &mut Px, car: crate::entity::EntityRef) {
        let Some(v) = self.try_veh(car) else { return };
        if v.flags.is_locked || !v.can_be_deleted(|o| self.try_ped(o).is_none_or(|p| p.can_be_deleted())) {
            return;
        }
        let seated = v.num_max_passengers as usize;
        let occupants: Vec<_> = v.driver.into_iter().chain(v.passengers[..seated].iter().flatten().copied()).collect();
        for o in occupants {
            self.remove_ped(px, o);
        }
        self.delete(car);
    }

    /// `CWorld::ClearCarsFromArea`.
    fn script_clear_cars_from_area(&mut self, px: &mut Px, min: Vec3, max: Vec3) {
        for car in self.vehicle_refs() {
            let at = self.epos(car);
            if at.cmpge(min).all() && at.cmple(max).all() {
                self.remove_car_if_deletable(px, car);
            }
        }
    }

    /// `CWorld::ClearExcitingStuffFromArea`.
    pub(crate) fn clear_exciting_stuff_from_area(&mut self, px: &mut Px, pos: Vec3, radius: f32, remove_projectiles: bool) {
        let near = |p: Vec3| (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2) < radius * radius;
        for ped in self.ped_refs() {
            if !self.ped(ped).is_player() && self.ped(ped).can_be_deleted() && near(self.epos(ped)) {
                self.remove_ped(px, ped);
            }
        }
        for car in self.vehicle_refs() {
            if near(self.epos(car)) {
                self.remove_car_if_deletable(px, car);
            }
        }
        self.delete_temp_objects_in_area(pos, radius);
        self.extinguish_fires_at(px, pos, radius);
        self.extinguish_all_car_fires_in_area(pos, radius);
        self.ents.explosions.remove_in_area(pos, radius);
        if remove_projectiles {
            px.hooks.remove_all_projectiles();
        }
    }

    /// `FindMissionEntitiesIntersectingCube` for the test of `IS_POINT_OBSCURED_BY_A_MISSION_ENTITY`: any mission car, ped or object whose bound reaches the box.
    fn mission_entity_in_cube(&self, min: Vec3, max: Vec3) -> bool {
        use crate::entity::object::CreatedBy;
        use crate::entity::Kind;
        let refs = self.vehicle_refs().into_iter().chain(self.ped_refs()).chain(self.object_refs());
        refs.filter(|&r| match &self.ent(r).kind {
            Kind::Vehicle(v) => v.created_by == crate::vehicle::base::MISSION_VEHICLE,
            Kind::Ped(p) => p.char_created_by == crate::ped::data::MISSION_CHAR,
            Kind::Object(o) => o.created_by == CreatedBy::Mission,
            _ => false,
        })
        .any(|r| {
            let (at, radius) = (self.epos(r), self.bound_radius_of(r));
            (at + Vec3::splat(radius)).cmpge(min).all() && (at - Vec3::splat(radius)).cmple(max).all()
        })
    }

    /// `CREATE_RANDOM_CAR_FOR_CAR_PARK`: a parked car of one of the loaded models that suit a car park.
    fn script_create_random_car_for_car_park(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks) {
        use crate::modelinfo::indices_gta3::mi;
        use crate::modelinfo::vehicle_type;
        if self.ents.car_ctrl.num_random_cars >= 30 {
            return;
        }
        let loaded = sh.loaded_vehicle_models();
        if loaded.is_empty() {
            return;
        }
        let mut index = px.rng.range_i32(0, 50) as usize;
        let mut model = None;
        for _ in 0..50 {
            let m = loaded[index % loaded.len()];
            let is_car = m >= 0 && self.ents.models.get(m).and_then(|i| i.vehicle()).is_some_and(|v| v.vehicle_type == vehicle_type::CAR);
            let suits = matches!(
                m as i16,
                mi::IDAHO | mi::STINGER | mi::PEREN | mi::SENTINEL | mi::PATRIOT | mi::MANANA | mi::INFERNUS | mi::BLISTA | mi::PONY | mi::CHEETAH | mi::MOONBEAM | mi::ESPERANT | mi::TAXI | mi::KURUMA | mi::BOBCAT | mi::BANSHEE | mi::CABBIE | mi::STALLION | mi::RUMPO | 151..=153
            );
            if is_car && suits {
                model = Some(m as i16);
                break;
            }
            index = (index + 1) % 50;
        }
        let Some(model) = model else { return };
        let Some(car) = self.new_car(model, crate::vehicle::base::RANDOM_VEHICLE, px.rng) else { return };
        let mut pos = self.spv(0);
        pos.z += self.base_offset_of(car);
        let heading = self.spf(3).to_radians();
        let ent = self.ent_mut(car);
        ent.base.set_position(pos);
        ent.base.set_heading(heading);
        px.hooks.clear_space_for_mission_entity(pos, car);
        self.ent_mut(car).base.status = crate::entity::status::ABANDONED;
        self.try_veh_mut(car).unwrap().flags.is_locked = false;
        self.try_veh_mut(car).unwrap().flags.is_car_park_vehicle = true;
        self.join_car_with_road_system(px, car);
        let level = self.ents.zones.get_level_from_position(pos);
        let v = self.try_veh_mut(car).unwrap();
        v.auto_pilot.car_mission = crate::vehicle::base::car_mission::NONE;
        v.auto_pilot.temp_action = 0;
        v.auto_pilot.driving_style = crate::vehicle::base::driving_style::STOP_FOR_CARS;
        v.auto_pilot.cruise_speed = 9;
        v.auto_pilot.max_traffic_speed = 9.0;
        (v.auto_pilot.current_lane, v.auto_pilot.next_lane) = (0, 0);
        v.flags.engine_on = false;
        if let Some(p) = self.ent_mut(car).phys.as_mut() {
            p.zone_level = level as i8;
        }
        let _ = self.add(car);
    }

    /// The rest of `ProcessCommands900To999`.
    fn process_commands_900_to_999_rest(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::SWITCH_STREAMING => {
                self.collect(s, 1);
                sh.switch_streaming(self.sp(0) != 0);
            }
            c::IS_GARAGE_OPEN | c::IS_GARAGE_CLOSED => {
                self.collect(s, 1);
                let g = self.sp(0) as usize;
                let r = if command == c::IS_GARAGE_OPEN { self.ents.garages.is_garage_open(g) } else { self.ents.garages.is_garage_closed(g) };
                s.update_compare_flag(r);
            }
            c::START_CATALINA_HELI => sh.start_catalina_heli(),
            c::CATALINA_HELI_TAKE_OFF => sh.catalina_heli_take_off(),
            c::REMOVE_CATALINA_HELI => sh.remove_catalina_heli(),
            c::HAS_CATALINA_HELI_BEEN_SHOT_DOWN => s.update_compare_flag(sh.catalina_heli_shot_down()),
            c::CATALINA_HELI_FLY_AWAY => sh.catalina_heli_fly_away(),
            c::GRAB_CATALINA_HELI => {
                self.ents.scripts.params[0] = sh.catalina_heli();
                self.store(s, 1);
            }
            c::SWAP_NEAREST_BUILDING_MODEL => {
                self.collect(s, 6);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let model = |w: &World, v: i32| if v >= 0 { v } else { w.ents.scripts.used_objects.get((-v) as usize).map_or(-1, |u| u.1) };
                let (old, new) = (model(self, self.sp(4)), model(self, self.sp(5)));
                sh.swap_nearest_building_model(pos, self.spf(3), old, new);
            }
            c::SWITCH_WORLD_PROCESSING => {
                self.collect(s, 1);
                sh.switch_world_processing(self.sp(0) != 0);
            }
            c::REMOVE_ALL_PLAYER_WEAPONS => {
                self.collect(s, 1);
                let player = self.ents.player_ped.expect("REMOVE_ALL_PLAYER_WEAPONS: no player");
                self.clear_weapons(player);
            }
            c::CLEAR_AREA_OF_CARS => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                self.script_clear_cars_from_area(px, a.min(b), a.max(b));
            }
            c::SET_ROTATING_GARAGE_DOOR => {
                self.collect(s, 1);
                self.ents.garages.set_garage_door_to_rotate(self.sp(0) as usize);
            }
            c::ADD_SPHERE => {
                self.collect(s, 4);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let radius = self.spf(3);
                self.ents.scripts.params[0] = self.ents.scripts.add_script_sphere(s.ip, pos, radius);
                self.store(s, 1);
            }
            c::REMOVE_SPHERE => {
                self.collect(s, 1);
                self.ents.scripts.remove_script_sphere(self.sp(0));
            }
            c::SET_EVERYONE_IGNORE_PLAYER => {
                self.collect(s, 2);
                let on = self.sp(1) != 0;
                self.player_wanted_mut().expect("SET_EVERYONE_IGNORE_PLAYER: no player").ignored_by_everyone = on;
                if on {
                    self.stop_all_law_enforcers_in_their_tracks();
                }
            }
            c::STORE_CAR_CHAR_IS_IN_NO_SAVE => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("STORE_CAR_CHAR_IS_IN_NO_SAVE: no such ped");
                self.ents.scripts.params[0] = self.car_handle_of_ped(Some(ped));
                self.store(s, 1);
            }
            c::STORE_CAR_PLAYER_IS_IN_NO_SAVE => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = self.car_handle_of_ped(self.ents.player_ped);
                self.store(s, 1);
            }
            c::IS_PHONE_DISPLAYING_MESSAGE => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.phones.is_message_being_displayed(self.sp(0) as usize));
            }
            c::DISPLAY_ONSCREEN_TIMER_WITH_STRING | c::DISPLAY_ONSCREEN_COUNTER_WITH_STRING => {
                let var = self.read_global_var_offset(s);
                let timer = command == c::DISPLAY_ONSCREEN_TIMER_WITH_STRING;
                if !timer {
                    self.collect(s, 1);
                }
                let text = self.ents.scripts.read_text_label(&mut s.ip);
                if timer {
                    self.ents.onscreen_timer.add_clock(var, Some(&text));
                } else {
                    let ty = if self.sp(0) == 0 { CounterDisplay::Number } else { CounterDisplay::Bar };
                    self.ents.onscreen_timer.add_counter(var, ty, Some(&text));
                }
            }
            c::CREATE_RANDOM_CAR_FOR_CAR_PARK => {
                self.collect(s, 4);
                self.script_create_random_car_for_car_park(px, sh);
            }
            c::IS_COLLISION_IN_MEMORY => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.collision_in_memory == self.sp(0));
            }
            c::SET_WANTED_MULTIPLIER => {
                self.collect(s, 1);
                let m = self.spf(0);
                self.player_wanted_mut().expect("SET_WANTED_MULTIPLIER: no player").crime_sensitivity = m;
            }
            c::SET_CAMERA_IN_FRONT_OF_PLAYER => sh.set_camera_in_front_of_player(),
            c::IS_CAR_VISIBLY_DAMAGED => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_VISIBLY_DAMAGED: no such car");
                s.update_compare_flag(self.try_veh(car).unwrap().flags.is_damaged);
            }
            c::DOES_OBJECT_EXIST => {
                self.collect(s, 1);
                s.update_compare_flag(self.script_object(self.sp(0)).is_some());
            }
            c::LOAD_SCENE => {
                self.collect(s, 3);
                sh.load_scene(self.spv(0));
            }
            c::ADD_STUCK_CAR_CHECK => {
                self.collect(s, 3);
                self.script_vehicle(self.sp(0)).expect("ADD_STUCK_CAR_CHECK: no such car");
                self.add_car_to_stuck_check(self.sp(0), self.spf(1), self.sp(2) as u32);
            }
            c::REMOVE_STUCK_CAR_CHECK => {
                self.collect(s, 1);
                self.ents.script_checks.stuck.remove_car_from_check(self.sp(0));
            }
            c::IS_CAR_STUCK => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.script_checks.stuck.has_car_been_stuck_for_a_while(self.sp(0)));
            }
            c::LOAD_MISSION_AUDIO => {
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.load_mission_audio(&name);
            }
            c::HAS_MISSION_AUDIO_LOADED => s.update_compare_flag(sh.mission_audio_loaded()),
            c::PLAY_MISSION_AUDIO => sh.play_mission_audio(),
            c::HAS_MISSION_AUDIO_FINISHED => s.update_compare_flag(sh.mission_audio_finished()),
            c::GET_CLOSEST_CAR_NODE_WITH_HEADING => {
                self.collect(s, 3);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let node = self.ents.paths.find_node_closest_to_coors(pos, 0, 999999.9, true, true) as usize;
                let at = self.ents.paths.nodes[node].pos;
                let heading = self.ents.paths.find_node_orientation_for_car_placement(node);
                self.ents.scripts.set_param_v3(0, at);
                self.ents.scripts.set_param_f(3, heading);
                self.store(s, 4);
            }
            c::HAS_IMPORT_GARAGE_SLOT_BEEN_FILLED => {
                self.collect(s, 2);
                s.update_compare_flag(self.ents.garages.has_import_export_garage_collected_this_car(self.sp(0) as usize, (self.sp(1) - 1) as u8));
            }
            c::CLEAR_THIS_PRINT | c::CLEAR_THIS_BIG_PRINT => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                if command == c::CLEAR_THIS_PRINT {
                    sh.clear_this_print(&key);
                } else {
                    sh.clear_this_big_print(&key);
                }
            }
            c::SET_MISSION_AUDIO_POSITION => {
                self.collect(s, 3);
                sh.set_mission_audio_position(self.spv(0));
            }
            c::ACTIVATE_SAVE_MENU => sh.activate_save_menu(),
            c::HAS_SAVE_GAME_FINISHED => s.update_compare_flag(sh.save_game_finished()),
            c::NO_SPECIAL_CAMERA_FOR_THIS_GARAGE => {
                self.collect(s, 1);
                self.ents.garages.set_leave_camera_for_this_garage(self.sp(0) as usize);
            }
            c::ADD_BLIP_FOR_PICKUP_OLD => {
                self.collect(s, 3);
                self.pickup_blip(sh, s, (self.sp(1), self.sp(2)), None);
            }
            c::ADD_BLIP_FOR_PICKUP => {
                self.collect(s, 1);
                self.pickup_blip(sh, s, (6, BLIP_DISPLAY_BOTH), None);
                sh.change_blip_scale(self.ents.scripts.params[0], 3);
            }
            c::ADD_SPRITE_BLIP_FOR_PICKUP => {
                self.collect(s, 2);
                let sprite = self.sp(1);
                self.pickup_blip(sh, s, (6, BLIP_DISPLAY_BOTH), Some(sprite));
            }
            c::SET_PED_DENSITY_MULTIPLIER => {
                self.collect(s, 1);
                self.ents.population.ped_density_multiplier = self.spf(0);
            }
            c::FORCE_RANDOM_PED_TYPE => {
                self.collect(s, 1);
                self.ents.population.all_random_peds_this_type = self.sp(0);
            }
            c::SET_TEXT_DRAW_BEFORE_FADE => {
                self.collect(s, 1);
                let on = self.sp(0) != 0;
                self.intro_line().text_before_fade = on;
            }
            c::GET_COLLECTABLE1S_COLLECTED => {
                self.ents.scripts.params[0] = self.ents.player_info.collected_packages;
                self.store(s, 1);
            }
            c::REGISTER_EL_BURRO_TIME => {
                self.collect(s, 1);
                sh.stat(ScriptStat::Named("RegisterElBurroTime", vec![self.sp(0)]));
            }
            c::SET_SPRITES_DRAW_BEFORE_FADE => {
                self.collect(s, 1);
                let on = self.sp(0) != 0;
                let sc = &mut self.ents.scripts;
                let i = sc.number_of_intro_rectangles_this_frame.min(super::text::MAX_NUM_INTRO_RECTANGLES - 1);
                sc.intro_rectangles[i].before_fade = on;
            }
            c::SET_TEXT_RIGHT_JUSTIFY => {
                self.collect(s, 1);
                let on = self.sp(0) != 0;
                self.intro_line().right_justify = on;
            }
            c::PRINT_HELP => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                let aiming_help = matches!(key.as_str(), "HELP15" | "GUN_2A" | "GUN_3A" | "GUN_4A");
                if !(aiming_help && sh.use_mouse_third_person()) {
                    sh.set_help_message(Some(&key));
                }
            }
            c::CLEAR_HELP => sh.set_help_message(None),
            c::FLASH_HUD_OBJECT => {
                self.collect(s, 1);
                sh.flash_hud_object(self.sp(0));
            }
            c::IS_POINT_OBSCURED_BY_A_MISSION_ENTITY => {
                self.collect(s, 6);
                let (centre, half) = (self.spv(0), self.spv(3));
                let (a, b) = (centre - half, centre + half);
                s.update_compare_flag(self.mission_entity_in_cube(a.min(b), a.max(b)));
            }
            c::CLEAR_AREA => {
                self.collect(s, 5);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let (radius, projectiles) = (self.spf(3), self.sp(4) != 0);
                self.clear_exciting_stuff_from_area(px, pos, radius, projectiles);
            }
            c::IS_CRANE_LIFTING_CAR => {
                self.collect(s, 3);
                let lifting = self.script_vehicle(self.sp(2)).is_some_and(|car| self.ents.cranes.is_car_picked_up(self.spf(0), self.spf(1), car));
                s.update_compare_flag(lifting);
            }
            c::FIND_DRUG_PLANE_COORDINATES => {
                let at = sh.find_drug_plane_coordinates();
                self.ents.scripts.set_param_v3(0, at);
                self.store(s, 3);
            }
            c::REMOVE_PARTICLE_EFFECTS_IN_AREA => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                sh.remove_particle_effects_in_area(a.min(b), a.max(b));
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
    use crate::vehicle::base::RANDOM_VEHICLE;
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

    fn label(a: &mut Asm, command: u16, text: &[u8; 8]) {
        a.bytes.extend_from_slice(&command.to_le_bytes());
        a.bytes.extend_from_slice(text);
    }

    #[derive(Default)]
    struct Spy {
        help: Vec<Option<String>>,
        streaming: Vec<bool>,
        mouse: bool,
    }
    impl ScriptHooks for Spy {
        fn set_help_message(&mut self, key: Option<&str>) {
            self.help.push(key.map(str::to_owned));
        }
        fn switch_streaming(&mut self, on: bool) {
            self.streaming.push(on);
        }
        fn use_mouse_third_person(&mut self) -> bool {
            self.mouse
        }
    }

    #[test]
    fn collision_collectables_and_the_help_text() {
        let mut a = Asm::new(4);
        a.cmd(c::IS_COLLISION_IN_MEMORY, &[A::I8(2)]);
        a.cmd(c::GET_COLLECTABLE1S_COLLECTED, &[A::G(8)]);
        label(&mut a, c::PRINT_HELP, b"HELP15\0\0");
        a.cmd(c::CLEAR_HELP, &[]);
        label(&mut a, c::PRINT_HELP, b"OTHER\0\0\0");
        a.cmd(c::SWITCH_STREAMING, &[A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        w.ents.collision_in_memory = 2;
        w.ents.player_info.collected_packages = 7;
        let i = start(&mut w);
        let mut spy = Spy { mouse: true, ..Default::default() };
        run_frame(&mut w, &mut spy);
        assert!(w.ents.scripts.scripts[i].cond_result);
        assert_eq!(g(&w, 8), 7);
        // the help about the pad is left out when the mouse aims
        assert_eq!((spy.help, spy.streaming), (vec![None, Some("OTHER".to_owned())], vec![false]));
    }

    #[test]
    fn script_spheres_have_handles_that_go_stale() {
        let mut s = super::super::Scripts::default();
        let first = s.add_script_sphere(1, Vec3::X, 2.0);
        s.remove_script_sphere(first);
        let second = s.add_script_sphere(2, Vec3::Y, 3.0);
        assert_ne!(first, second);
        // the old handle no longer names the sphere that took its slot
        s.remove_script_sphere(first);
        assert!(s.spheres[(second & 0xFFFF) as usize].in_use);
        s.remove_script_sphere(second);
        assert!(!s.spheres[(second & 0xFFFF) as usize].in_use);
        for _ in 0..super::super::MAX_NUM_SCRIPT_SPHERES {
            assert_ne!(s.add_script_sphere(0, Vec3::ZERO, 1.0), -1);
        }
        assert_eq!(s.add_script_sphere(0, Vec3::ZERO, 1.0), -1);
    }

    #[test]
    fn a_stuck_check_watches_the_car_that_was_named() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CAR, &[A::I16(LANDSTAL_MODEL), A::F(10.0), A::F(10.0), A::F(0.0), A::G(8)]);
        a.cmd(c::ADD_STUCK_CAR_CHECK, &[A::G(8), A::F(2.0), A::I32(500)]);
        a.cmd(c::IS_CAR_STUCK, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let car = &w.ents.script_checks.stuck.cars[0];
        assert_eq!((car.vehicle, car.radius, car.stuck_time), (g(&w, 8), 2.0, 500));
        assert!(!w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn clearing_cars_from_an_area_keeps_the_locked_ones() {
        let mut w = world(&Asm::new(0));
        let mut rng = Rng::default();
        let (open, locked) = (w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap(), w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap());
        w.try_veh_mut(locked).unwrap().flags.is_locked = true;
        w.script_clear_cars_from_area(&mut Px { rng: &mut rng, hooks: &mut NoPedHooks }, Vec3::splat(-1000.0), Vec3::splat(1000.0));
        assert_eq!((w.get(open).is_some(), w.get(locked).is_some()), (false, true));
    }

    #[test]
    fn the_car_a_player_is_in_is_minus_one_on_foot() {
        let mut a = Asm::new(4);
        a.cmd(c::STORE_CAR_PLAYER_IS_IN_NO_SAVE, &[A::I8(0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(g(&w, 8), -1);
    }
}
