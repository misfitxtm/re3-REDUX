//! `Script4.cpp`, commands 800 to 899: zones, random cars, on-screen text, armour, garages and kill frenzies.
//! (Rotating and sliding objects, the chase scene, planes, dressing, warping into cars and a few more come later.)
use super::commands as c;
use super::{RunningScript, ScriptHooks};
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::slot;
use crate::ped::ctx::Px;
use crate::ped::data::objective;
use crate::ped::types::ped_type;
use crate::vehicle::base::{MISSION_VEHICLE, RANDOM_VEHICLE};
use crate::world::{Kinds, World};
use glam::Vec3;

const MAP_Z_LOW_LIMIT: f32 = -100.0;

impl World {
    /// The line of intro text the next `SET_TEXT_*` command changes.
    /// `CExplosion::TestForExplosionInArea` for the numbered type a script gives.
    fn explosion_in_area(&self, ty: i32, min: Vec3, max: Vec3) -> bool {
        use crate::weapons::explosion::ExplosionType as E;
        let ty = [E::Grenade, E::Molotov, E::Rocket, E::Car, E::CarQuick, E::Heli, E::Mine, E::Barrel, E::TankGrenade, E::HeliBomb].get(ty as usize).copied();
        ty.is_some_and(|ty| self.ents.explosions.test_in_area(ty, min, max))
    }

    /// Whether a car, ped or object is in the way of an object that is about to be turned or moved: its collision box at `matrix` is tested against them.
    fn object_blocked_at(&mut self, obj: EntityRef, matrix: crate::math::Matrix) -> bool {
        let Some(col) = self.col_model_of(obj) else { return false };
        let b = col.bounding_box;
        let corners = [b.min, Vec3::new(b.max.x, b.min.y, b.min.z), Vec3::new(b.min.x, b.max.y, b.min.z), Vec3::new(b.min.x, b.min.y, b.max.z)].map(|c| matrix.transform_point(c));
        let lo = corners.iter().fold(Vec3::splat(f32::MAX), |a, &c| a.min(c));
        let hi = corners.iter().fold(Vec3::splat(f32::MIN), |a, &c| a.max(c));
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        !self.find_objects_intersecting_angled_collision_box(&b, &matrix, matrix.pos, (lo.x, lo.y), (hi.x, hi.y), 2, kinds).is_empty()
    }

    /// `ROTATE_OBJECT`: one step of turning an object to a heading; true when it has got there or is stopped by something.
    fn script_rotate_object(&mut self, s: &mut RunningScript) {
        let obj = self.script_object(self.sp(0)).expect("ROTATE_OBJECT: no such object");
        let limit = |a: f32| if a < 0.0 { a + 360.0 } else { a };
        let forward = self.ent(obj).base.matrix.forward;
        let heading = limit((-forward.x).atan2(forward.y).to_degrees());
        let target = self.spf(1);
        let by = self.spf(2) * self.ents.timer.time_step_fix();
        if target == heading {
            s.update_compare_flag(true);
            return;
        }
        let (clockwise, counter) = (limit(target - heading), limit(heading - target));
        let new = if clockwise < counter {
            if by < clockwise { heading + by } else { target }
        } else if by < counter {
            heading - by
        } else {
            target
        };
        if self.sp(3) != 0 {
            let mut m = crate::math::Matrix::rotate_z_only(new.to_radians());
            m.pos = self.epos(obj);
            if self.object_blocked_at(obj, m) {
                s.update_compare_flag(true);
                return;
            }
        }
        self.ent_mut(obj).base.set_heading(new.to_radians());
        s.update_compare_flag(new == target);
    }

    /// `SLIDE_OBJECT`: one step of moving an object to a point, each axis by its own speed.
    fn script_slide_object(&mut self, s: &mut RunningScript) {
        let obj = self.script_object(self.sp(0)).expect("SLIDE_OBJECT: no such object");
        let (pos, target) = (self.epos(obj), self.spv(1));
        let by = self.spv(4) * self.ents.timer.time_step_fix();
        if target == pos {
            s.update_compare_flag(true);
            return;
        }
        let axis = |from: f32, to: f32, by: f32| {
            let diff = from - to;
            if diff < 0.0 {
                if -diff < by { to } else { from + by }
            } else if diff < by {
                to
            } else {
                from - by
            }
        };
        let new = Vec3::new(axis(pos.x, target.x, by.x), axis(pos.y, target.y, by.y), axis(pos.z, target.z, by.z));
        if self.sp(7) != 0 {
            let mut m = self.ent(obj).base.matrix;
            m.pos = new;
            if self.object_blocked_at(obj, m) {
                s.update_compare_flag(true);
                return;
            }
        }
        self.teleport_object(obj, new);
        s.update_compare_flag(new == target);
    }

