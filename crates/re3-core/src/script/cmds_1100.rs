//! `Script6.cpp`, commands 1100 to 1199: loading screens, object placing, random cops, handbrake turns and the debug camera.
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::object::CreatedBy;
use crate::entity::{EntityRef, Kind, PoolKind};
use crate::ped::ctx::Px;
use crate::ped::data::objective;
use crate::ped::types::ped_type;
use crate::vehicle::automobile::max_passengers;
use crate::vehicle::base::temp_action;
use crate::world::{Kinds, World};
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// `CPhysical::PlacePhysicalRelativeToOtherPhysical`.
    pub(crate) fn place_physical_relative_to_other(&mut self, other: EntityRef, phys: EntityRef, local: Vec3) {
        let matrix = self.ent(other).base.matrix;
        let speed = self.ent(other).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let pos = speed * (0.9 * self.ents.timer.time_step) + matrix.transform_point(local);
        self.remove(phys);
        let e = self.ent_mut(phys);
        e.base.matrix = matrix;
        e.base.set_position(pos);
        if let Some(p) = e.phys.as_mut() {
            p.move_speed = speed;
        }
        let _ = self.add(phys);
    }

    /// The object of a model that is nearest to a point; scenery of that model turns into a real object first.
    fn closest_object_of_type(&mut self, px: &mut Px, pos: Vec3, range: f32, model: i16) -> Option<EntityRef> {
        let kinds = Kinds { objects: true, dummies: true, ..Kinds::NONE };
        let nearest = |w: &mut World| {
            let found = w.find_objects_of_type_in_range(model, pos, range, true, 16, kinds);
            let mut best = (2.0 * range, None);
            for r in found {
                let d = (w.epos(r) - pos).length();
                if d < best.0 {
                    best = (d, Some(r));
                }
            }
            best.1
        };
        let mut closest = nearest(self);
        if let Some(dummy) = closest.filter(|&r| matches!(self.ent(r).kind, Kind::Dummy)) {
            self.convert_to_real_object(px, dummy);
            closest = nearest(self).filter(|&r| !matches!(self.ent(r).kind, Kind::Dummy));
        }
        closest
    }

    /// `GET_RANDOM_COP_IN_AREA` and `_IN_ZONE`: a policeman near the player becomes the script's.
    fn script_random_cop(&mut self, s: &RunningScript, inside: impl Fn(&World, Vec3) -> bool) -> i32 {
        let suits = |pd: &crate::ped::data::PedData, _| pd.ped_type == ped_type::COP && (pd.is_ped_in_control() || pd.ped_state == crate::ped::data::state::DRIVING);
        self.script_random_ped_where(s, suits, inside)
    }

    pub(crate) fn process_commands_1100_to_1199(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, idx: usize, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::LOAD_COLLISION_WITH_SCREEN => {
                self.collect(s, 1);
                sh.load_collision_with_screen(self.sp(0));
            }
            c::LOAD_SPLASH_SCREEN => {
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.load_splash_screen(&name);
            }
            c::SET_CAR_IGNORE_LEVEL_TRANSITIONS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_IGNORE_LEVEL_TRANSITIONS: no such car");
                self.set_ignore_level_transitions(car, self.sp(1) != 0);
            }
            c::MAKE_CRAIGS_CAR_A_BIT_STRONGER => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("MAKE_CRAIGS_CAR_A_BIT_STRONGER: no such car");
                let on = self.sp(1) != 0;
                self.car_mut(car).expect("MAKE_CRAIGS_CAR_A_BIT_STRONGER: not a car").flags.more_resistant_to_damage = on;
            }
            c::SET_JAMES_CAR_ON_PATH_TO_PLAYER => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("SET_JAMES_CAR_ON_PATH_TO_PLAYER: no such car");
                let to = self.player_info_pos();
                self.join_car_with_road_system_goto_coors(px, car, to, false);
            }
            c::LOAD_END_OF_GAME_TUNE => sh.load_end_of_game_tune(),
            c::ENABLE_PLAYER_CONTROL_CAMERA => sh.enable_player_control_camera(),
            c::SET_OBJECT_ROTATION => {
                self.collect(s, 4);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_ROTATION: no such object");
                let (x, y, z) = (self.spf(1).to_radians(), self.spf(2).to_radians(), self.spf(3).to_radians());
                self.remove(obj);
                self.ent_mut(obj).base.set_orientation(x, y, z);
                let _ = self.add(obj);
            }
            c::GET_DEBUG_CAMERA_COORDINATES | c::GET_DEBUG_CAMERA_FRONT_VECTOR | c::GET_DEBUG_CAMERA_POINT_AT => {
                let (source, front) = sh.debug_camera();
                let v = match command {
                    c::GET_DEBUG_CAMERA_COORDINATES => source,
                    c::GET_DEBUG_CAMERA_FRONT_VECTOR => front,
                    _ => source + front,
                };
                self.ents.scripts.set_param_v3(0, v);
                self.store(s, 3);
            }
            c::IS_PLAYER_TARGETTING_ANY_CHAR | c::IS_PLAYER_TARGETTING_CHAR | c::IS_PLAYER_TARGETTING_OBJECT => {
                self.collect(s, if command == c::IS_PLAYER_TARGETTING_ANY_CHAR { 1 } else { 2 });
                let target = self.ped(self.script_player()).point_gun_at;
                let wanted = match command {
                    c::IS_PLAYER_TARGETTING_ANY_CHAR => target.map(|t| t.pool == PoolKind::Ped),
                    c::IS_PLAYER_TARGETTING_CHAR => self.script_ped(self.sp(1)).map(|p| target == Some(p)),
                    _ => self.script_object(self.sp(1)).map(|o| target == Some(o)),
                };
                s.update_compare_flag(wanted.unwrap_or(false));
            }
            c::TERMINATE_ALL_SCRIPTS_WITH_THIS_NAME => {
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                let mut bytes = [0u8; 8];
                bytes[..name.len().min(8)].copy_from_slice(&name.as_bytes()[..name.len().min(8)]);
                let mut at = self.ents.scripts.active;
                while let Some(i) = at {
                    at = self.ents.scripts.next_of(i);
                    // the running script was taken out of its slot, so its own name is s.name
                    let its_name = if i == idx { s.name } else { self.ents.scripts.scripts[i].name };
                    if its_name == bytes {
                        self.ents.scripts.remove_from_list(i, true);
                        self.ents.scripts.add_to_list(i, false);
                    }
                }
            }
            c::DISPLAY_TEXT_WITH_NUMBER | c::DISPLAY_TEXT_WITH_2_NUMBERS => {
                self.collect(s, 2);
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                let (x, y) = (self.spf(0), self.spf(1));
                let count = if command == c::DISPLAY_TEXT_WITH_NUMBER { 1 } else { 2 };
                self.collect(s, count);
                let numbers = self.ents.scripts.params[..count].to_vec();
                let line = self.intro_line();
                (line.at_x, line.at_y, line.text, line.numbers) = (x, y, key, numbers);
                self.ents.scripts.number_of_intro_text_lines_this_frame += 1;
            }
            c::FAIL_CURRENT_MISSION => self.ents.scripts.fail_current_mission = 2,
            c::GET_CLOSEST_OBJECT_OF_TYPE => {
                self.collect(s, 5);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let wanted = self.sp(4);
                let model = if wanted < 0 { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1) } else { wanted } as i16;
                let range = self.spf(3);
                let handle = match self.closest_object_of_type(px, pos, range, model) {
                    Some(obj) => {
                        if let Kind::Object(o) = &mut self.ent_mut(obj).kind {
                            o.created_by = CreatedBy::Mission;
                        }
                        obj.handle
                    }
                    None => -1,
                };
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::PLACE_OBJECT_RELATIVE_TO_OBJECT => {
                self.collect(s, 5);
                let obj = self.script_object(self.sp(0)).expect("PLACE_OBJECT_RELATIVE_TO_OBJECT: no such object");
                let target = self.script_object(self.sp(1)).expect("PLACE_OBJECT_RELATIVE_TO_OBJECT: no such target");
                self.place_physical_relative_to_other(target, obj, self.spv(2));
            }
            c::SET_ALL_OCCUPANTS_OF_CAR_LEAVE_CAR => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("SET_ALL_OCCUPANTS_OF_CAR_LEAVE_CAR: no such car");
                let v = self.try_veh(car).unwrap();
                let occupants: Vec<_> = v.driver.into_iter().chain(v.passengers.iter().flatten().copied()).collect();
                for ped in occupants {
                    self.ped_mut(ped).flags.script_objective_completed = false;
                    self.set_objective_entity(px, ped, objective::LEAVE_CAR, car);
                }
            }
            c::SET_INTERPOLATION_PARAMETERS => {
                self.collect(s, 2);
                sh.set_interpolation_parameters(self.spf(0), self.sp(1));
            }
            c::GET_CLOSEST_CAR_NODE_WITH_HEADING_TOWARDS_POINT | c::GET_CLOSEST_CAR_NODE_WITH_HEADING_AWAY_POINT => {
                self.collect(s, 5);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let towards = command == c::GET_CLOSEST_CAR_NODE_WITH_HEADING_TOWARDS_POINT;
                let (x, y) = (self.spf(3), self.spf(4));
                let node = self.ents.paths.find_node_closest_to_coors(pos, 0, 999999.9, true, true) as usize;
                let at = self.ents.paths.nodes[node].pos;
                let heading = self.ents.paths.find_node_orientation_facing_destination(node, x, y, towards);
                self.ents.scripts.set_param_v3(0, at);
                self.ents.scripts.set_param_f(3, heading);
                self.store(s, 4);
            }
            // the originals do nothing, and take no arguments
            c::ATTACH_CHAR_TO_CAR | c::DETACH_CHAR_FROM_CAR => {}
            c::SET_CAR_CHANGE_LANE => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_CHANGE_LANE: no such car");
                let fast = self.sp(1) == 0;
                self.try_veh_mut(car).unwrap().auto_pilot.stay_in_fast_lane = fast;
            }
            c::CLEAR_CHAR_LAST_WEAPON_DAMAGE => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("CLEAR_CHAR_LAST_WEAPON_DAMAGE: no such ped");
                self.ped_mut(ped).last_wep_dam = -1;
            }
            c::CLEAR_CAR_LAST_WEAPON_DAMAGE => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("CLEAR_CAR_LAST_WEAPON_DAMAGE: no such car");
                self.try_veh_mut(car).unwrap().last_weapon_damage = -1;
            }
            c::GET_RANDOM_COP_IN_AREA => {
                self.collect(s, 4);
                let (a, b) = ((self.spf(0), self.spf(1)), (self.spf(2), self.spf(3)));
                let handle = self.script_random_cop(s, |_, p| p.x >= a.0.min(b.0) && p.x <= a.0.max(b.0) && p.y >= a.1.min(b.1) && p.y <= a.1.max(b.1));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::GET_RANDOM_COP_IN_ZONE => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let zone = self.ents.zones.find_zone_by_label_and_return_index(&label);
                let handle = self.script_random_cop(s, |w, p| zone.is_some_and(|z| crate::zones::point_lies_within_zone(p, &w.ents.zones.zone_array[z])));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::SET_CHAR_OBJ_FLEE_CAR => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_FLEE_CAR: no such ped");
                let car = self.script_vehicle(self.sp(1)).expect("SET_CHAR_OBJ_FLEE_CAR: no such car");
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective_entity(px, ped, objective::FLEE_CAR, car);
            }
            c::GET_DRIVER_OF_CAR => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_DRIVER_OF_CAR: no such car");
                self.ents.scripts.params[0] = self.try_veh(car).unwrap().driver.map_or(-1, |d| d.handle);
                self.store(s, 1);
            }
            c::GET_NUMBER_OF_FOLLOWERS => {
                self.collect(s, 1);
                let leader = self.script_ped(self.sp(0)).expect("GET_NUMBER_OF_FOLLOWERS: no such ped");
                let followers = self.ped_refs().into_iter().filter(|&p| self.ped(p).leader == Some(leader)).count();
                self.ents.scripts.params[0] = followers as i32;
                self.store(s, 1);
            }
            c::GIVE_REMOTE_CONTROLLED_MODEL_TO_PLAYER => {
                self.collect(s, 6);
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let heading = self.spf(4).to_radians();
                self.give_player_remote_controlled_car(px, pos.x, pos.y, pos.z, heading, self.sp(5) as i16);
            }
            c::GET_CURRENT_PLAYER_WEAPON | c::GET_CURRENT_CHAR_WEAPON => {
                self.collect(s, 1);
                let ped = if command == c::GET_CURRENT_PLAYER_WEAPON { self.script_player() } else { self.script_ped(self.sp(0)).expect("GET_CURRENT_CHAR_WEAPON: no such ped") };
                let p = self.ped(ped);
                self.ents.scripts.params[0] = p.weapons[p.current_weapon as usize].ty as i32;
                self.store(s, 1);
            }
            c::SET_CAR_HANDBRAKE_TURN_LEFT | c::SET_CAR_HANDBRAKE_TURN_RIGHT | c::SET_CAR_HANDBRAKE_STOP => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_HANDBRAKE_*: no such car");
                let until = self.now().wrapping_add(self.sp(1) as u32);
                let action = match command {
                    c::SET_CAR_HANDBRAKE_TURN_LEFT => temp_action::HANDBRAKETURNLEFT,
                    c::SET_CAR_HANDBRAKE_TURN_RIGHT => temp_action::HANDBRAKETURNRIGHT,
                    _ => temp_action::HANDBRAKESTRAIGHT,
                };
                let ap = &mut self.try_veh_mut(car).unwrap().auto_pilot;
                (ap.temp_action, ap.time_temp_action) = (action, until);
            }
            c::IS_CHAR_ON_ANY_BIKE | c::IS_PLAYER_ON_ANY_BIKE => {
                self.collect(s, 1);
                let ped = if command == c::IS_PLAYER_ON_ANY_BIKE { self.script_player() } else { self.script_ped(self.sp(0)).expect("IS_CHAR_ON_ANY_BIKE: no such ped") };
                let p = self.ped(ped);
                let on_bike = p.in_vehicle && p.my_vehicle.is_some_and(|v| self.ents.models.is_bike_model(self.ent(v).base.model_index as i32));
                s.update_compare_flag(on_bike);
            }
            c::GET_NUMBER_OF_SEATS_IN_MODEL => {
                self.collect(s, 1);
                let model = self.sp(0) as i16;
                let doors = self.ents.models.get(model as i32).and_then(|m| m.vehicle()).map_or(0, |v| v.num_doors);
                self.ents.scripts.params[0] = max_passengers(model, doors) as i32 + 1;
                self.store(s, 1);
            }
            c::IS_CHAR_LYING_DOWN => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_LYING_DOWN: no such ped");
                s.update_compare_flag(self.ped(ped).flags.fallen_down);
            }
            c::CAN_CHAR_SEE_DEAD_CHAR => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("CAN_CHAR_SEE_DEAD_CHAR: no such ped");
                let kind = self.sp(1);
                let p = self.ped(ped);
                let near: Vec<_> = p.near_peds.iter().take(p.num_near_peds as usize).flatten().copied().collect();
                let mut can = false;
                for other in near {
                    let o = self.ped(other);
                    if o.health <= 0.0 && o.ped_type as i32 == kind && self.our_ped_can_see_this_one(ped, other) {
                        can = true;
                    }
                }
                s.update_compare_flag(can);
            }
            c::SET_ENTER_CAR_RANGE_MULTIPLIER => {
                self.collect(s, 1);
                sh.set_enter_car_range_multiplier(self.spf(0));
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

    use crate::vehicle::base::temp_action;

    fn run(a: &Asm, setup: impl FnOnce(&mut World)) -> (World, usize) {
        let mut w = world(a);
        setup(&mut w);
        let i = start(&mut w);
        run_frame(&mut w, &mut super::super::hooks::NoScriptHooks);
        (w, i)
    }

    #[test]
    fn a_physical_is_placed_where_the_other_one_is_going() {
        let mut w = world(&Asm::new(0));
        let mut rng = Rng::default();
        let (base, other) = (w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap(), w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap());
        w.ent_mut(base).base.set_position(Vec3::new(10.0, 20.0, 0.0));
        w.ent_mut(base).phys.as_mut().unwrap().move_speed = Vec3::new(1.0, 0.0, 0.0);
        w.place_physical_relative_to_other(base, other, Vec3::new(0.0, 5.0, 0.0));
        let at = w.epos(other);
        // 0.9 time steps ahead of the other one's movement
        assert!((at - Vec3::new(10.9, 25.0, 0.0)).length() < 1e-4, "{at}");
        assert_eq!(w.ent(other).phys.as_ref().unwrap().move_speed, Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn scripts_with_a_name_are_moved_to_the_idle_list() {
        let mut a = Asm::new(0);
        label(&mut a, c::TERMINATE_ALL_SCRIPTS_WITH_THIS_NAME, b"BOB\0\0\0\0\0");
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let other = a.here() as u32;
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let first = start(&mut w);
        let second = w.ents.scripts.start_new_script(other);
        w.ents.scripts.scripts[second].name = *b"bob\0\0\0\0\0";
        w.ents.scripts.scripts[first].name = *b"carol\0\0\0";
        run_frame(&mut w, &mut super::super::hooks::NoScriptHooks);
        let mut active = vec![];
        let mut at = w.ents.scripts.active;
        while let Some(i) = at {
            active.push(i);
            at = w.ents.scripts.next_of(i);
        }
        assert_eq!(active, vec![first]);
        assert!(w.ents.scripts.idle.is_some_and(|i| i == second || w.ents.scripts.next_of(i) == Some(second)));
    }

    #[test]
    fn text_with_numbers_keeps_its_place_and_numbers() {
        let mut a = Asm::new(0);
        a.bytes.extend_from_slice(&c::DISPLAY_TEXT_WITH_2_NUMBERS.to_le_bytes());
        a.bytes.extend_from_slice(&[6, 0x00, 0x08, 6, 0x00, 0x10]);
        a.bytes.extend_from_slice(b"HELLO\0\0\0");
        a.bytes.extend_from_slice(&[4, 5, 4, 7]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let (w, _) = run(&a, |_| {});
        let sc = &w.ents.scripts;
        let l = &sc.intro_text_lines[0];
        assert_eq!((sc.number_of_intro_text_lines_this_frame, l.text.as_str(), l.numbers.clone(), l.at_x, l.at_y), (1, "HELLO", vec![5, 7], 128.0, 256.0));
    }

    #[test]
    fn handbrake_turns_and_seats_in_a_model() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CAR, &[A::I16(LANDSTAL_MODEL), A::F(10.0), A::F(10.0), A::F(0.0), A::G(8)]);
        a.cmd(c::SET_CAR_HANDBRAKE_TURN_LEFT, &[A::G(8), A::I16(500)]);
        a.cmd(c::GET_NUMBER_OF_SEATS_IN_MODEL, &[A::I16(crate::modelinfo::indices_gta3::mi::FIRETRUCK), A::G(12)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let (w, _) = run(&a, |w| w.ents.timer.time_in_ms = 1000);
        let car = w.script_vehicle(g(&w, 8)).unwrap();
        let ap = &w.try_veh(car).unwrap().auto_pilot;
        assert_eq!((ap.temp_action, ap.time_temp_action, g(&w, 12)), (temp_action::HANDBRAKETURNLEFT, 1500, 2));
    }

    #[test]
    fn a_random_cop_becomes_the_scripts() {
        let mut a = Asm::new(4);
        a.cmd(c::GET_RANDOM_COP_IN_AREA, &[A::F(-50.0), A::F(-50.0), A::F(50.0), A::F(50.0), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let mut px = Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks };
        // slot 0 is never reached, and a civilian is not a cop
        let _first = w.create_ped(&mut px, ped_type::COP, -1, None).unwrap();
        let civilian = w.create_ped(&mut px, ped_type::CIVMALE, -1, None).unwrap();
        let cop = w.create_ped(&mut px, ped_type::COP, -1, None).unwrap();
        for p in [civilian, cop] {
            w.get_mut(p).unwrap().base.set_position(Vec3::new(5.0, 5.0, 0.0));
        }
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut super::super::hooks::NoScriptHooks);
        assert_eq!(g(&w, 8), cop.handle);
        assert_eq!((w.ped(cop).char_created_by, w.ped(civilian).char_created_by), (crate::ped::data::MISSION_CHAR, crate::ped::data::RANDOM_CHAR));
    }
}
