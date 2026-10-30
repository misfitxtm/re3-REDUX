//! `Script3.cpp`, commands 500 to 599: pickups, garages, health, explosions and numbers.
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::math::atan_of_xy;
use crate::ped::ctx::Px;
use crate::ped::data::objective;
use crate::vehicle::base::CarLock;
use crate::weapons::explosion::ExplosionType;
use crate::world::World;
use glam::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

const MAP_Z_LOW_LIMIT: f32 = -100.0;

fn explosion_type_of(v: i32) -> ExplosionType {
    use ExplosionType::*;
    [Grenade, Molotov, Rocket, Car, CarQuick, Heli, Mine, Barrel, TankGrenade, HeliBomb].get(v as usize).copied().unwrap_or(Grenade)
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

impl World {
    /// A script handle of a ped in a car is where the car is.
    fn locate_of(&self, ped: EntityRef) -> Vec3 {
        let pd = self.ped(ped);
        self.epos(pd.my_vehicle.filter(|_| pd.in_vehicle).unwrap_or(ped))
    }

    /// `COMMAND_TURN_CHAR_TO_FACE_CHAR` and its relatives: the ped turns towards the other one when it is on foot.
    fn turn_to_face(&mut self, source: EntityRef, target: EntityRef) {
        let (sp, tp) = (self.locate_of(source), self.locate_of(target));
        let mut angle = atan_of_xy(sp.x - tp.x, sp.y - tp.y) + FRAC_PI_2;
        if angle > TAU {
            angle -= TAU;
        }
        if !self.ped(source).in_vehicle {
            let pd = self.ped_mut(source);
            pd.rotation_cur = angle;
            pd.rotation_dest = angle;
            self.get_mut(source).unwrap().base.set_heading(angle);
        }
    }

    /// Two opposite corners in order, the way the garage and road commands want them.
    fn ordered_box(&self) -> (Vec3, Vec3) {
        let (a, b) = (self.spv(0), self.spv(3));
        (a.min(b), a.max(b))
    }

    pub(crate) fn process_commands_500_to_599(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::IS_CAR_UPSIDEDOWN | c::IS_CAR_UPRIGHT => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_UPSIDEDOWN: no such car");
                let up = self.ent(car).base.matrix.up.z;
                s.update_compare_flag(if command == c::IS_CAR_UPSIDEDOWN { up <= -0.97 } else { up >= 0.0 });
            }
            c::GET_PLAYER_CHAR => {
                self.collect(s, 1);
                self.ents.scripts.params[0] = self.ents.player_ped.expect("GET_PLAYER_CHAR: no player").handle;
                self.store(s, 1);
            }
            c::CANCEL_OVERRIDE_RESTART => self.ents.restart.cancel_override_restart(),
            c::SET_POLICE_IGNORE_PLAYER => {
                self.collect(s, 2);
                let ignore = self.sp(1) != 0;
                if let Some(w) = self.player_wanted_mut() {
                    w.ignored_by_cops = ignore;
                }
                if ignore {
                    self.stop_all_law_enforcers_in_their_tracks();
                }
            }
            c::ADD_PAGER_MESSAGE_WITH_NUMBER => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 4);
                sh.add_pager_message_with_number(&key, self.sp(0), self.sp(1), self.sp(2), self.sp(3));
            }
            c::START_KILL_FRENZY => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 8);
                let p = self.ents.scripts.params;
                let proper = key.eq_ignore_ascii_case("PAGE_00");
                self.start_frenzy(px, p[0] as usize, p[1], p[2] as u16, p[3], Some(key), proper, p[4], p[5], p[6], p[7] != 0, false);
            }
            c::READ_KILL_FRENZY_STATUS => {
                self.ents.scripts.params[0] = i32::from(self.ents.darkel.status);
                self.store(s, 1);
            }
            c::SQRT => {
                self.collect(s, 1);
                let r = self.spf(0).sqrt();
                self.ents.scripts.set_param_f(0, r);
                self.store(s, 1);
            }
            c::GENERATE_RANDOM_FLOAT_IN_RANGE => {
                self.collect(s, 2);
                let r = px.rng.range_f32(self.spf(0), self.spf(1));
                self.ents.scripts.set_param_f(0, r);
                self.store(s, 1);
            }
            c::GENERATE_RANDOM_INT_IN_RANGE => {
                self.collect(s, 2);
                self.ents.scripts.params[0] = px.rng.range_i32(self.sp(0), self.sp(1));
                self.store(s, 1);
            }
            c::LOCK_CAR_DOORS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("LOCK_CAR_DOORS: no such car");
                self.try_veh_mut(car).unwrap().door_lock = car_lock_of(self.sp(1));
            }
            c::EXPLODE_CAR => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("EXPLODE_CAR: no such car");
                self.blow_up_car_entity(car, None);
            }
            c::ADD_EXPLOSION => {
                self.collect(s, 4);
                let camera = px.hooks.camera_position();
                self.add_explosion(px.rng, camera, None, None, explosion_type_of(self.sp(3)), self.spv(0), 0);
            }
            c::TURN_CHAR_TO_FACE_CHAR | c::TURN_CHAR_TO_FACE_PLAYER | c::TURN_PLAYER_TO_FACE_CHAR => {
                self.collect(s, 2);
                let player = self.ents.player_ped.expect("player");
                let source = if command == c::TURN_PLAYER_TO_FACE_CHAR { player } else { self.script_ped(self.sp(0)).expect("TURN_*: no such ped") };
                let target = if command == c::TURN_CHAR_TO_FACE_PLAYER { player } else { self.script_ped(self.sp(1)).expect("TURN_*: no such target ped") };
                self.turn_to_face(source, target);
            }
            c::SET_CHAR_OBJ_GOTO_COORD_ON_FOOT => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_OBJ_GOTO_COORD_ON_FOOT: no such ped");
                let (x, y) = (self.spf(1), self.spf(2));
                let target = Vec3::new(x, y, self.find_ground_z_for_coord(x, y));
                self.ped_mut(ped).flags.script_objective_completed = false;
                self.set_objective_pos(ped, objective::GOTO_AREA_ON_FOOT, target);
            }
            c::CREATE_PICKUP => {
                self.collect(s, 5);
                let wanted = self.sp(0);
                let model = if wanted < 0 { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1) } else { wanted } as i16;
                let mut pos = self.spv(2);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y) + 0.5;
                }
                let ty = self.sp(1) as u8;
                let handle = self.generate_new_pickup(px, pos, model, ty, 0);
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::HAS_PICKUP_BEEN_COLLECTED => {
                self.collect(s, 1);
                let id = self.sp(0);
                s.update_compare_flag(self.ents.pickups.is_pick_up_picked_up(id));
            }
            c::REMOVE_PICKUP => {
                self.collect(s, 1);
                self.remove_pick_up(self.sp(0));
            }
            c::SET_TAXI_LIGHTS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_TAXI_LIGHTS: no such car");
                let on = self.sp(1) != 0;
                self.car_mut(car).expect("a car").flags.taxi_light = on;
            }
            c::SET_ALL_TAXI_LIGHTS => {
                self.collect(s, 1);
                self.ents.all_taxi_lights = self.sp(0) != 0;
            }
            c::PRINT_BIG_Q => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 2);
                sh.print_big_queued(&key, self.sp(0) as u32, self.sp(1) - 1);
            }
            c::PRINT_WITH_NUMBER_BIG_Q => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 3);
                sh.print_big_with_number_queued(&key, self.sp(1) as u32, self.sp(2) - 1, self.sp(0));
            }
            c::SET_GARAGE | c::SET_GARAGE_WITH_CAR_MODEL => {
                self.collect(s, if command == c::SET_GARAGE { 7 } else { 8 });
                let (lo, hi) = self.ordered_box();
                let (ty, target) = (self.sp(6) as u8, if command == c::SET_GARAGE { 0 } else { self.sp(7) });
                self.ents.scripts.params[0] = self.add_garage(lo, hi, ty, target) as i32;
                self.store(s, 1);
            }
            c::SET_TARGET_CAR_FOR_MISSION_GARAGE => {
                self.collect(s, 2);
                let target = if self.sp(1) >= 0 { Some(self.script_vehicle(self.sp(1)).expect("SET_TARGET_CAR_FOR_MISSION_GARAGE: no such car")) } else { None };
                self.ents.garages.set_target_car_for_misson_garage(self.sp(0) as usize, target);
            }
            c::IS_CAR_IN_MISSION_GARAGE => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.garages.has_car_been_dropped_off_yet(self.sp(0) as usize));
            }
            c::SET_FREE_BOMBS => {
                self.collect(s, 1);
                self.ents.garages.bombs_are_free = self.sp(0) != 0;
            }
            c::IS_CAR_ARMED_WITH_ANY_BOMB => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_CAR_ARMED_WITH_ANY_BOMB: no such car");
                s.update_compare_flag(self.car_ref(car).expect("a car").bomb_type != Default::default());
            }
            c::APPLY_BRAKES_TO_PLAYERS_CAR => {
                self.collect(s, 2);
                sh.apply_brakes_to_players_car(self.sp(0), self.sp(1) != 0);
            }
            c::SET_PLAYER_HEALTH | c::SET_CHAR_HEALTH => {
                self.collect(s, 2);
                let ped = if command == c::SET_PLAYER_HEALTH { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("SET_CHAR_HEALTH: no such ped") };
                let health = self.sp(1);
                if health != 0 || command == c::SET_PLAYER_HEALTH {
                    self.ped_mut(ped).health = health as f32;
                } else if self.ped(ped).in_vehicle {
                    self.set_dead(px, ped);
                    if !self.ped(ped).is_player() {
                        self.flag_to_destroy_when_next_processed(ped);
                    }
                } else {
                    self.set_die(px, ped, crate::anim::tables_gta3::anim::STD_KO_FRONT, 4.0, 0.0);
                }
            }
            c::SET_CAR_HEALTH => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_HEALTH: no such car");
                self.try_veh_mut(car).unwrap().health = self.sp(1) as f32;
            }
            c::GET_PLAYER_HEALTH | c::GET_CHAR_HEALTH => {
                self.collect(s, 1);
                let ped = if command == c::GET_PLAYER_HEALTH { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("GET_CHAR_HEALTH: no such ped") };
                self.ents.scripts.params[0] = self.ped(ped).health as i32;
                self.store(s, 1);
            }
            c::GET_CAR_HEALTH => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("GET_CAR_HEALTH: no such car");
                self.ents.scripts.params[0] = self.try_veh(car).unwrap().health as i32;
                self.store(s, 1);
            }
            _ => return self.process_commands_500_to_599_rest(px, sh, s, command),
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

    #[derive(Default)]
    struct Spy {
        brakes: Vec<(i32, bool)>,
    }
    impl ScriptHooks for Spy {
        fn apply_brakes_to_players_car(&mut self, pad: i32, on: bool) {
            self.brakes.push((pad, on));
        }
    }

    fn world(a: &Asm) -> World {
        let mut w = world_with_script(a);
        w.ents.scripts.space[..a.bytes.len()].copy_from_slice(&a.bytes);
        let p = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ped_mut(p).health = 100.0;
        w.ents.player_ped = Some(p);
        w
    }

    fn g(w: &World, o: u32) -> i32 {
        w.ents.scripts.global(o)
    }

    #[test]
    fn health_numbers_and_random_ranges() {
        let mut a = Asm::new(6);
        a.cmd(c::SET_PLAYER_HEALTH, &[A::I8(0), A::I8(55)]);
        a.cmd(c::GET_PLAYER_HEALTH, &[A::I8(0), A::G(8)]);
        a.cmd(c::SET_VAR_FLOAT, &[A::G(12), A::F(16.0)]);
        a.cmd(c::SQRT, &[A::G(12), A::G(12)]);
        a.cmd(c::GENERATE_RANDOM_INT_IN_RANGE, &[A::I8(10), A::I8(20), A::G(16)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(g(&w, 8), 55);
        assert_eq!(super::super::f32_of(g(&w, 12)), 4.0);
        assert!((10..20).contains(&g(&w, 16)));
    }

    #[test]
    fn a_pickup_is_created_and_taken_and_a_garage_is_added() {
        let mut a = Asm::new(6);
        a.cmd(c::CREATE_PICKUP, &[A::I16(crate::modelinfo::indices_gta3::mi::COLT), A::I8(2), A::F(10.0), A::F(0.0), A::F(0.0), A::G(8)]);
        a.cmd(c::SET_GARAGE, &[A::F(10.0), A::F(0.0), A::F(0.0), A::F(0.0), A::F(10.0), A::F(5.0), A::I8(5), A::G(12)]);
        a.cmd(c::REMOVE_PICKUP, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(g(&w, 8) & 0xFFFF, 0);
        assert_eq!(w.ents.pickups.pickups[0].ty, crate::control::pickups::pickup_type::NONE, "removed by the same script");
        let garage = g(&w, 12) as usize;
        assert_eq!((w.ents.garages.garages[garage].ty, w.ents.garages.garages[garage].x1, w.ents.garages.garages[garage].x2), (5, 0.0, 10.0));
    }

    #[test]
    fn cars_are_exploded_locked_and_turned_to_face_peds() {
        let mut a = Asm::new(4);
        a.cmd(c::LOCK_CAR_DOORS, &[A::G(8), A::I8(2)]);
        a.cmd(c::IS_CAR_UPRIGHT, &[A::G(8)]);
        a.cmd(c::APPLY_BRAKES_TO_PLAYERS_CAR, &[A::I8(0), A::I8(1)]);
        a.cmd(c::TURN_CHAR_TO_FACE_PLAYER, &[A::G(12), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut Rng::default()).unwrap();
        let ped = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
        w.get_mut(ped).unwrap().base.set_position(Vec3::new(10.0, 0.0, 0.0));
        w.ents.scripts.set_global(8, car.handle);
        w.ents.scripts.set_global(12, ped.handle);
        let i = start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert_eq!(w.try_veh(car).unwrap().door_lock, CarLock::Locked);
        assert!(w.ents.scripts.scripts[i].cond_result, "the car is upright");
        assert_eq!(spy.brakes, vec![(0, true)]);
        // the ped now faces the player (at the origin), along the x axis
        assert!((w.ped(ped).rotation_cur - w.ped(ped).rotation_dest).abs() < 1e-6);
    }
}