    /// `CREATE_RANDOM_CHAR`: a civilian of the player's zone, loaded when possible.
    fn script_create_random_char(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript) {
        use crate::modelinfo::indices_gta3::mi;
        let zone = self.ents.zones.get_zone_info_for_time_of_day(self.player_info_pos(), &self.ents.clock);
        let (mut model, mut ty, mut attempt) = (-1, ped_type::COP, 0);
        while ty != ped_type::CIVMALE && ty != ped_type::CIVFEMALE && attempt < 5 {
            model = self.ents.population.choose_civilian_occupation(px.rng, zone.ped_group as usize);
            if sh.has_model_loaded_for_script(model as i16) {
                ty = self.ents.models.get(model).and_then(|m| match &m.kind { crate::modelinfo::Kind::Ped(p) => Some(p.ped_type as usize), _ => None }).unwrap_or(ped_type::COP);
            }
            attempt += 1;
        }
        if !sh.has_model_loaded_for_script(model as i16) {
            model = mi::MALE01 as i32;
            ty = self.ents.models.get(model).and_then(|m| match &m.kind { crate::modelinfo::Kind::Ped(p) => Some(p.ped_type as usize), _ => None }).unwrap_or(ped_type::CIVMALE);
        }
        let clump = px.hooks.load_ped_clump(model as i16);
        let Some(ped) = self.create_civilian(px, ty, model as i16, clump) else { return };
        let pd = self.ped_mut(ped);
        pd.char_created_by = crate::ped::data::MISSION_CHAR;
        pd.flags.responds_to_threats = false;
        pd.flags.allow_medics_to_revive_me = false;
        let mut pos = self.spv(0);
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        pos.z += 1.0;
        let e = self.ent_mut(ped);
        e.base.set_position(pos);
        e.base.set_orientation(0.0, 0.0, 0.0);
        px.hooks.clear_space_for_mission_entity(pos, ped);
        let _ = self.add(ped);
        let level = self.ents.zones.get_level_from_position(pos);
        if let Some(p) = self.ent_mut(ped).phys.as_mut() {
            p.zone_level = level as i8;
        }
        self.ents.population.total_mission_peds += 1;
        self.ents.scripts.params[0] = ped.handle;
        self.store(s, 1);
        if s.is_mission_script {
            self.ents.scripts.cleanup.add_entity_to_list(ped.handle, super::CLEANUP_CHAR);
        }
    }

    /// What `WARP_PLAYER_FROM_CAR_TO_COORD and `WARP_CHAR_FROM_CAR_TO_COORD` share: the ped leaves its car and stands at the point.
    pub(crate) fn warp_ped_from_car_to_coord(&mut self, px: &mut Px, ped: EntityRef, mut pos: Vec3, is_char: bool) {
        use crate::ped::data::{move_state, state};
        if pos.z <= MAP_Z_LOW_LIMIT {
            pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
        }
        if let (true, Some(car)) = (self.ped(ped).in_vehicle, self.ped(ped).my_vehicle) {
            if self.try_veh(car).is_some_and(|v| v.flags.is_bus) {
                self.ped_mut(ped).flags.render_ped_in_car = true;
            }
            let v = self.try_veh_mut(car).expect("the car of a ped in a car");
            if v.driver == Some(ped) {
                v.remove_driver(crate::entity::status::PHYSICS);
                v.flags.engine_on = false;
                v.auto_pilot.cruise_speed = 0;
                self.ent_mut(car).base.status = crate::entity::status::ABANDONED;
            } else {
                v.remove_passenger(ped);
            }
            if is_char && let Some(p) = self.ent_mut(car).phys.as_mut() {
                (p.move_speed, p.turn_speed) = (Vec3::new(0.0, 0.0, -0.00001), Vec3::ZERO);
            }
        }
        let pd = self.ped_mut(ped);
        (pd.in_vehicle, pd.my_vehicle, pd.ped_state) = (false, None, state::IDLE);
        if is_char {
            pd.last_ped_state = state::NONE;
        }
        self.ent_mut(ped).base.flags.uses_collision = true;
        if let Some(p) = self.ent_mut(ped).phys.as_mut() {
            p.move_speed = Vec3::ZERO;
        }
        let weapon = self.ped(ped).weapons[self.ped(ped).current_weapon as usize].ty;
        let model = self.ents.weapon_infos.get(weapon).model_id;
        self.add_weapon_model(ped, model);
        self.remove_in_car_anims(ped);
        if let Some(a) = self.ped(ped).vehicle_anim.and_then(|id| self.ped_assoc_mut(ped, id as u32)) {
            a.blend_delta = -1000.0;
        }
        self.ped_mut(ped).vehicle_anim = None;
        self.restart_non_partial_anims(ped);
        self.ped_mut(ped).set_move_state(move_state::NONE);
        let group = self.ped(ped).anim_group as usize;
        self.ped_blend_anim(ped, group, crate::anim::tables_gta3::anim::STD_IDLE, 100.0);
        pos.z += self.base_offset_of(ped);
        self.teleport(ped, pos);
        px.hooks.clear_space_for_mission_entity(pos, ped);
    }

    pub(crate) fn intro_line(&mut self) -> &mut super::text::IntroTextLine {
        let sc = &mut self.ents.scripts;
        let i = sc.number_of_intro_text_lines_this_frame.min(super::text::MAX_NUM_INTRO_TEXT_LINES - 1);
        &mut sc.intro_text_lines[i]
    }

    /// A random car of a model becomes the script's: every matching one does, as in the original.
    fn script_take_random_cars(&mut self, s: &RunningScript, model: i32, inside: impl Fn(&World, Vec3) -> bool) -> i32 {
        let mut handle = -1;
        for v in self.vehicle_refs().into_iter().rev() {
            let (m, created_by, pos) = (i32::from(self.ent(v).base.model_index), self.try_veh(v).unwrap().created_by, self.epos(v));
            if (model != m && model >= 0) || created_by != RANDOM_VEHICLE || !inside(self, pos) {
                continue;
            }
            handle = v.handle;
            self.try_veh_mut(v).unwrap().created_by = MISSION_VEHICLE;
            self.ents.car_ctrl.num_mission_cars += 1;
            self.ents.car_ctrl.num_random_cars -= 1;
            if s.is_mission_script {
                self.ents.scripts.cleanup.add_entity_to_list(handle, super::CLEANUP_CAR);
            }
        }
        handle
    }

