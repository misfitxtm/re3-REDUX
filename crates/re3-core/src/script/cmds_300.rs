//! `Script2.cpp`, commands 300 to 399: zones, the camera, blips, sounds and a few tests of cars and chars.
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::audio::{script_sound, sound};
use crate::control::onscreen_timer::CounterDisplay;
use crate::entity::{EntityRef, PoolKind};
use crate::math::heading;
use crate::ped::ctx::Px;
use crate::vehicle::base::CarLock;
use crate::world::World;
use crate::zones::CarInfoChange;
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

/// `eBlipType`.
pub mod blip {
    pub const CAR: u8 = 1;
    pub const CHAR: u8 = 2;
    pub const OBJECT: u8 = 3;
    pub const COORD: u8 = 4;
    pub const CONTACT_POINT: u8 = 5;
}
/// `eBlipDisplay`.
pub const BLIP_DISPLAY_BOTH: i32 = 3;

fn limit_angle_degrees(radians: f32) -> f32 {
    let mut a = radians.to_degrees();
    while a < 0.0 {
        a += 360.0;
    }
    while a >= 360.0 {
        a -= 360.0;
    }
    a
}

impl World {
    pub(crate) fn script_object(&self, handle: i32) -> Option<EntityRef> {
        let r = EntityRef { pool: PoolKind::Object, handle };
        self.get(r).map(|_| r)
    }

    fn ground_pos(&mut self, mut pos: Vec3) -> Vec3 {
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        pos
    }

    /// The car of a ped in one, else the ped.
    fn car_or_ped(&self, ped: EntityRef) -> EntityRef {
        let pd = self.ped(ped);
        pd.my_vehicle.filter(|_| pd.in_vehicle).unwrap_or(ped)
    }

    /// The facing of an entity in degrees as the scripts see it.
    fn heading_degrees(&self, e: EntityRef) -> f32 {
        limit_angle_degrees(heading(self.ent(e).base.matrix.forward))
    }

    /// A blip for an entity or a place: the command collects `n` arguments, makes the blip and stores its id.
    fn script_make_blip(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, of_entity: bool, kind: u8, style: (i32, i32, Option<i32>)) {
        let (colour, display, scale) = style;
        let id = match of_entity {
            true => sh.add_entity_blip(kind, self.sp(0), colour, display),
            false => {
                let pos = self.ground_pos(self.spv(0));
                sh.add_coord_blip(kind, pos, colour, display)
            }
        };
        if let Some(sc) = scale {
            sh.change_blip_scale(id, sc);
        }
        self.ents.scripts.params[0] = id;
        self.store(s, 1);
    }

