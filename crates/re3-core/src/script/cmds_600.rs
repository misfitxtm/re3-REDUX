//! `Script3.cpp`, commands 600 to 699 (the PS2-only "collective" commands are not in the PC game).
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::general::radian_angle_between_points;
use crate::ped::ctx::Px;
use crate::ped::data::objective;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// The `BlipSprite` commands: the same blip as the plain ones with a sprite on it.
    fn script_sprite_blip(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, of_entity: bool, blip: (u8, i32, i32)) {
        let (kind, colour, sprite) = blip;
        let id = if of_entity {
            sh.add_entity_blip(kind, self.sp(0), colour, super::cmds_300::BLIP_DISPLAY_BOTH)
        } else {
            let mut pos = self.spv(0);
            if pos.z <= MAP_Z_LOW_LIMIT {
                pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
            }
            sh.add_coord_blip(kind, pos, colour, super::cmds_300::BLIP_DISPLAY_BOTH)
        };
        sh.set_blip_sprite(id, sprite);
        self.ents.scripts.params[0] = id;
        self.store(s, 1);
    }

    /// `PlayerInAngledAreaCheckCommand`: a box whose first side is given by two points and whose second side is `side2` long.
    fn process_player_in_angled_area(&mut self, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) {
        let three_d = command >= c::IS_PLAYER_IN_ANGLED_AREA_3D;
        let name_index = if three_d { command - c::IS_PLAYER_IN_ANGLED_AREA_3D } else { command - c::IS_PLAYER_IN_ANGLED_AREA_2D };
        let (stopped, mode) = (name_index >= 3, name_index % 3); // 0 any, 1 on foot, 2 in car
        self.collect(s, if three_d { 9 } else { 7 });
        let (a, b) = (self.spv(1), if three_d { self.spv(4) } else { Vec3::new(self.spf(3), self.spf(4), 0.0) });
        let (inf_z, sup_z, side2, debug) = if three_d { (a.z.min(b.z), a.z.max(b.z), self.spf(7), self.sp(8)) } else { (0.0, 0.0, self.spf(5), self.sp(6)) };
        let mut angle = radian_angle_between_points(a.x, a.y, b.x, b.y) + FRAC_PI_2;
        while angle < 0.0 {
            angle += TAU;
        }
        while angle > TAU {
            angle -= TAU;
        }
        let rotated_sup = (b.x + side2 * angle.sin(), b.y - side2 * angle.cos());
        let rotated_inf = (a.x + side2 * angle.sin(), a.y - side2 * angle.cos());
        let (side1, side2v) = ((b.x - a.x, b.y - a.y), (rotated_inf.0 - a.x, rotated_inf.1 - a.y));
        let (len1, len2) = (side1.0.hypot(side1.1), side2v.0.hypot(side2v.1));
        let result = if stopped && !self.is_player_stopped() {
            false
        } else {
            let pos = self.player_info_pos();
            let (x, y) = (pos.x - a.x, pos.y - a.y);
            let along1 = x * side1.0 / len1 + y * side1.1 / len1;
            let along2 = x * side2v.0 / len2 + y * side2v.1 / len2;
            let inside = (0.0..=len1).contains(&along1) && (0.0..=len2).contains(&along2) && (!three_d || pos.z >= inf_z && pos.z <= sup_z);
            let in_vehicle = self.ents.player_ped.is_some_and(|p| self.ped(p).in_vehicle);
            inside
                && match mode {
                    0 => true,
                    1 => !in_vehicle,
                    _ => in_vehicle,
                }
        };
        s.update_compare_flag(result);
        if debug != 0 {
            sh.highlight_important_angled_area(s.ip, [(a.x, a.y), (b.x, b.y), rotated_sup, rotated_inf], if three_d { (inf_z + sup_z) / 2.0 } else { MAP_Z_LOW_LIMIT });
        }
    }

    pub(crate) fn process_commands_600_to_699(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::SET_CHAR_HEED_THREATS | c::SET_PLAYER_HEED_THREATS => {
                self.collect(s, 2);
                let ped = if command == c::SET_PLAYER_HEED_THREATS { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("SET_CHAR_HEED_THREATS: no such ped") };
                self.ped_mut(ped).flags.responds_to_threats = self.sp(1) != 0;
            }
            c::GET_CONTROLLER_MODE => {
                self.ents.scripts.params[0] = i32::from(px.hooks.pad_mode());
                self.store(s, 1);
            }
            c::SET_CAN_RESPRAY_CAR => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAN_RESPRAY_CAR: no such car");
                let fixed = self.sp(1) == 0;
                self.car_mut(car).expect("a car").flags.fixed_colour = fixed;
            }
            c::IS_TAXI => {
                self.collect(s, 1);
                use crate::modelinfo::indices_gta3::mi;
                let car = self.script_vehicle(self.sp(0)).expect("IS_TAXI: no such car");
                s.update_compare_flag(matches!(self.ent(car).base.model_index, mi::TAXI | mi::CABBIE | mi::BORGNINE));
            }
            c::UNLOAD_SPECIAL_CHARACTER => {
                self.collect(s, 1);
                sh.unload_special_character(self.sp(0) - 1);
            }
            c::RESET_NUM_OF_MODELS_KILLED_BY_PLAYER => self.ents.darkel.reset_models_killed_by_player(),
            c::GET_NUM_OF_MODELS_KILLED_BY_PLAYER => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = i32::from(self.ents.darkel.query_models_killed_by_player(self.sp(0)));
                self.store(s, 1);
            }
            c::ACTIVATE_GARAGE => {
                self.collect(s, 1);
                self.ents.garages.activate_garage(self.sp(0) as usize);
            }
            c::DEACTIVATE_GARAGE => {
                self.collect(s, 1);
                self.ents.garages.de_activate_garage(self.sp(0) as usize);
            }
            c::SWITCH_TAXI_TIMER => {
                self.collect(s, 1);
                let now = self.now();
                let pi = &mut self.ents.player_info;
                if self.ents.scripts.params[0] != 0 {
                    pi.unused_taxi_timer = now;
                    pi.unused_taxi_thing = true;
                } else {
                    pi.unused_taxi_thing = false;
                }
            }
            c::CREATE_OBJECT_NO_OFFSET => {
                self.collect(s, 4);
                self.script_create_object_no_offset(px, s);
            }
            c::IS_BOAT => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_BOAT: no such car");
                s.update_compare_flag(self.try_veh(car).unwrap().is_boat());
            }
            c::SET_CHAR_OBJ_GOTO_AREA_ANY_MEANS => {
                self.collect(s, 5);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_GOTO_AREA_ANY_MEANS: no such ped");
                let (a, b) = ((self.spf(1), self.spf(2)), (self.spf(3), self.spf(4)));
                let (inf_x, sup_x, inf_y, sup_y) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
                let (x, y) = ((inf_x + sup_x) / 2.0, (inf_y + sup_y) / 2.0);
                let pos = Vec3::new(x, y, self.find_ground_z_for_coord(x, y));
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective_area(ped, objective::GOTO_AREA_ANY_MEANS, pos, (x - inf_x).max(y - inf_y));
            }
            c::IS_PLAYER_STOPPED => {
                self.collect(s, 1);
                s.update_compare_flag(self.is_player_stopped());
            }
            c::IS_CHAR_STOPPED => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_STOPPED: no such ped");
                s.update_compare_flag(self.is_ped_stopped(ped));
            }
            c::MESSAGE_WAIT => {
                self.collect(s, 2);
                s.wake_time = self.now().wrapping_add(self.sp(0) as u32);
                if self.sp(1) != 0 {
                    s.skip_wake_time = true;
                }
                return 1;
            }
            c::ADD_PARTICLE_EFFECT => {
                self.collect(s, 5);
                let mut pos = self.spv(1);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                sh.add_particle_effect(self.sp(0), pos, self.sp(4) != 0);
            }
            c::SWITCH_WIDESCREEN => {
                self.collect(s, 1);
                px.hooks.camera_widescreen(self.sp(0) != 0);
            }
            c::ADD_SPRITE_BLIP_FOR_CAR | c::ADD_SPRITE_BLIP_FOR_CHAR | c::ADD_SPRITE_BLIP_FOR_OBJECT => {
                self.collect(s, 2);
                let (kind, colour) = match command {
                    c::ADD_SPRITE_BLIP_FOR_CAR => (super::cmds_300::blip::CAR, 0),
                    c::ADD_SPRITE_BLIP_FOR_CHAR => (super::cmds_300::blip::CHAR, 1),
                    _ => (super::cmds_300::blip::OBJECT, 6),
                };
                let sprite = self.sp(1);
                self.script_sprite_blip(sh, s, true, (kind, colour, sprite));
            }
            c::ADD_SPRITE_BLIP_FOR_CONTACT_POINT | c::ADD_SPRITE_BLIP_FOR_COORD => {
                self.collect(s, 4);
                let (kind, colour) = if command == c::ADD_SPRITE_BLIP_FOR_CONTACT_POINT { (super::cmds_300::blip::CONTACT_POINT, 2) } else { (super::cmds_300::blip::COORD, 5) };
                let sprite = self.sp(3);
                self.script_sprite_blip(sh, s, false, (kind, colour, sprite));
            }
            c::SET_CHAR_ONLY_DAMAGED_BY_PLAYER | c::SET_CAR_ONLY_DAMAGED_BY_PLAYER => {
                self.collect(s, 2);
                let e = if command == c::SET_CHAR_ONLY_DAMAGED_BY_PLAYER { self.script_ped(self.sp(0)) } else { self.script_vehicle(self.sp(0)) }.expect("SET_*_ONLY_DAMAGED_BY_PLAYER: no such entity");
                let on = self.sp(1) != 0;
                self.get_mut(e).unwrap().base.flags.only_damaged_by_player = on;
            }
            c::SET_CHAR_PROOFS | c::SET_CAR_PROOFS => {
                self.collect(s, 6);
                let e = if command == c::SET_CHAR_PROOFS { self.script_ped(self.sp(0)) } else { self.script_vehicle(self.sp(0)) }.expect("SET_*_PROOFS: no such entity");
                let p = self.ents.scripts.params;
                let f = &mut self.get_mut(e).unwrap().base.flags;
                (f.bullet_proof, f.fire_proof, f.explosion_proof, f.collision_proof, f.melee_proof) = (p[1] != 0, p[2] != 0, p[3] != 0, p[4] != 0, p[5] != 0);
            }
            c::IS_PLAYER_IN_ANGLED_AREA_2D..=c::IS_PLAYER_STOPPED_IN_ANGLED_AREA_IN_CAR_3D => self.process_player_in_angled_area(sh, s, command),
            c::GET_NUMBER_OF_CARS_COLLECTED_BY_GARAGE => {
                self.collect(s, 1);
                // the original returns nothing but a zero
                self.ents.scripts.params[0] = 0;
                self.store(s, 1);
            }
            c::HAS_CAR_BEEN_TAKEN_TO_GARAGE => {
                self.collect(s, 2);
                s.update_compare_flag(self.ents.garages.has_this_car_been_collected(self.sp(0) as usize, (self.sp(1) - 1) as u8));
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// `COMMAND_CREATE_OBJECT_NO_OFFSET`: like `CREATE_OBJECT` but the height is not raised by the model's base.
    fn script_create_object_no_offset(&mut self, px: &mut Px, s: &mut RunningScript) {
        let wanted = self.sp(0);
        let model = if wanted >= 0 { wanted } else { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1) } as i16;
        let Some(obj) = self.new_object(model, crate::math::Matrix::default(), px.rng) else { return };
        if let Some(crate::entity::Kind::Object(o)) = self.get_mut(obj).map(|e| &mut e.kind) {
            o.created_by = crate::entity::object::CreatedBy::Mission;
        }
        let mut pos = self.spv(1);
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
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
        sprites: Vec<(i32, i32)>,
    }
    impl ScriptHooks for Spy {
        fn add_entity_blip(&mut self, _kind: u8, _entity: i32, _colour: i32, _display: i32) -> i32 {
            41
        }
        fn set_blip_sprite(&mut self, blip: i32, sprite: i32) {
            self.sprites.push((blip, sprite));
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
    fn the_angled_area_follows_the_rotated_box() {
        // the first side runs from (0,0) to (10,0); the second is 4 long, to one side of it
        let mut a = Asm::new(2);
        a.cmd(c::IS_PLAYER_IN_ANGLED_AREA_2D, &[A::I8(0), A::F(0.0), A::F(0.0), A::F(10.0), A::F(0.0), A::F(4.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let p = w.ents.player_ped.unwrap();
        // the box is on the side the angle gives: try both sides of the line and the ends
        let mut inside = vec![];
        for (x, y) in [(5.0, 2.0), (5.0, -2.0), (5.0, 6.0), (-1.0, 2.0), (11.0, 2.0)] {
            w.get_mut(p).unwrap().base.set_position(Vec3::new(x, y, 0.0));
            let i = start(&mut w);
            run_frame(&mut w, &mut Spy::default());
            inside.push(w.ents.scripts.scripts[i].cond_result);
            w.ents.scripts.remove_from_list(i, true);
            w.ents.scripts.add_to_list(i, false);
        }
        assert_eq!(inside.iter().filter(|&&b| b).count(), 1, "exactly one side of the line is in the box: {inside:?}");
        assert!(!inside[2] && !inside[3] && !inside[4]);
        let _ = g(&w, 8);
    }

    #[test]
    fn proofs_flags_taxis_and_sprite_blips() {
        let mut a = Asm::new(4);
        a.cmd(c::SET_CAR_PROOFS, &[A::G(8), A::I8(1), A::I8(0), A::I8(1), A::I8(0), A::I8(1)]);
        a.cmd(c::SET_CAR_ONLY_DAMAGED_BY_PLAYER, &[A::G(8), A::I8(1)]);
        a.cmd(c::IS_TAXI, &[A::G(8)]);
        a.cmd(c::ADD_SPRITE_BLIP_FOR_CAR, &[A::G(8), A::I8(7), A::G(12)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        w.ents.scripts.set_global(8, car.handle);
        let i = start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        let f = &w.ent(car).base.flags;
        assert_eq!((f.bullet_proof, f.fire_proof, f.explosion_proof, f.collision_proof, f.melee_proof, f.only_damaged_by_player), (true, false, true, false, true, true));
        assert!(!w.ents.scripts.scripts[i].cond_result, "a Landstalker is no taxi");
        assert_eq!((spy.sprites, g(&w, 12)), (vec![(41, 7)], 41));
    }

    #[test]
    fn a_message_wait_can_be_skipped_and_the_taxi_timer_runs() {
        let mut a = Asm::new(2);
        a.cmd(c::MESSAGE_WAIT, &[A::I16(5000), A::I8(1)]);
        a.cmd(c::SWITCH_TAXI_TIMER, &[A::I8(1)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let i = start(&mut w);
        w.ents.timer.time_in_ms = 123;
        run_frame(&mut w, &mut Spy::default());
        let s = &w.ents.scripts.scripts[i];
        assert_eq!((s.wake_time, s.skip_wake_time, w.ents.player_info.unused_taxi_thing), (5123, true, false), "still waiting");
        w.ents.timer.time_in_ms = 6000;
        run_frame(&mut w, &mut Spy::default());
        assert_eq!((w.ents.player_info.unused_taxi_thing, w.ents.player_info.unused_taxi_timer), (true, 6000));
    }
}