    pub(crate) fn process_commands_800_to_899(&mut self, px: &mut Px, sh: &mut dyn ScriptHooks, s: &mut RunningScript, command: u16) -> i8 {
        match command {
            c::IS_CHAR_IN_PLAYERS_GROUP => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("IS_CHAR_IN_PLAYERS_GROUP: no such ped");
                s.update_compare_flag(self.ped(ped).leader == self.ents.player_ped);
            }
            c::EXPLODE_CHAR_HEAD | c::EXPLODE_PLAYER_HEAD => {
                let player = command == c::EXPLODE_PLAYER_HEAD;
                self.collect(s, 1);
                let ped = if player { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("EXPLODE_CHAR_HEAD: no such ped") };
                let nasty = px.hooks.nasty_game();
                let at = self.epos(ped);
                if !player && self.ped(ped).ped_state == crate::ped::data::state::DRIVING {
                    self.set_dead(px, ped);
                    if !self.ped(ped).is_player() {
                        self.flag_to_destroy_when_next_processed(ped);
                    }
                } else if nasty && (player || self.ped(ped).is_ped_in_control()) {
                    self.apply_head_shot(px, ped, at, true);
                } else {
                    self.set_die(px, ped, crate::anim::tables_gta3::anim::STD_KO_FRONT, 4.0, 0.0);
                }
            }
            c::ANCHOR_BOAT => {
                self.collect(s, 2);
                let boat = self.script_vehicle(self.sp(0)).expect("ANCHOR_BOAT: no such boat");
                let on = self.sp(1) != 0;
                if let crate::vehicle::base::VehicleSub::Boat(b) = &mut self.try_veh_mut(boat).unwrap().sub {
                    b.is_anchored = on;
                }
            }
            c::SET_ZONE_GROUP => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 2);
                if let Some(zone) = self.ents.zones.find_zone_by_label_and_return_index(&label) {
                    self.ents.zones.set_ped_group(zone, self.sp(0) != 0, self.sp(1) as u16);
                }
            }
            c::GET_RANDOM_CAR_OF_TYPE_IN_AREA => {
                self.collect(s, 5);
                let (a, b, model) = ((self.spf(0), self.spf(1)), (self.spf(2), self.spf(3)), self.sp(4));
                let handle = self.script_take_random_cars(s, model, |_, p| p.x >= a.0.min(b.0) && p.x <= a.0.max(b.0) && p.y >= a.1.min(b.1) && p.y <= a.1.max(b.1));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::GET_RANDOM_CAR_OF_TYPE_IN_ZONE => {
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let zone = self.ents.zones.find_zone_by_label_and_return_index(&label);
                self.collect(s, 1);
                let model = self.sp(0);
                let handle = self.script_take_random_cars(s, model, |w, p| zone.is_some_and(|z| crate::zones::point_lies_within_zone(p, &w.ents.zones.zone_array[z])));
                self.ents.scripts.params[0] = handle;
                self.store(s, 1);
            }
            c::HAS_RESPRAY_HAPPENED => {
                self.collect(s, 1);
                s.update_compare_flag(self.ents.garages.has_respray_happened(self.sp(0) as usize));
            }
            c::SET_CAMERA_ZOOM => {
                self.collect(s, 1);
                sh.set_camera_zoom(self.sp(0));
            }
            c::CREATE_PICKUP_WITH_AMMO => {
                self.collect(s, 6);
                let wanted = self.sp(0);
                let model = if wanted < 0 { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1 as i16) } else { wanted as i16 };
                let mut pos = self.spv(3);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y) + 0.5;
                }
                let (ty, ammo) = (self.sp(1) as u8, self.sp(2) as u32);
                self.ents.scripts.params[0] = self.generate_new_pickup(px, pos, model, ty, ammo);
                self.store(s, 1);
            }
            c::CREATE_FLOATING_PACKAGE => {
                self.collect(s, 3);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y) + 0.5;
                }
                let model = self.ents.indices.gta3_slot(slot::FLOATPACKAGE1);
                self.ents.scripts.params[0] = self.generate_new_pickup(px, pos, model, crate::control::pickups::pickup_type::FLOATINGPACKAGE, 0);
                self.store(s, 1);
            }
            c::SET_CAR_RAM_CAR | c::SET_CAR_BLOCK_CAR => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_RAM_CAR: no such car");
                let target = self.script_vehicle(self.sp(1)).expect("SET_CAR_RAM_CAR: no such target car");
                if command == c::SET_CAR_RAM_CAR {
                    self.tell_car_to_ram_other_car(car, target);
                } else {
                    self.tell_car_to_block_other_car(car, target);
                }
            }
            c::SET_CHAR_OBJ_CATCH_TRAIN | c::SET_CHAR_OBJ_HAIL_TAXI | c::SET_CHAR_OBJ_STEAL_ANY_CAR => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("objective command: no such ped");
                self.ped_mut(ped).flags.script_objective_completed = false;
                let o = match command {
                    c::SET_CHAR_OBJ_CATCH_TRAIN => objective::CATCH_TRAIN,
                    c::SET_CHAR_OBJ_HAIL_TAXI => objective::HAIL_TAXI,
                    _ => objective::STEAL_ANY_CAR,
                };
                self.set_objective(px, ped, o);
            }
            c::SET_PLAYER_NEVER_GETS_TIRED => {
                self.collect(s, 2);
                self.ents.player_info.infinite_sprint = self.sp(1) != 0;
            }
            c::SET_PLAYER_FAST_RELOAD => {
                self.collect(s, 2);
                self.ents.player_info.fast_reload = self.sp(1) != 0;
            }
            c::SET_CHAR_BLEEDING => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_BLEEDING: no such ped");
                self.ped_mut(ped).flags.ped_is_bleeding = self.sp(1) != 0;
            }
            c::SET_CAR_FUNNY_SUSPENSION => {
                self.collect(s, 2);
                self.script_vehicle(self.sp(0)).expect("SET_CAR_FUNNY_SUSPENSION: no such car");
            }
            c::SET_CAR_BIG_WHEELS => {
                self.collect(s, 2);
                let car = self.script_vehicle(self.sp(0)).expect("SET_CAR_BIG_WHEELS: no such car");
                let on = self.sp(1) != 0;
                self.car_mut(car).expect("a car").flags.big_wheels = on;
            }
            c::SET_FREE_RESPRAYS => {
                self.collect(s, 1);
                self.ents.garages.resprays_are_free = self.sp(0) != 0;
            }
            c::SET_PLAYER_VISIBLE | c::SET_CHAR_VISIBLE | c::SET_CAR_VISIBLE => {
                self.collect(s, 2);
                let e = match command {
                    c::SET_PLAYER_VISIBLE => self.ents.player_ped,
                    c::SET_CHAR_VISIBLE => self.script_ped(self.sp(0)),
                    _ => self.script_vehicle(self.sp(0)),
                }
                .expect("SET_*_VISIBLE: no such entity");
                let on = self.sp(1) != 0;
                self.get_mut(e).unwrap().base.flags.is_visible = on;
            }
            c::IS_AREA_OCCUPIED => {
                self.collect(s, 11);
                let (a, b) = (self.spv(0), self.spv(3));
                let p = self.ents.scripts.params;
                let kinds = Kinds { buildings: p[6] != 0, vehicles: p[7] != 0, peds: p[8] != 0, objects: p[9] != 0, dummies: p[10] != 0 };
                let found = self.find_objects_intersecting_cube(a.min(b), a.max(b), 2, kinds);
                s.update_compare_flag(!found.is_empty());
            }
            c::SAVE_PLAYER_FROM_FIRES => {
                self.collect(s, 1);
                let at = self.player_info_pos();
                self.extinguish_fires_at(px, at, 3.0);
            }
            c::DISPLAY_TEXT => {
                self.collect(s, 2);
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                let (x, y) = (self.spf(0), self.spf(1));
                let line = self.intro_line();
                (line.at_x, line.at_y, line.text) = (x, y, key);
                let sc = &mut self.ents.scripts;
                sc.number_of_intro_text_lines_this_frame += 1;
            }
            c::SET_TEXT_SCALE => {
                self.collect(s, 2);
                let (x, y) = (self.spf(0), self.spf(1));
                let l = self.intro_line();
                (l.scale_x, l.scale_y) = (x, y);
            }
            c::SET_TEXT_COLOUR | c::SET_TEXT_BACKGROUND_COLOUR => {
                self.collect(s, 4);
                let p = self.ents.scripts.params;
                let rgba = [p[0] as u8, p[1] as u8, p[2] as u8, p[3] as u8];
                let l = self.intro_line();
                if command == c::SET_TEXT_COLOUR {
                    l.colour = rgba;
                } else {
                    l.background_colour = rgba;
                }
            }
            c::SET_TEXT_JUSTIFY
            | c::SET_TEXT_CENTRE
            | c::SET_TEXT_BACKGROUND
            | c::SET_TEXT_BACKGROUND_ONLY_TEXT
            | c::SET_TEXT_PROPORTIONAL
            | c::SET_TEXT_WRAPX
            | c::SET_TEXT_CENTRE_SIZE
            | c::SET_TEXT_FONT => {
                self.collect(s, 1);
                let (i, f) = (self.sp(0), self.spf(0));
                let l = self.intro_line();
                match command {
                    c::SET_TEXT_JUSTIFY => l.justify = i != 0,
                    c::SET_TEXT_CENTRE => l.centered = i != 0,
                    c::SET_TEXT_BACKGROUND => l.background = i != 0,
                    c::SET_TEXT_BACKGROUND_ONLY_TEXT => l.background_only = i != 0,
                    c::SET_TEXT_PROPORTIONAL => l.text_proportional = i != 0,
                    c::SET_TEXT_WRAPX => l.wrap_x = f,
                    c::SET_TEXT_CENTRE_SIZE => l.centre_size = f,
                    _ => l.font = i,
                }
            }
            c::INDUSTRIAL_PASSED | c::COMMERCIAL_PASSED | c::SUBURBAN_PASSED => sh.level_passed(match command {
                c::INDUSTRIAL_PASSED => 0,
                c::COMMERCIAL_PASSED => 1,
                _ => 2,
            }),
            c::REMOVE_CHAR_ELEGANTLY => {
                self.collect(s, 1);
                self.script_remove_char_elegantly(px, self.sp(0));
                if s.is_mission_script {
                    self.ents.scripts.cleanup.remove_entity_from_list(self.sp(0), super::CLEANUP_CHAR);
                }
            }
            c::SET_CHAR_STAY_IN_SAME_PLACE => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_STAY_IN_SAME_PLACE: no such ped");
                self.ped_mut(ped).flags.kinda_stay_in_same_place = self.sp(1) != 0;
            }
            c::IS_NASTY_GAME => s.update_compare_flag(px.hooks.nasty_game()),
            c::ADD_ARMOUR_TO_PLAYER | c::ADD_ARMOUR_TO_CHAR => {
                self.collect(s, 2);
                let ped = if command == c::ADD_ARMOUR_TO_PLAYER { self.ents.player_ped.expect("player") } else { self.script_ped(self.sp(0)).expect("ADD_ARMOUR_TO_CHAR: no such ped") };
                let add = self.sp(1) as f32;
                let pd = self.ped_mut(ped);
                pd.armour = (pd.armour + add).clamp(0.0, 100.0);
            }
            c::OPEN_GARAGE => {
                self.collect(s, 1);
                self.ents.garages.open_garage(self.sp(0) as usize);
            }
            c::CLOSE_GARAGE => {
                self.collect(s, 1);
                self.ents.garages.close_garage(self.sp(0) as usize);
            }
            c::HAS_CHAR_SPOTTED_CHAR => {
                self.collect(s, 2);
                let ped = self.script_ped(self.sp(0)).expect("HAS_CHAR_SPOTTED_CHAR: no such ped");
                let target = self.script_ped(self.sp(1)).expect("HAS_CHAR_SPOTTED_CHAR: no such target");
                let seen = self.our_ped_can_see_this_one(ped, target);
                s.update_compare_flag(seen);
            }
            c::HAS_OBJECT_BEEN_DAMAGED => {
                self.collect(s, 1);
                let obj = self.script_object(self.sp(0)).expect("HAS_OBJECT_BEEN_DAMAGED: no such object");
                let f = &self.ent(obj).base.flags;
                s.update_compare_flag(f.render_damaged || !f.is_visible);
            }
            c::START_KILL_FRENZY_HEADSHOT => {
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, 8);
                let p = self.ents.scripts.params;
                let proper = key.eq_ignore_ascii_case("PAGE_00");
                self.start_frenzy(px, p[0] as usize, p[1], p[2] as u16, p[3], Some(key), proper, p[4], p[5], p[6], p[7] != 0, true);
            }
            c::ACTIVATE_MILITARY_CRANE => {
                self.collect(s, 10);
                let (a, b) = ((self.spf(2), self.spf(3)), (self.spf(4), self.spf(5)));
                let drop = Vec3::new(self.spf(6), self.spf(7), self.spf(8));
                let (heading, pick_x, pick_y) = (self.spf(9).to_radians(), self.spf(0), self.spf(1));
                self.activate_crane(a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1), drop, heading, false, true, pick_x, pick_y);
            }
            c::PRINT_WITH_2_NUMBERS_BIG..=c::PRINT_WITH_6_NUMBERS_BIG => {
                let count = 2 + (command - c::PRINT_WITH_2_NUMBERS_BIG) as usize;
                let key = self.ents.scripts.read_text_label(&mut s.ip);
                self.collect(s, count + 2);
                let p = self.ents.scripts.params;
                sh.print_big_with_numbers(&key, p[count] as u32, p[count + 1] - 1, &p[..count]);
            }
            c::SET_CAMERA_BEHIND_PLAYER => px.hooks.camera_directly_behind_ped(),
            c::SET_MOTION_BLUR => {
                self.collect(s, 1);
                sh.set_motion_blur(self.sp(0));
            }
            _ if self.phone_messages_command(s, command) => {}
            c::START_CAR_FIRE | c::START_CHAR_FIRE => {
                self.collect(s, 1);
                let target = if command == c::START_CAR_FIRE { self.script_vehicle(self.sp(0)) } else { self.script_ped(self.sp(0)) }.expect("START_x_FIRE: no such entity");
                let pos = self.epos(target);
                self.ents.scripts.params[0] = self.start_script_fire(px, pos, Some(target), 0.8, true);
                self.store(s, 1);
            }
            c::START_CHASE_SCENE => {
                self.collect(s, 1);
                sh.start_chase_scene(self.spf(0));
            }
            c::STOP_CHASE_SCENE => sh.stop_chase_scene(),
            c::START_DRUG_RUN => sh.start_drug_run(),
            c::HAS_DRUG_RUN_BEEN_COMPLETED => s.update_compare_flag(sh.drug_run_completed()),
            c::HAS_DRUG_PLANE_BEEN_SHOT_DOWN => s.update_compare_flag(sh.drug_plane_shot_down()),
            c::START_DRUG_DROP_OFF => sh.start_drug_drop_off(),
            c::HAS_DROP_OFF_PLANE_BEEN_SHOT_DOWN => s.update_compare_flag(sh.drop_off_plane_shot_down()),
            c::FIND_DROP_OFF_PLANE_COORDINATES => {
                let at = sh.find_drop_off_plane_coordinates();
                self.ents.scripts.set_param_v3(0, at);
                self.store(s, 3);
            }
            c::UNDRESS_CHAR => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("UNDRESS_CHAR: no such ped");
                let name = self.ents.scripts.read_text_label(&mut s.ip).to_lowercase();
                sh.delete_ped_rw_object(ped.handle);
                let model = if self.ped(ped).is_player() { 0 } else { self.ent(ped).base.model_index as i32 };
                sh.request_special_model(model, &name);
                self.remove(ped);
            }
            c::DRESS_CHAR => {
                self.collect(s, 1);
                let ped = self.script_ped(self.sp(0)).expect("DRESS_CHAR: no such ped");
                sh.recreate_ped_rw_object(ped.handle);
                let _ = self.add(ped);
            }
            c::ROTATE_OBJECT => {
                self.collect(s, 4);
                self.script_rotate_object(s);
            }
            c::SLIDE_OBJECT => {
                self.collect(s, 8);
                self.script_slide_object(s);
            }
            c::WARP_CHAR_FROM_CAR_TO_COORD => {
                self.collect(s, 4);
                let ped = self.script_ped(self.sp(0)).expect("WARP_CHAR_FROM_CAR_TO_COORD: no such ped");
                self.warp_ped_from_car_to_coord(px, ped, self.spv(1), true);
            }
            c::WARP_PLAYER_INTO_CAR | c::WARP_CHAR_INTO_CAR => {
                self.collect(s, 2);
                let ped = if command == c::WARP_PLAYER_INTO_CAR { self.script_player() } else { self.script_ped(self.sp(0)).expect("WARP_CHAR_INTO_CAR: no such ped") };
                let car = self.script_vehicle(self.sp(1)).expect("WARP_x_INTO_CAR: no such car");
                self.set_objective_entity(px, ped, crate::ped::data::objective::ENTER_CAR_AS_DRIVER, car);
                self.warp_ped_into_car(px, ped, car);
            }
            c::CREATE_RANDOM_CHAR => {
                self.collect(s, 3);
                self.script_create_random_char(px, sh, s);
            }
            c::SET_VISIBILITY_OF_CLOSEST_OBJECT_OF_TYPE => {
                self.collect(s, 6);
                let mut pos = self.spv(0);
                if pos.z <= MAP_Z_LOW_LIMIT {
                    pos.z = self.find_ground_z_for_coord(pos.x, pos.y);
                }
                let wanted = self.sp(4);
                let model = if wanted < 0 { self.ents.scripts.used_objects.get((-wanted) as usize).map_or(-1, |u| u.1) } else { wanted } as i16;
                let (range, visible) = (self.spf(3), self.sp(5) != 0);
                let kinds = crate::world::Kinds { buildings: true, objects: true, dummies: true, ..crate::world::Kinds::NONE };
                let found = self.find_objects_of_type_in_range(model, pos, range, true, 16, kinds);
                let closest = found.into_iter().map(|r| ((self.epos(r) - pos).length(), r)).filter(|&(d, _)| d < 2.0 * range).min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, r)) = closest {
                    self.ent_mut(r).base.flags.is_visible = visible;
                    self.ents.scripts.add_to_invisibility_swap_array(r, visible);
                }
            }
            c::PLACE_OBJECT_RELATIVE_TO_CAR => {
                self.collect(s, 5);
                let obj = self.script_object(self.sp(0)).expect("PLACE_OBJECT_RELATIVE_TO_CAR: no such object");
                let car = self.script_vehicle(self.sp(1)).expect("PLACE_OBJECT_RELATIVE_TO_CAR: no such car");
                self.place_physical_relative_to_other(car, obj, self.spv(2));
            }
            c::MAKE_OBJECT_TARGETTABLE => {
                self.collect(s, 1);
                let obj = self.script_object(self.sp(0)).expect("MAKE_OBJECT_TARGETTABLE: no such object");
                self.make_object_targettable(self.script_player(), obj);
            }
            c::SET_CHAR_WAIT_STATE => {
                self.collect(s, 3);
                let ped = self.script_ped(self.sp(0)).expect("SET_CHAR_WAIT_STATE: no such ped");
                let time = (self.sp(2) >= 0).then(|| self.sp(2));
                self.set_wait_state(px, ped, self.sp(1) as u8, time);
            }
            c::PRINT_STRING_IN_STRING => {
                let (text, inner) = (self.ents.scripts.read_text_label(&mut s.ip), self.ents.scripts.read_text_label(&mut s.ip));
                self.collect(s, 2);
                sh.print_string_in_string(0, &text, self.sp(0) as u32, self.sp(1), &inner);
            }
            c::IS_SNIPER_BULLET_IN_AREA => {
                self.collect(s, 6);
                let (a, b) = (self.spv(0), self.spv(3));
                let (lo, hi) = (a.min(b), a.max(b));
                s.update_compare_flag(sh.test_for_sniper_bullet(lo.x, hi.x, lo.y, hi.y, lo.z, hi.z));
            }
            c::GIVE_PLAYER_DETONATOR => self.give_player_detonator(),
            c::SET_OBJECT_VELOCITY => {
                self.collect(s, 4);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_VELOCITY: no such object");
                let speed = self.spv(1) * (1.0 / 50.0);
                if let Some(p) = self.ent_mut(obj).phys.as_mut() {
                    p.move_speed = speed;
                }
            }
            c::SET_OBJECT_COLLISION => {
                self.collect(s, 2);
                let obj = self.script_object(self.sp(0)).expect("SET_OBJECT_COLLISION: no such object");
                let on = self.sp(1) != 0;
                self.ent_mut(obj).base.flags.uses_collision = on;
            }
            c::IS_ICECREAM_JINGLE_ON => {
                self.collect(s, 1);
                let car = self.script_vehicle(self.sp(0)).expect("IS_ICECREAM_JINGLE_ON: no such car");
                s.update_compare_flag(self.try_veh(car).unwrap().siren_or_alarm);
            }
            c::IS_EXPLOSION_IN_AREA => {
                self.collect(s, 7);
                let (a, b) = (self.spv(1), self.spv(4));
                s.update_compare_flag(self.explosion_in_area(self.sp(0), a.min(b), a.max(b)));
            }
            c::IS_EXPLOSION_IN_ZONE => {
                self.collect(s, 1);
                let label = self.ents.scripts.read_text_label(&mut s.ip);
                let zone = self.ents.zones.find_zone_by_label_and_return_index(&label).expect("IS_EXPLOSION_IN_ZONE: no such zone");
                let z = &self.ents.zones.zone_array[zone];
                let (min, max) = (z.min, z.max);
                s.update_compare_flag(self.explosion_in_area(self.sp(0), min, max));
            }
            _ => panic!("script command {command} is not ported (script {}, at {})", s.name_str(), s.ip),
        }
        0
    }

    /// `COMMAND_REMOVE_CHAR_ELEGANTLY`: a mission ped leaves its car or fades out.
    fn script_remove_char_elegantly(&mut self, px: &mut Px, handle: i32) {
        let Some(ped) = self.script_ped(handle) else { return };
        if self.ped(ped).char_created_by != crate::ped::data::MISSION_CHAR {
            return;
        }
        self.remove_references_to(ped);
        if self.ped(ped).in_vehicle {
            if let Some(car) = self.ped(ped).my_vehicle {
                if self.try_veh(car).is_some_and(|v| v.driver == Some(ped)) {
                    let status = self.ent(car).base.status;
                    let v = self.try_veh_mut(car).unwrap();
                    let new = v.remove_driver(status);
                    if v.door_lock == crate::vehicle::base::CarLock::LockedInitially {
                        v.door_lock = crate::vehicle::base::CarLock::Unlocked;
                    }
                    let law = v.flags.is_law_enforcer;
                    self.get_mut(car).unwrap().base.status = new;
                    if self.ped(ped).ped_type == crate::ped::types::ped_type::COP && law {
                        self.change_law_enforcer_state(car, false);
                    }
                } else if let Some(v) = self.try_veh_mut(car) {
                    v.remove_passenger(ped);
                }
            }
            self.destroy_ped(px, ped);
        } else {
            let pd = self.ped_mut(ped);
            pd.char_created_by = crate::ped::data::RANDOM_CHAR;
            pd.flags.responds_to_threats = true;
            pd.flags.script_objective_completed = false;
            self.clear_leader(px, ped);
            self.ped_mut(ped).flags.fade_out = true;
        }
        self.ents.population.total_mission_peds = self.ents.population.total_mission_peds.saturating_sub(1);
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
        zoom: Vec<i32>,
        big: Vec<(String, Vec<i32>)>,
        sniper: Vec<[f32; 6]>,
        strings: Vec<(String, String)>,
    }
    impl ScriptHooks for Spy {
        fn test_for_sniper_bullet(&mut self, x1: f32, x2: f32, y1: f32, y2: f32, z1: f32, z2: f32) -> bool {
            self.sniper.push([x1, x2, y1, y2, z1, z2]);
            true
        }
        fn print_string_in_string(&mut self, _kind: u8, key: &str, _time: u32, _flags: i32, inner_key: &str) {
            self.strings.push((key.to_owned(), inner_key.to_owned()));
        }
        fn set_camera_zoom(&mut self, z: i32) {
            self.zoom.push(z);
        }
        fn print_big_with_numbers(&mut self, key: &str, _time: u32, _style: i32, numbers: &[i32]) {
            self.big.push((key.to_owned(), numbers.to_vec()));
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
    fn random_cars_of_a_model_in_an_area_become_mission_cars() {
        let mut a = Asm::new(4);
        a.cmd(c::GET_RANDOM_CAR_OF_TYPE_IN_AREA, &[A::F(-10.0), A::F(-10.0), A::F(10.0), A::F(10.0), A::I16(LANDSTAL_MODEL), A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let mut rng = Rng::default();
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap();
        let far = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, &mut rng).unwrap();
        w.get_mut(far).unwrap().base.set_position(Vec3::new(100.0, 0.0, 0.0));
        w.ents.car_ctrl.num_random_cars = 2;
        let i = start(&mut w);
        w.ents.scripts.scripts[i].is_mission_script = true;
        run_frame(&mut w, &mut Spy::default());
        assert_eq!(g(&w, 8), car.handle);
        assert_eq!((w.try_veh(car).unwrap().created_by, w.try_veh(far).unwrap().created_by), (MISSION_VEHICLE, RANDOM_VEHICLE));
        assert_eq!((w.ents.car_ctrl.num_mission_cars, w.ents.car_ctrl.num_random_cars, w.ents.scripts.cleanup.count), (1, 1, 1));
    }

    #[test]
    fn on_screen_text_lines_are_set_up_and_shown() {
        let mut a = Asm::new(2);
        a.cmd(c::SET_TEXT_SCALE, &[A::F(0.5), A::F(1.0)]);
        a.cmd(c::SET_TEXT_COLOUR, &[A::I16(10), A::I16(20), A::I16(30), A::I16(255)]);
        a.cmd(c::SET_TEXT_CENTRE, &[A::I8(1)]);
        a.bytes.extend_from_slice(&c::DISPLAY_TEXT.to_le_bytes());
        a.bytes.extend_from_slice(&[6, 0x00, 0x08, 6, 0x00, 0x10]);
        a.bytes.extend_from_slice(b"HELLO\0\0\0");
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let sc = &w.ents.scripts;
        let l = &sc.intro_text_lines[0];
        assert_eq!((sc.number_of_intro_text_lines_this_frame, l.text.as_str(), l.at_x, l.at_y, l.scale_x, l.colour, l.centered), (1, "HELLO", 128.0, 256.0, 0.5, [10, 20, 30, 255], true));
    }

    #[test]
    fn armour_is_clamped_zoom_goes_to_the_camera_and_garages_open() {
        let mut a = Asm::new(2);
        a.cmd(c::ADD_ARMOUR_TO_PLAYER, &[A::I8(0), A::I8(70)]);
        a.cmd(c::ADD_ARMOUR_TO_PLAYER, &[A::I8(0), A::I8(70)]);
        a.cmd(c::SET_CAMERA_ZOOM, &[A::I8(2)]);
        a.cmd(c::OPEN_GARAGE, &[A::I8(0)]);
        a.bytes.extend_from_slice(&c::PRINT_WITH_2_NUMBERS_BIG.to_le_bytes());
        a.bytes.extend_from_slice(b"BIG\0\0\0\0\0");
        a.bytes.extend_from_slice(&[4, 5, 4, 6, 5, 0xB8, 0x0B, 4, 2]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        w.ents.garages.garages[0].ty = crate::control::garages::garage_type::MISSION;
        w.ents.garages.garages[0].state = crate::control::garages::garage_state::FULLYCLOSED;
        start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        let p = w.ents.player_ped.unwrap();
        assert_eq!((w.ped(p).armour, w.ents.garages.garages[0].state), (100.0, crate::control::garages::garage_state::OPENING));
        assert_eq!((spy.zoom, spy.big), (vec![2], vec![("BIG".to_owned(), vec![5, 6])]));
    }

    #[test]
    fn a_mission_ped_removed_elegantly_fades_out_and_stops_being_a_mission_ped() {
        let mut a = Asm::new(2);
        a.cmd(c::REMOVE_CHAR_ELEGANTLY, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let ped = w.create_ped(&mut Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks }, ped_type::CIVMALE, -1, None).unwrap();
        w.ped_mut(ped).char_created_by = crate::ped::data::MISSION_CHAR;
        w.ents.population.total_mission_peds = 1;
        w.ents.scripts.set_global(8, ped.handle);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert_eq!((w.ped(ped).char_created_by, w.ped(ped).flags.fade_out, w.ents.population.total_mission_peds), (crate::ped::data::RANDOM_CHAR, true, 0));
    }

    #[test]
    fn sniper_bullets_in_a_box_given_in_any_order_and_strings_inside_strings() {
        let mut a = Asm::new(0);
        a.cmd(c::IS_SNIPER_BULLET_IN_AREA, &[A::F(10.0), A::F(-3.0), A::F(5.0), A::F(-10.0), A::F(3.0), A::F(-5.0)]);
        a.bytes.extend_from_slice(&c::PRINT_STRING_IN_STRING.to_le_bytes());
        a.bytes.extend_from_slice(b"OUTER\0\0\0");
        a.bytes.extend_from_slice(b"INNER\0\0\0");
        a.bytes.extend_from_slice(&[4, 5, 4, 1]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let i = start(&mut w);
        let mut spy = Spy::default();
        run_frame(&mut w, &mut spy);
        assert!(w.ents.scripts.scripts[i].cond_result);
        assert_eq!(spy.sniper, vec![[-10.0, 10.0, -3.0, 3.0, -5.0, 5.0]]);
        assert_eq!(spy.strings, vec![("OUTER".to_owned(), "INNER".to_owned())]);
    }

    #[test]
    fn a_heavy_car_and_the_ice_cream_jingle() {
        let mut a = Asm::new(4);
        a.cmd(c::CREATE_CAR, &[A::I16(LANDSTAL_MODEL), A::F(10.0), A::F(10.0), A::F(0.0), A::G(8)]);
        a.cmd(c::SET_CAR_HEAVY, &[A::G(8), A::I8(1)]);
        a.cmd(c::SWITCH_CAR_SIREN, &[A::G(8), A::I8(1)]);
        a.cmd(c::IS_ICECREAM_JINGLE_ON, &[A::G(8)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let car = w.script_vehicle(g(&w, 8)).unwrap();
        assert!(w.ent(car).phys.as_ref().unwrap().flags.is_heavy);
        assert!(w.ents.scripts.scripts[i].cond_result);
    }

    #[test]
    fn a_ped_warped_out_of_its_car_stands_at_the_point_and_the_car_is_left_abandoned() {
        let mut a = Asm::new(4);
        a.cmd(c::WARP_CHAR_FROM_CAR_TO_COORD, &[A::G(8), A::F(30.0), A::F(40.0), A::F(5.0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let mut px = Px { rng: &mut Rng::default(), hooks: &mut NoPedHooks };
        let car = w.new_car(LANDSTAL_MODEL, RANDOM_VEHICLE, px.rng).unwrap();
        let ped = w.create_ped(&mut px, ped_type::CIVMALE, -1, None).unwrap();
        w.try_veh_mut(car).unwrap().driver = Some(ped);
        let pd = w.ped_mut(ped);
        (pd.in_vehicle, pd.my_vehicle) = (true, Some(car));
        w.ents.scripts.set_global(8, ped.handle);
        start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        let (pd, v) = (w.ped(ped), w.try_veh(car).unwrap());
        assert_eq!((pd.in_vehicle, pd.my_vehicle, v.driver, w.ent(car).base.status), (false, None, None, crate::entity::status::ABANDONED));
        let at = w.epos(ped);
        assert!((at.x - 30.0).abs() < 1e-3 && (at.y - 40.0).abs() < 1e-3, "{at}");
    }

    #[test]
    fn hidden_things_are_remembered_and_shown_again() {
        let mut s = super::super::Scripts::default();
        let r = EntityRef { pool: crate::entity::PoolKind::Object, handle: 3 };
        s.add_to_invisibility_swap_array(r, false);
        s.add_to_invisibility_swap_array(r, false);
        assert_eq!(s.invisibility_settings.iter().flatten().count(), 1);
        s.add_to_invisibility_swap_array(r, true);
        assert_eq!(s.invisibility_settings.iter().flatten().count(), 0);
    }

    #[test]
    fn an_object_slides_and_turns_in_steps_and_reports_when_it_is_there() {
        use crate::math::Matrix;
        let mut a = Asm::new(4);
        a.cmd(c::SLIDE_OBJECT, &[A::G(8), A::F(30.0), A::F(20.0), A::F(0.0), A::F(1.0), A::F(1.0), A::F(1.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let obj = w.new_object(crate::world::tests::BOX_MODEL, Matrix::from_translation(Vec3::new(20.0, 20.0, 0.0)), &mut Rng::default()).unwrap();
        w.add(obj).unwrap();
        w.ents.scripts.set_global(8, obj.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        // one step along x only; the others are already where they should be
        let at = w.epos(obj);
        let step = w.ents.timer.time_step_fix();
        assert!((at.x - (20.0 + step)).abs() < 1e-4 && at.y == 20.0, "{at}");
        assert!(!w.ents.scripts.scripts[i].cond_result);

        let mut a = Asm::new(4);
        a.cmd(c::ROTATE_OBJECT, &[A::G(8), A::F(90.0), A::F(1000.0), A::I8(0)]);
        a.cmd(c::WAIT, &[A::I32(100000)]);
        let mut w = world(&a);
        let obj = w.new_object(crate::world::tests::BOX_MODEL, Matrix::from_translation(Vec3::new(20.0, 20.0, 0.0)), &mut Rng::default()).unwrap();
        w.add(obj).unwrap();
        w.ents.scripts.set_global(8, obj.handle);
        let i = start(&mut w);
        run_frame(&mut w, &mut Spy::default());
        assert!(w.ents.scripts.scripts[i].cond_result);
        assert!((w.ent(obj).base.matrix.forward.x + 1.0).abs() < 1e-4);
    }
}