    pub(crate) fn process_commands_300_to_399(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::HAS_PLAYER_BEEN_ARRESTED => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.player_info.wb_state == crate::player_info::wb_state::BUSTED);
            }
            c::CHANGE_CAR_LOCK => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("CHANGE_CAR_LOCK: no such car");
                self.try_veh_mut(car).unwrap().door_lock = car_lock_of(self.sp(1));
            }
            c::SHAKE_CAM_WITH_POINT => {
                self.collect(s, 4);
                sh.camera_shake_with_point(self.sp(0) as f32 / 1000.0, self.spv(1));
            }
            c::IS_CAR_MODEL => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_MODEL: no such car");
                s.update_compare_flag(self.ent(car).base.model_index as i32 == self.sp(1));
            }
            c::IS_CAR_DEAD_IN_AREA_2D => {
                self.collect(s, 6);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_DEAD_IN_AREA_2D: no such car");
                let (x1, y1, x2, y2) = (self.spf(1), self.spf(2), self.spf(3), self.spf(4));
                let e = self.ent(car);
                s.update_compare_flag(e.base.status == crate::entity::status::WRECKED && e.base.is_within_area_2d(x1, y1, x2, y2));
                if self.sp(5) != 0 {
                    sh.highlight_important_area(s.ip, x1, y1, x2, y2, MAP_Z_LOW_LIMIT);
                }
            }
            c::IS_CAR_DEAD_IN_AREA_3D => {
                self.collect(s, 8);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_DEAD_IN_AREA_3D: no such car");
                let (a, b) = (self.spv(1), self.spv(4));
                let e = self.ent(car);
                s.update_compare_flag(e.base.status == crate::entity::status::WRECKED && e.base.is_within_area(a, b));
                if self.sp(7) != 0 {
                    sh.highlight_important_area(s.ip, a.x, a.y, b.x, b.y, (a.z + b.z) / 2.0);
                }
            }
            c::IS_CAR_CRUSHED => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.garages.has_car_been_crushed(self.sp(0)));
            }
            c::CREATE_CAR_GENERATOR => {
                self.collect(s, 12);
                let pos = self.spv(0);
                let (angle, p) = (self.spf(3), self.ents.scripts.params);
                let id = self.create_car_generator(pos, angle, p[4], p[5] as i16, p[6] as i16, p[7] as u8, p[8] as u8, p[9] as u8, p[10] as u16, p[11] as u16);
                self.ents.scripts.params[0] = id.map_or(-1, |i| i as i32);
                self.store(s, 1);
            }
            c::SWITCH_CAR_GENERATOR => {
                self.collect(s, 2);
                let (i, n, now) = (self.sp(0) as usize, self.sp(1), self.now());
                if n == 0 {
                    self.ents.cargens.switch_off(i);
                } else if n <= 100 {
                    self.ents.cargens.switch_on(i, now);
                    self.ents.cargens.generators[i].uses_remaining = n as u16;
                } else {
                    self.ents.cargens.switch_on(i, now);
                }
            }
            c::ADD_PAGER_MESSAGE => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 3);
                sh.add_pager_message(&key, self.sp(0), self.sp(1), self.sp(2));
            }
            c::DISPLAY_ONSCREEN_TIMER | c::CLEAR_ONSCREEN_TIMER | c::DISPLAY_ONSCREEN_COUNTER | c::CLEAR_ONSCREEN_COUNTER => {
                let var = self.read_global_var_offset(s);
                match command {
                    c::DISPLAY_ONSCREEN_TIMER => self.ents.onscreen_timer.add_clock(var, None),
                    c::CLEAR_ONSCREEN_TIMER => self.ents.onscreen_timer.clear_clock(var),
                    c::DISPLAY_ONSCREEN_COUNTER => {
                        self.collect(s, 1);
                        let ty = if self.sp(0) == 0 { CounterDisplay::Number } else { CounterDisplay::Bar };
                        self.ents.onscreen_timer.add_counter(var, ty, None);
                    }
                    _ => self.ents.onscreen_timer.clear_counter(var),
                }
            }
            c::SET_ZONE_CAR_INFO => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 16);
                if let Some(zone) = self.ents.zones.find_zone_by_label_and_return_index(&label) {
                    let p = self.ents.scripts.params;
                    let mut gangs = [0i16; 9];
                    for (g, v) in gangs.iter_mut().zip(&p[2..9]) {
                        *g = *v as i16;
                    }
                    let change = CarInfoChange { car_density: p[1] as i16, gangs, cops: p[9] as i16, cars: [p[10] as i16, p[11] as i16, p[12] as i16, p[13] as i16, p[14] as i16, p[15] as i16] };
                    self.ents.zones.set_zone_car_info(zone, p[0] != 0, change);
                }
            }
            c::SET_ZONE_PED_INFO => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 10);
                if let Some(zone) = self.ents.zones.find_zone_by_label_and_return_index(&label) {
                    let p = self.ents.scripts.params;
                    let mut gangs = [0i16; 9];
                    for (g, v) in gangs.iter_mut().zip(&p[2..9]) {
                        *g = *v as i16;
                    }
                    self.ents.zones.set_zone_ped_info(zone, p[0] != 0, p[1] as i16, gangs, p[9] as i16);
                }
            }
            c::IS_CHAR_IN_ZONE => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_ZONE: no such ped");
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let pos = self.epos(self.car_or_ped(ped));
                let zones = &mut self.ents.zones;
                let inside = zones.find_zone_by_label_and_return_index(&label).is_some_and(|z| crate::zones::point_lies_within_zone(pos, &zones.zone_array[z]));
                s.update_compare_flag(inside);
            }
            c::SET_CAR_DENSITY | c::SET_PED_DENSITY => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 2);
                if let Some(zone) = self.ents.zones.find_zone_by_label_and_return_index(&label) {
                    let (day, density) = (self.sp(0) != 0, self.sp(1) as u16);
                    if command == c::SET_CAR_DENSITY {
                        self.ents.zones.set_car_density(zone, day, density);
                    } else {
                        self.ents.zones.set_ped_density(zone, day, density);
                    }
                }
            }
            c::POINT_CAMERA_AT_PLAYER => {
                self.collect(s, 3);
                sh.camera_take_control(None, self.sp(1), self.sp(2));
            }
            c::POINT_CAMERA_AT_CAR => {
                self.collect(s, 3);
                let car = self.script_vehicle(self.sp(0)).expect("POINT_CAMERA_AT_CAR: no such car");
                sh.camera_take_control(Some(car), self.sp(1), self.sp(2));
            }
            c::POINT_CAMERA_AT_CHAR => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("POINT_CAMERA_AT_CHAR: no such ped");
                sh.camera_take_control(Some(ped), self.sp(1), self.sp(2));
            }
            c::RESTORE_CAMERA => sh.camera_restore(),
            c::SHAKE_PAD => {
                // the original reads the arguments it was never given: they are what the last command left
                sh.pad_shake(self.sp(0), self.sp(1), self.sp(2));
            }
            c::SET_TIME_SCALE => {
                self.collect(s, 1);
                self.ents.timer.time_scale = self.spf(0);
            }
            c::IS_CAR_IN_AIR => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_IN_AIR: no such car");
                s.update_compare_flag(self.car_ref(car).expect("a car").all_wheels_off_ground());
            }
            c::SET_FIXED_CAMERA_POSITION => {
                self.collect(s, 6);
                sh.camera_set_fixed_position(self.spv(0), self.spv(3));
            }
            c::POINT_CAMERA_AT_POINT => {
                self.collect(s, 4);
                let pos = self.ground_pos(self.spv(0));
                sh.camera_take_control_no_entity(pos, self.sp(3));
            }
            c::ADD_BLIP_FOR_CAR_OLD | c::ADD_BLIP_FOR_CHAR_OLD | c::ADD_BLIP_FOR_OBJECT_OLD => {
                self.collect(s, 3);
                let kind = match command {
                    c::ADD_BLIP_FOR_CAR_OLD => blip::CAR,
                    c::ADD_BLIP_FOR_CHAR_OLD => blip::CHAR,
                    _ => blip::OBJECT,
                };
                let (colour, display) = (self.sp(1), self.sp(2));
                self.script_make_blip(sh, s, true, kind, (colour, display, None));
            }
            c::REMOVE_BLIP => {
                self.collect(s, 1);
                sh.clear_blip(self.sp(0));
            }
            c::CHANGE_BLIP_COLOUR => {
                self.collect(s, 2);
                sh.change_blip_colour(self.sp(0), self.sp(1));
            }
            c::DIM_BLIP => {
                self.collect(s, 2);
                sh.change_blip_brightness(self.sp(0), self.sp(1));
            }
            c::ADD_BLIP_FOR_COORD_OLD => {
                self.collect(s, 5);
                let (colour, display) = (self.sp(3), self.sp(4));
                self.script_make_blip(sh, s, false, blip::COORD, (colour, display, None));
            }
            c::CHANGE_BLIP_SCALE => {
                self.collect(s, 2);
                sh.change_blip_scale(self.sp(0), self.sp(1));
            }
            c::SET_FADING_COLOUR => {
                self.collect(s, 3);
                sh.set_fade_colour(self.sp(0), self.sp(1), self.sp(2));
            }
            c::DO_FADE => {
                self.collect(s, 2);
                sh.fade(self.sp(0) as f32 / 1000.0, self.sp(1));
            }
            c::GET_FADING_STATUS => s.update_compare_flag(sh.is_fading()),
            c::ADD_HOSPITAL_RESTART | c::ADD_POLICE_RESTART | c::OVERRIDE_NEXT_RESTART => {
                self.collect(s, 4);
                let pos = self.ground_pos(self.spv(0));
                let angle = self.spf(3);
                let r = &mut self.ents.restart;
                match command {
                    c::ADD_HOSPITAL_RESTART => r.add_hospital_restart_point(pos, angle),
                    c::ADD_POLICE_RESTART => r.add_police_restart_point(pos, angle),
                    _ => r.override_next_restart(pos, angle),
                }
            }
            c::DRAW_SHADOW => {
                self.collect(s, 10);
                let pos = self.spv(1);
                let (angle, length) = (self.spf(4), self.spf(5));
                let (x, y) = if angle != 0.0 { (angle.sin() * length, angle.cos() * length) } else { (0.0, length) };
                let p = self.ents.scripts.params;
                sh.draw_shadow(p[0], pos, (-x, y), (y, x), [p[6], p[7], p[8], p[9]]);
            }
            c::GET_PLAYER_HEADING | c::GET_CHAR_HEADING => {
                self.collect(s, 1);
                let ped = if command == c::GET_PLAYER_HEADING { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("GET_CHAR_HEADING: no such ped") };
                let at = self.car_or_ped(ped);
                self.ents.scripts.set_param_f(0, self.heading_degrees(at));
                self.store(s, 1);
            }
            c::SET_PLAYER_HEADING | c::SET_CHAR_HEADING => {
                self.collect(s, 2);
                let ped = if command == c::SET_PLAYER_HEADING { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("SET_CHAR_HEADING: no such ped") };
                if !self.ped(ped).in_vehicle {
                    let angle = self.spf(1).to_radians();
                    let pd = self.ped_mut(ped);
                    pd.rotation_dest = angle;
                    pd.rotation_cur = angle;
                    self.get_mut(ped).unwrap().base.set_heading(angle);
                }
            }
            c::GET_CAR_HEADING | c::GET_OBJECT_HEADING => {
                self.collect(s, 1);
                let e = if command == c::GET_CAR_HEADING { self.script_vehicle(self.sp(0)) } else { self.script_object(self.sp(0)) }.expect("GET_*_HEADING: no such entity");
                self.ents.scripts.set_param_f(0, self.heading_degrees(e));
                self.store(s, 1);
            }
            c::SET_CAR_HEADING => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_HEADING: no such car");
                let angle = self.spf(1).to_radians();
                self.get_mut(car).unwrap().base.set_heading(angle);
            }
            c::SET_OBJECT_HEADING => {
                self.collect(s, 2);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_HEADING: no such object");
                let angle = self.spf(1).to_radians();
                self.remove(obj);
                self.get_mut(obj).unwrap().base.set_heading(angle);
                let _ = self.add(obj);
            }
            c::IS_PLAYER_TOUCHING_OBJECT | c::IS_CHAR_TOUCHING_OBJECT => {
                self.collect(s, 2);
                let ped = if command == c::IS_PLAYER_TOUCHING_OBJECT { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CHAR_TOUCHING_OBJECT: no such ped") };
                let obj = self.script_object(self.sp(1)).expect("IS_*_TOUCHING_OBJECT: no such object");
                let who = self.car_or_ped(ped);
                s.update_compare_flag(self.ent(who).phys.as_ref().is_some_and(|p| p.has_collided_with(true, obj)));
            }
            c::SET_PLAYER_AMMO | c::SET_CHAR_AMMO => {
                self.collect(s, 3);
                let ped = if command == c::SET_PLAYER_AMMO { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("SET_CHAR_AMMO: no such ped") };
                self.set_ammo(ped, self.sp(1) as usize, self.sp(2));
            }
            c::DECLARE_MISSION_FLAG => {
                s.ip += 1;
                self.ents.scripts.on_a_mission_flag = self.ents.scripts.read2(&mut s.ip) as u16 as u32;
            }
            c::DECLARE_MISSION_FLAG_FOR_CONTACT => {
                self.collect(s, 1);
                s.ip += 1;
                let offset = self.ents.scripts.read2(&mut s.ip) as u16 as i32;
                self.ents.scripts.on_a_mission_for_contact_flag[self.sp(0) as usize] = offset;
            }
            c::DECLARE_BASE_BRIEF_ID_FOR_CONTACT => {
                self.collect(s, 2);
                self.ents.scripts.base_brief_id_for_contact[self.sp(0) as usize] = self.sp(1);
            }
            c::IS_PLAYER_HEALTH_GREATER | c::IS_CHAR_HEALTH_GREATER => {
                self.collect(s, 2);
                let ped = if command == c::IS_PLAYER_HEALTH_GREATER { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("IS_CHAR_HEALTH_GREATER: no such ped") };
                s.update_compare_flag(self.ped(ped).health > self.sp(1) as f32);
            }
            c::IS_CAR_HEALTH_GREATER => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_HEALTH_GREATER: no such car");
                s.update_compare_flag(self.try_veh(car).unwrap().health > self.sp(1) as f32);
            }
            c::ADD_BLIP_FOR_CAR => {
                self.collect(s, 1);
                self.script_make_blip(sh, s, true, blip::CAR, (0, BLIP_DISPLAY_BOTH, Some(3)));
            }
            c::ADD_BLIP_FOR_CHAR => {
                self.collect(s, 1);
                self.script_make_blip(sh, s, true, blip::CHAR, (1, BLIP_DISPLAY_BOTH, Some(3)));
            }
            c::ADD_BLIP_FOR_OBJECT => {
                self.collect(s, 1);
                self.script_make_blip(sh, s, true, blip::OBJECT, (6, BLIP_DISPLAY_BOTH, Some(3)));
            }
            c::ADD_BLIP_FOR_CONTACT_POINT => {
                self.collect(s, 3);
                self.script_make_blip(sh, s, false, blip::CONTACT_POINT, (2, BLIP_DISPLAY_BOTH, Some(3)));
            }
            c::ADD_BLIP_FOR_COORD => {
                self.collect(s, 3);
                self.script_make_blip(sh, s, false, blip::COORD, (5, BLIP_DISPLAY_BOTH, Some(3)));
            }
            c::CHANGE_BLIP_DISPLAY => {
                self.collect(s, 2);
                sh.change_blip_display(self.sp(0), self.sp(1));
            }
            c::ADD_ONE_OFF_SOUND => {
                self.collect(s, 4);
                let id = self.sp(3) as u16;
                let front_end = match id {
                    script_sound::EVIDENCE_PICKUP => Some(sound::EVIDENCE_PICKUP),
                    script_sound::UNLOAD_GOLD => Some(sound::UNLOAD_GOLD),
                    script_sound::PART_MISSION_COMPLETE => Some(sound::PART_MISSION_COMPLETE),
                    script_sound::RACE_START_3 => Some(sound::RACE_START_3),
                    script_sound::RACE_START_2 => Some(sound::RACE_START_2),
                    script_sound::RACE_START_1 => Some(sound::RACE_START_1),
                    script_sound::RACE_START_GO => Some(sound::RACE_START_GO),
                    _ => None,
                };
                match front_end {
                    Some(snd) => px.hooks.front_end_sound_with(snd, 0),
                    None => sh.add_one_off_sound(self.spv(0), id),
                }
            }
            c::ADD_CONTINUOUS_SOUND => {
                self.collect(s, 4);
                self.ents.scripts.params[0] = sh.add_continuous_sound(self.spv(0), self.sp(3) as u16);
                self.store(s, 1);
            }
            c::REMOVE_SOUND => {
                self.collect(s, 1);
                sh.remove_sound(self.sp(0));
            }
            c::IS_CAR_STUCK_ON_ROOF => {
                self.collect(s, 1);
                self.script_vehicle(self.sp(0)).expect("IS_CAR_STUCK_ON_ROOF: no such car");
                s.update_compare_flag(self.ents.script_checks.upside_down.has_car_been_upside_down_for_a_while(self.sp(0)));
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// A global variable written where the command expects one: the type byte is skipped and its offset read.
    pub(crate) fn read_global_var_offset(&mut self, s: &mut RunningScript) -> u32 {
        debug_assert_eq!(self.ents.scripts.space[s.ip as usize], super::arg::GLOBALVAR);
        s.ip += 1;
        self.ents.scripts.read2(&mut s.ip) as u16 as u32
    }
}

fn car_lock_of(v: i32) -> CarLock {
    match v {
        0 => CarLock::NotUsed,
        1 => CarLock::Unlocked,
        2 => CarLock::Locked,
        3 => CarLock::LockoutPlayerOnly,
        4 => CarLock::LockedPlayerInside,
        5 => CarLock::LockedInitially,
        _ => CarLock::ForceShutDoors,
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

    #[derive(Default)]
    struct Spy {
        blips: Vec<(u8, i32, i32, i32)>,
        scales: Vec<(i32, i32)>,
        fades: Vec<(f32, i32)>,
        sounds: Vec<(u16, glam::Vec3)>,
    }
    impl ScriptHooks for Spy {
        fn add_entity_blip(&mut self, kind: u8, entity: i32, colour: i32, display: i32) -> i32 {
            self.blips.push((kind, entity, colour, display));
            77
        }
        fn change_blip_scale(&mut self, blip: i32, scale: i32) {
            self.scales.push((blip, scale));
        }
        fn fade(&mut self, seconds: f32, direction: i32) {
            self.fades.push((seconds, direction));
        }
        fn is_fading(&mut self) -> bool {
            true
        }
        fn add_one_off_sound(&mut self, pos: glam::Vec3, id: u16) {
            self.sounds.push((id, pos));
        }
    }

    fn world_with_player(a: &Asm) -> World {
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
    fn mission_flags_blips_and_the_fade() {
        let mut a = Asm::new(6);
        a.cmd(c::DECLARE_MISSION_FLAG, &[A::G(8)]);
        a.cmd(c::DECLARE_MISSION_FLAG_FOR_CONTACT, &[A::I8(2), A::G(12)]);
        a.cmd(c::DECLARE_BASE_BRIEF_ID_FOR_CONTACT, &[A::I8(2), A::I16(100)]);
        a.cmd(c::ADD_BLIP_FOR_CAR, &[A::G(16), A::G(20)]);
        a.cmd(c::DO_FADE, &[A::I16(1500), A::I8(1)]);
        a.cmd(c::GET_FADING_STATUS, &[]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        w.ents.scripts.set_global(16, 4242);
        let i = start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        let sc = &w.ents.scripts;
        assert_eq!((sc.on_a_mission_flag, sc.on_a_mission_for_contact_flag[2], sc.base_brief_id_for_contact[2]), (8, 12, 100));
        assert_eq!(spy.blips, vec![(blip::CAR, 4242, 0, BLIP_DISPLAY_BOTH)]);
        assert_eq!((spy.scales.clone(), g(&w, 20)), (vec![(77, 3)], 77));
        assert_eq!(spy.fades, vec![(1.5, 1)]);
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn zone_densities_the_clock_of_the_screen_and_restart_points() {
        // the label sits between the command and its numbers
        let mut a = Asm::new(4);
        a.bytes.extend_from_slice(&c::SET_CAR_DENSITY.to_le_bytes());
        a.bytes.extend_from_slice(b"CITYZON\0");
        a.bytes.extend_from_slice(&[4, 1, 5, 33, 0]);
        a.cmd(c::ADD_HOSPITAL_RESTART, &[A::F(1.0), A::F(2.0), A::F(3.0), A::F(90.0)]);
        a.cmd(c::DISPLAY_ONSCREEN_TIMER, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(w.ents.zones.zone_info_array[w.ents.zones.zone_array[0].zoneinfo_day as usize].car_density, 33);
        assert_eq!(w.ents.restart.hospital_points.len(), 1);
        assert_eq!(w.ents.onscreen_timer.entries[0].timer_offset, 8);
    }

    #[test]
    fn headings_in_degrees_and_car_locks_and_health() {
        let mut a = Asm::new(4);
        a.cmd(c::SET_CAR_HEADING, &[A::G(8), A::F(90.0)]);
        a.cmd(c::GET_CAR_HEADING, &[A::G(8), A::G(12)]);
        a.cmd(c::CHANGE_CAR_LOCK, &[A::G(8), A::I8(2)]);
        a.cmd(c::IS_CAR_HEALTH_GREATER, &[A::G(8), A::I16(500)]);
        a.cmd(c::IS_CAR_MODEL, &[A::G(8), A::I16(LANDSTAL_MODEL)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        let car = w.new_car(LANDSTAL_MODEL, crate::vehicle::base::RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.ents.scripts.set_global(8, car.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let deg = f32_of_global(&w, 12);
        assert!((deg - 90.0).abs() < 0.01 || (deg - 270.0).abs() < 0.01, "{deg}");
        assert_eq!(w.try_veh(car).unwrap().door_lock, CarLock::Locked);
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    fn f32_of_global(w: &World, o: u32) -> f32 {
        super::super::f32_of(g(w, o))
    }

    #[test]
    fn a_one_off_sound_goes_to_the_hook_and_a_race_start_sound_to_the_front_end() {
        let mut a = Asm::new(2);
        a.cmd(c::ADD_ONE_OFF_SOUND, &[A::F(1.0), A::F(2.0), A::F(3.0), A::I8(5)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world_with_player(&a);
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(spy.sounds, vec![(5, glam::Vec3::new(1.0, 2.0, 3.0))]);
    }
}
