//! `CopPed.cpp`: street cops, FBI, SWAT and army chasing, shooting and arresting the player.
use super::ctx::Px;
use super::data::*;
use super::types::ped_type;
use crate::anim::{assoc_group, tables_gta3::anim};
use crate::audio::sound;
use crate::entity::{status, EntityRef};
use crate::general::radian_angle_between_points;
use crate::modelinfo::indices_gta3::mi;
use crate::vehicle::base::{car_mission, driving_style};
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, LineOpts, World};
use glam::{Vec2, Vec3};

/// `eCopType`.
pub mod cop_type {
    pub const STREET: u8 = 0;
    pub const FBI: u8 = 1;
    pub const SWAT: u8 = 2;
    pub const ARMY: u8 = 3;
}

/// The fields `CCopPed` adds to `CPed`.
#[derive(Debug, Clone, PartialEq)]
pub struct CopData {
    pub cop_type: u8,
    /// `m_nRoadblockNode`: -1 for none.
    pub roadblock_node: i16,
    pub distance_to_target: f32,
    pub is_in_pursuit: bool,
    pub is_disabled_cop: bool,
    pub beating_suspect: bool,
    pub stop_and_shoot_disabled_zone: bool,
    pub zone_disabled: bool,
}

impl Default for CopData {
    fn default() -> Self {
        Self {
            cop_type: cop_type::STREET,
            roadblock_node: -1,
            distance_to_target: 0.0,
            is_in_pursuit: false,
            is_disabled_cop: false,
            beating_suspect: false,
            stop_and_shoot_disabled_zone: false,
            zone_disabled: false,
        }
    }
}

impl World {
    fn cop(&self, r: EntityRef) -> &CopData {
        self.ped(r).cop.as_deref().expect("cop data")
    }

    fn cop_mut(&mut self, r: EntityRef) -> &mut CopData {
        self.ped_mut(r).cop.get_or_insert_with(Default::default)
    }

    /// `new CCopPed(copType)` with the model's skeleton at hand.
    pub fn create_cop(&mut self, px: &mut Px, kind: u8, clump: Option<super::clump::PedClump>) -> Option<EntityRef> {
        let model = match kind {
            cop_type::FBI => mi::FBI,
            cop_type::SWAT => mi::SWAT,
            cop_type::ARMY => mi::ARMY,
            _ => mi::COP,
        };
        let r = self.create_ped(px, ped_type::COP, model, clump)?;
        // (weapons, armour, skill, accuracy) by type
        match kind {
            cop_type::STREET => {
                self.give_weapon(r, wt::COLT45, 1000);
                self.ped_mut(r).current_weapon = wt::UNARMED as u8;
                self.set_cop_stats(r, 0.0, 208, 60);
            }
            cop_type::FBI => {
                self.give_weapon(r, wt::COLT45, 1000);
                self.give_weapon(r, wt::AK47, 1000);
                self.set_current_weapon(r, wt::AK47);
                self.set_cop_stats(r, 100.0, 176, 76);
            }
            cop_type::SWAT => {
                self.give_weapon(r, wt::COLT45, 1000);
                self.give_weapon(r, wt::UZI, 1000);
                self.set_current_weapon(r, wt::UZI);
                self.set_cop_stats(r, 50.0, 32, 68);
            }
            _ => {
                self.give_weapon(r, wt::COLT45, 1000);
                self.give_weapon(r, wt::M16, 1000);
                self.give_weapon(r, wt::GRENADE, 10);
                self.set_current_weapon(r, wt::M16);
                self.set_cop_stats(r, 100.0, 32, 84);
            }
        }
        let pd = self.ped_mut(r);
        pd.attack_timer = 0;
        pd.point_gun_at = None;
        pd.class = PedClass::Cop;
        pd.cop = Some(Box::new(CopData { cop_type: kind, ..Default::default() }));
        Some(r)
    }

    fn set_cop_stats(&mut self, r: EntityRef, armour: f32, skills: u8, accuracy: u8) {
        let pd = self.ped_mut(r);
        pd.armour = armour;
        pd.wep_skills = skills;
        pd.wep_accuracy = accuracy;
    }

    /// `CCopPed::SetArrestPlayer`.
    pub fn set_arrest_player(&mut self, px: &mut Px, r: EntityRef, player: EntityRef) {
        if !self.ped(r).is_ped_in_control() || self.try_ped(player).is_none() {
            return;
        }
        let said = match self.cop(r).cop_type {
            cop_type::FBI => sound::PED_ARREST_FBI,
            cop_type::SWAT => sound::PED_ARREST_SWAT,
            _ => sound::PED_ARREST_COP,
        };
        self.say(px, r, said);
        let now = self.now();
        if self.ped(player).entering_car() {
            if now > self.ped(r).ped_state_timer {
                return;
            }
            self.ped_mut(player).flags.gonna_kill_the_car_jacker = true;
            self.player_arrested_by(player, r);
        } else if !matches!(self.ped(player).ped_state, state::DIE | state::DEAD | state::ARRESTED) {
            let pd = self.ped_mut(player);
            pd.last_ped_state = pd.ped_state;
            pd.ped_state = state::ARRESTED;
            self.player_arrested_by(player, r);
        }
        let pd = self.ped_mut(r);
        pd.ped_state = state::ARREST_PLAYER;
        pd.prev_objective = objective::NONE;
        pd.flags.is_pointing_gun_at = false;
        pd.seek_target = Some(player);
        self.set_objective(px, r, objective::NONE);
        self.ped_mut(r).prev_objective = objective::NONE;
        self.set_current_weapon(r, wt::COLT45);
        if self.ped(player).in_vehicle
            && let Some(v) = self.ped(player).my_vehicle
        {
            if let Some(vd) = self.try_veh_mut(v) {
                vd.num_getting_in = 0;
                vd.getting_in_flags = 0;
                vd.flags.is_handbrake_on = true;
            }
            self.get_mut(v).expect("vehicle").base.status = status::PLAYER_DISABLED;
        }
        let pd = self.ped(r);
        if pd.weapons[pd.current_weapon as usize].ty == wt::UNARMED {
            self.set_current_weapon(r, wt::COLT45);
        }
    }

    /// The arrest bookkeeping on the player: they can't be hurt and know who is arresting them.
    fn player_arrested_by(&mut self, player: EntityRef, cop: EntityRef) {
        if let Some(p) = self.ents.player_ped.and_then(|p| self.ped_mut_opt(p)).and_then(|p| p.player.as_mut()) {
            p.can_be_damaged = false;
        }
        if let Some(p) = self.ped_mut(player).player.as_mut() {
            p.arresting_cop = Some(cop);
        }
    }

    fn ped_mut_opt(&mut self, r: EntityRef) -> Option<&mut PedData> {
        match &mut self.get_mut(r)?.kind {
            crate::entity::Kind::Ped(p) => Some(p),
            _ => None,
        }
    }

    /// `CCopPed::ClearPursuit`.
    pub fn clear_pursuit(&mut self, px: &mut Px, r: EntityRef) {
        let Some(player) = self.ents.player_ped else { return };
        if !self.cop(r).is_in_pursuit {
            return;
        }
        self.cop_mut(r).is_in_pursuit = false;
        if let Some(w) = self.player_wanted_mut() {
            let (mut ours, mut biggest) = (None, 0);
            for i in 0..w.max_cops.max(w.current_cops) as usize {
                if ours.is_none() && w.cops[i] == Some(r) {
                    w.cops[i] = None;
                    w.current_cops = w.current_cops.saturating_sub(1);
                    ours = Some(i);
                    biggest = i;
                } else if w.cops[i].is_some() {
                    biggest = i;
                }
            }
            if let Some(i) = ours.filter(|&i| biggest > i) {
                w.cops[i] = w.cops[biggest];
                w.cops[biggest] = None;
            }
        }
        let pd = self.ped_mut(r);
        pd.objective = objective::NONE;
        pd.prev_objective = objective::NONE;
        pd.last_ped_state = state::NONE;
        pd.flags.is_running = false;
        pd.flags.not_allowed_to_duck = false;
        pd.flags.kinda_stay_in_same_place = false;
        let cop = self.cop_mut(r);
        cop.stop_and_shoot_disabled_zone = false;
        cop.zone_disabled = false;
        self.clear_objective(px, r);
        if !self.ped(r).is_ped_in_control() {
            return;
        }
        let level = self.wanted_info().level;
        let veh = self.ped(r).my_vehicle;
        match veh {
            Some(v) if level == 0 => self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, v),
            Some(v) if (self.epos(v) - self.epos(r)).length_squared() < 25.0 => {
                self.ped_mut(r).last_ped_state = state::IDLE;
                self.set_seek_entity(r, Some(v), 2.5);
            }
            _ => {
                self.ped_mut(r).last_ped_state = state::WANDER_PATH;
                let p = self.epos(player);
                self.set_find_path_and_flee_pos(r, Vec2::new(p.x, p.y), 10000, true);
            }
        }
    }

    /// `CCopPed::SetPursuit`.
    pub fn set_pursuit(&mut self, px: &mut Px, r: EntityRef, ignore_cop_limit: bool) {
        if self.cop(r).is_in_pursuit || !self.ped(r).is_ped_in_control() {
            return;
        }
        let Some(player) = self.ents.player_ped else { return };
        let mut joined = false;
        if let Some(w) = self.player_wanted_mut()
            && (w.current_cops < w.max_cops || ignore_cop_limit)
            && let Some(slot) = w.cops[..w.max_cops as usize].iter().position(Option::is_none)
        {
            w.cops[slot] = Some(r);
            w.current_cops += 1;
            joined = true;
        }
        if !joined {
            return;
        }
        self.cop_mut(r).is_in_pursuit = true;
        self.clear_objective(px, r);
        self.ped_mut(r).prev_objective = objective::NONE;
        self.set_objective_entity(px, r, objective::KILL_CHAR_ON_FOOT, player);
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.set_objective_timer(0, now);
        pd.flags.not_allowed_to_duck = true;
        pd.flags.is_running = true;
        self.cop_mut(r).stop_and_shoot_disabled_zone = false;
    }

    /// `CCopPed::ArrestPlayer`.
    pub fn arrest_player(&mut self, px: &mut Px, r: EntityRef) {
        self.ped_mut(r).vehicle_anim = None;
        let Some(suspect) = self.ped(r).seek_target.filter(|&s| self.is_ped(s)) else {
            self.clear_pursuit(px, r);
            return;
        };
        if self.ped(suspect).can_set_ped_state() {
            self.ped_mut(suspect).ped_state = state::ARRESTED;
        }
        let (sp, mine) = (self.ped(suspect), self.ped(r));
        if sp.in_vehicle && mine.my_vehicle.is_some() && sp.my_vehicle == mine.my_vehicle {
            self.line_up_ped_with_car(r, line_up::TO_CAR_2);
        }
        let sp = self.ped(suspect);
        if !(sp.ped_state == state::ARRESTED || sp.dying_or_dead() || sp.entering_car()) {
            self.clear_pursuit(px, r);
            return;
        }
        if self.ped_assoc(r, anim::STD_ARREST).is_none_or(|a| a.blend_delta < 0.0) {
            self.ped_blend_anim(r, assoc_group::STD, anim::STD_ARREST, 4.0);
        }
        let mid = self.ped_node_position(suspect, node::MID);
        let pos = self.epos(r);
        let dest = radian_angle_between_points(mid.x, mid.y, pos.x, pos.y);
        let pd = self.ped_mut(r);
        pd.rotation_dest = dest;
        pd.rotation_cur = dest;
        self.get_mut(r).expect("cop").base.set_orientation(0.0, 0.0, dest);
    }

    /// `FindPlayerVehicle()`: the car the player is in.
    pub fn player_vehicle(&self) -> Option<EntityRef> {
        let p = self.try_ped(self.ents.player_ped?)?;
        p.my_vehicle.filter(|_| p.in_vehicle)
    }

    /// `CCopPed::ScanForCrimes`.
    pub fn scan_for_crimes(&mut self, r: EntityRef) {
        let now = self.now();
        let Some(player) = self.ents.player_ped else { return };
        if let Some(v) = self.player_vehicle()
            && let Some(vd) = self.try_veh(v)
            && vd.is_car()
            && vd.is_alarm_on(self.ent(v).base.status)
            && (self.epos(v) - self.epos(r)).length_squared() < 400.0
            && let Some(w) = self.player_wanted_mut()
        {
            w.set_wanted_level_no_drop(1, now);
        }
        if !self.cop(r).is_in_pursuit {
            let pd = self.ped(r);
            let entering = matches!(pd.objective, objective::ENTER_CAR_AS_DRIVER | objective::ENTER_CAR_AS_PASSENGER);
            let pv = self.ped(player).my_vehicle;
            // FIX_BUGS: only the car this cop is getting into counts
            if entering && self.wanted_info().level == 0 && pv.is_some() && pd.my_vehicle == pv && pv.and_then(|v| self.try_veh(v)).is_some_and(|v| v.flags.is_law_enforcer) {
                self.set_wanted_level_no_drop(1);
            }
        }
    }

    /// `FindPlayerPed()` or the car they drive.
    fn player_or_vehicle(&self) -> Option<EntityRef> {
        self.player_vehicle().or(self.ents.player_ped)
    }

    fn clear_cop_stand(&mut self, r: EntityRef, crouch_too: bool) {
        let pd = self.ped_mut(r);
        pd.flags.kinda_stay_in_same_place = false;
        pd.flags.not_allowed_to_duck = false;
        pd.flags.duck_and_cover = false;
        if crouch_too {
            pd.flags.crouch_when_shooting = false;
            pd.flags.is_ducking = false;
        }
        self.cop_mut(r).is_disabled_cop = false;
    }

    /// `CCopPed::CopAI`.
    pub fn cop_ai(&mut self, px: &mut Px, r: EntityRef) {
        let Some(player) = self.ents.player_ped else { return };
        let Some(target) = self.player_or_vehicle() else { return };
        let now = self.now();
        let Some(w) = self.player_wanted().cloned() else { return };
        let level = w.level;
        if w.is_ignored() {
            if self.ped(r).ped_state != state::ARREST_PLAYER {
                self.clear_pursuit(px, r);
            }
            return;
        }
        let no_police = px.hooks.no_police_zone();
        if no_police && self.cop(r).is_in_pursuit && !self.cop(r).is_disabled_cop {
            if self.ped(r).flags.hit_something_last_frame {
                let far = self.cop(r).distance_to_target > 15.0;
                let cop = self.cop_mut(r);
                cop.zone_disabled = true;
                cop.is_disabled_cop = true;
                cop.roadblock_node = -1;
                let pd = self.ped_mut(r);
                pd.flags.kinda_stay_in_same_place = true;
                pd.flags.is_running = false;
                pd.flags.not_allowed_to_duck = false;
                pd.flags.crouch_when_shooting = false;
                self.set_idle(px, r);
                self.clear_objective(px, r);
                self.clear_pursuit(px, r);
                let pd = self.ped_mut(r);
                pd.prev_objective = objective::NONE;
                pd.last_ped_state = state::NONE;
                self.set_attack_timer(r, 0);
                // a safe distance for a disabled zone, or just to make the game easier
                if far {
                    self.cop_mut(r).stop_and_shoot_disabled_zone = true;
                }
            }
        } else if self.cop(r).zone_disabled && !no_police {
            self.cop_mut(r).zone_disabled = false;
            self.clear_cop_stand(r, true);
            self.cop_mut(r).stop_and_shoot_disabled_zone = false;
            self.clear_pursuit(px, r);
        }
        if level > 0 {
            if !self.cop(r).is_disabled_cop {
                self.cop_chase(px, r, player, target, level);
                return;
            }
            self.cop_shoot_from_cover(px, r, target);
        } else if !self.cop(r).is_disabled_cop || self.cop(r).zone_disabled {
            if self.ped(r).ped_state != state::AIM_GUN {
                if self.cop(r).is_in_pursuit {
                    self.clear_pursuit(px, r);
                }
                if self.ped(r).is_ped_in_control() {
                    self.cop_get_in_car(px, r);
                }
            }
        } else {
            if self.cop(r).is_in_pursuit && self.ped(r).ped_state != state::AIM_GUN {
                self.clear_pursuit(px, r);
            }
            self.clear_cop_stand(r, true);
            if let Some(v) = self.ped(r).my_vehicle {
                self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, v);
            }
        }
        let _ = now;
    }

    /// Wanted level above zero and not disabled: join or leave the pursuit, then keep to the player.
    fn cop_chase(&mut self, px: &mut Px, r: EntityRef, player: EntityRef, target: EntityRef, level: i32) {
        let dist = self.cop(r).distance_to_target;
        let in_pursuit = self.cop(r).is_in_pursuit;
        let w = self.player_wanted().cloned().expect("wanted");
        if !in_pursuit || w.current_cops > w.max_cops {
            let (mut farthest, mut farthest_dist) = (None, dist);
            for i in 0..w.max_cops.max(w.current_cops) as usize {
                if let Some(cop) = w.cops[i].filter(|&c| self.try_ped(c).is_some_and(|d| d.cop.is_some())) {
                    let d = self.cop(cop).distance_to_target;
                    if d > farthest_dist {
                        farthest_dist = d;
                        farthest = Some(cop);
                    }
                }
            }
            if in_pursuit {
                if farthest.is_some() && w.current_cops > w.max_cops {
                    if farthest == Some(r) && dist > 10.0 {
                        self.clear_pursuit(px, r);
                    } else if farthest_dist > 10.0
                        && let Some(f) = farthest
                    {
                        self.clear_pursuit(px, f);
                    }
                }
            } else if w.current_cops < w.max_cops {
                self.set_pursuit(px, r, true);
            } else if dist <= 10.0 || farthest.is_some() && dist < farthest_dist {
                if let Some(f) = farthest.filter(|_| farthest_dist > 10.0) {
                    self.clear_pursuit(px, f);
                }
                self.set_pursuit(px, r, true);
            }
        } else {
            self.set_pursuit(px, r, false);
        }
        if !self.cop(r).is_in_pursuit {
            return;
        }
        let weapon = {
            let pd = self.ped(r);
            pd.weapons[pd.current_weapon as usize].ty
        };
        let on_a_car = self.ped(player).current_phys_surface.is_some();
        if level > 1 && weapon == wt::UNARMED {
            self.set_current_weapon(r, wt::COLT45);
        } else if level == 1 && weapon != wt::UNARMED && !on_a_car {
            // a player on top of a car still gets the pistol
            self.set_current_weapon(r, wt::UNARMED);
        }
        if let Some(v) = self.player_vehicle() {
            if self.cop(r).beating_suspect {
                if let Some(w) = self.player_wanted_mut() {
                    w.cops_beating_suspect = w.cops_beating_suspect.saturating_sub(1);
                }
                self.cop_mut(r).beating_suspect = false;
            }
            let speed = self.ent(v).phys.as_ref().map_or(0.0, |p| p.move_speed.length());
            if dist * speed > 4.0 {
                self.clear_pursuit(px, r);
            }
        }
        let _ = target;
    }

    /// A disabled cop (roadblock, no-police zone) stands and shoots.
    fn cop_shoot_from_cover(&mut self, px: &mut Px, r: EntityRef, target: EntityRef) {
        let now = self.now();
        let pd = self.ped(r);
        let range = self.ents.weapon_infos.get(pd.weapons[pd.current_weapon as usize].ty).range;
        self.set_look_flag_entity(r, target, true);
        self.turn_body(r);
        self.set_current_weapon(r, wt::COLT45);
        if self.ped(r).flags.is_ducking {
            return;
        }
        let dist = self.cop(r).distance_to_target;
        let target_pos = self.epos(target);
        if self.ped(r).attack_timer >= now {
            let pd = self.ped(r);
            if matches!(pd.ped_state, state::ATTACK | state::FIGHT) || self.cop(r).zone_disabled {
                return;
            }
            let target_speed = self.ent(target).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
            if dist > 30.0 {
                if let Some(a) = self.ped_assoc_mut(r, anim::STD_RBLOCK_SHOOT) {
                    a.blend_delta = -1000.0;
                }
                // the target is coming onto us
                if target_speed.dot(target_pos - self.epos(r)) > 0.0 {
                    self.clear_cop_stand(r, false);
                    self.set_pursuit(px, r, false);
                    if let Some(p) = self.ents.player_ped {
                        self.set_objective_entity(px, r, objective::KILL_CHAR_ANY_MEANS, p);
                    }
                }
            } else if dist < 5.0 && self.player_vehicle().is_none_or(|v| self.ent(v).phys.as_ref().is_none_or(|p| p.move_speed.length_squared() < 1.0 / 40000.0)) {
                self.clear_cop_stand(r, false);
            } else {
                // FIX_BUGS: no roadblock means both sides count as in front
                let (mut cop_dot, mut target_dot) = (1.0, 1.0);
                let node = self.cop(r).roadblock_node;
                if node != -1
                    && let Some((road_pos, fwd)) = px.hooks.roadblock_road(node)
                {
                    let (me, you) = (self.epos(r), target_pos);
                    cop_dot = Vec2::new(me.x - road_pos.x, me.y - road_pos.y).dot(fwd);
                    target_dot = Vec2::new(you.x - road_pos.x, you.y - road_pos.y).dot(fwd);
                }
                // the roadblock may face either way along the road, so both signs are checked
                if (cop_dot >= 0.0 || target_dot >= 0.0) && (cop_dot <= 0.0 || target_dot <= 0.0) {
                    self.ped_mut(r).flags.is_pointing_gun_at = true;
                } else {
                    self.clear_cop_stand(r, true);
                    self.set_pursuit(px, r, false);
                }
            }
        } else {
            if dist < range {
                let fire_offset = self.ents.weapon_infos.get(self.ped(r).weapons[self.ped(r).current_weapon as usize].ty).fire_offset;
                let m = self.ent(r).base.matrix;
                let hand = self.ped_node_position(r, node::HANDR) + m.rotate_vector(fire_offset);
                let kinds = Kinds { vehicles: true, dummies: true, ..Kinds::NONE };
                let blocked = self.process_line_of_sight(hand, target_pos, kinds, LineOpts::default()).is_some_and(|h| h.entity != target);
                if !blocked {
                    self.set_weapon_lock_on_target(r, Some(target));
                    self.set_attack(px, r, target);
                    let t = px.rng.range_i32(500, 1000) as u32;
                    self.set_shoot_timer(r, t);
                }
                let t = px.rng.range_i32(200, 300) as u32;
                self.set_attack_timer(r, t);
            }
            self.ped_mut(r).set_move_state(move_state::STILL);
        }
    }

    /// Off duty: get into a police car, or wander off if the car isn't one.
    fn cop_get_in_car(&mut self, px: &mut Px, r: EntityRef) {
        let pd = self.ped(r);
        let Some(veh) = pd.my_vehicle.filter(|_| !pd.in_vehicle) else { return };
        let model = self.ent(veh).base.model_index;
        if !crate::vehicle::base::VehicleData::is_law_enforcement_vehicle(model) {
            self.ped_mut(r).my_vehicle = None;
            self.clear_objective(px, r);
            let dir = (px.rng.number() & 7) as i8;
            self.set_wander_path(px, r, dir);
            return;
        }
        match self.try_veh(veh).and_then(|v| v.driver) {
            Some(d) if self.ped(d).ped_type == ped_type::COP => {
                if self.ped(r).objective != objective::ENTER_CAR_AS_PASSENGER {
                    self.set_objective_entity(px, r, objective::ENTER_CAR_AS_PASSENGER, veh);
                }
            }
            Some(d) if self.ped(d).is_player() => self.set_wanted_level_no_drop(1),
            Some(_) => {}
            None => {
                if self.ped(r).objective != objective::ENTER_CAR_AS_DRIVER {
                    self.set_objective_entity(px, r, objective::ENTER_CAR_AS_DRIVER, veh);
                }
            }
        }
    }

    /// `CPed::SetWeaponLockOnTarget`.
    pub fn set_weapon_lock_on_target(&mut self, r: EntityRef, target: Option<EntityRef>) {
        self.ped_mut(r).point_gun_at = target;
    }

    /// `GetWeapon()->Update(m_audioEntityId)`.
    pub fn update_current_weapon(&mut self, r: EntityRef) {
        let (infos, timer) = (&self.ents.weapon_infos, &self.ents.timer);
        let Some(crate::entity::Kind::Ped(pd)) = self.ents.pools.get_mut(r).map(|e| &mut e.kind) else { return };
        let cur = pd.current_weapon as usize;
        pd.weapons[cur].update(infos, timer, true);
    }

    /// `CCopPed::ProcessControl`.
    pub fn cop_process_control(&mut self, px: &mut Px, r: EntityRef) {
        let zone = self.ent(r).phys.as_ref().map_or(0, |p| p.zone_level);
        if zone > 0 && zone as i32 != self.ents.collision_in_memory {
            return;
        }
        self.process_control(px, r);
        if self.ent(r).base.flags.was_postponed {
            return;
        }
        match self.ped(r).ped_state {
            state::DEAD => {
                self.clear_pursuit(px, r);
                self.ped_mut(r).objective = objective::NONE;
                return;
            }
            state::DIE => return,
            state::ARREST_PLAYER => {
                self.arrest_player(px, r);
                return;
            }
            _ => {}
        }
        self.update_current_weapon(r);
        if self.ped(r).moved.length() > 0.0 {
            self.avoid(px, r);
        }
        let (Some(player), Some(target)) = (self.ents.player_ped, self.player_or_vehicle()) else { return };
        let dist = (self.epos(target) - self.epos(r)).length();
        self.cop_mut(r).distance_to_target = dist;
        let player_down = self.ped(player).ped_state == state::ARRESTED || self.ped(player).dying_or_dead();
        if player_down {
            if dist < 5.0 {
                self.set_arrest_player(px, r, player);
                return;
            }
            if self.ped(r).is_ped_in_control() {
                self.set_idle(px, r);
            }
        }
        if self.cop(r).is_in_pursuit && !player_down {
            let said = match self.cop(r).cop_type {
                cop_type::FBI => sound::PED_PURSUIT_FBI,
                cop_type::SWAT => sound::PED_PURSUIT_SWAT,
                cop_type::ARMY => sound::PED_PURSUIT_ARMY,
                _ => sound::PED_PURSUIT_COP,
            };
            self.say(px, r, said);
        }
        if self.ped(r).is_ped_in_control() {
            self.cop_ai(px, r);
        } else if self.ped(r).in_vehicle
            && let Some(v) = self.ped(r).my_vehicle
        {
            self.cop_drive_off(px, r, v);
        }
        if self.ped(r).is_ped_in_control() || self.ped(r).ped_state == state::DRIVING {
            self.scan_for_crimes(r);
        }
        self.cop_arrest_logic(px, r, player);
        if self.cop(r).stop_and_shoot_disabled_zone {
            self.cop_shoot_in_disabled_zone(px, r);
        }
    }

    /// A cop driving a car nobody gave a mission cruises along the roads.
    fn cop_drive_off(&mut self, px: &mut Px, r: EntityRef, v: EntityRef) {
        let ok = self.try_veh(v).is_some_and(|d| d.driver == Some(r) && d.created_by != crate::vehicle::base::MISSION_VEHICLE);
        if ok && self.try_veh(v).is_some_and(|d| d.auto_pilot.car_mission == car_mission::NONE) && self.can_ped_drive_off(r) {
            px.hooks.join_car_with_road_system(v);
            if let Some(d) = self.try_veh_mut(v) {
                d.auto_pilot.car_mission = car_mission::CRUISE;
                d.auto_pilot.driving_style = driving_style::STOP_FOR_CARS;
                d.auto_pilot.cruise_speed = 17;
            }
        }
    }

    /// The idle and seek states of a cop that is about to arrest the player.
    fn cop_arrest_logic(&mut self, px: &mut Px, r: EntityRef, player: EntityRef) {
        let dist = self.cop(r).distance_to_target;
        match self.ped(r).ped_state {
            state::IDLE | state::ATTACK => {
                if self.ped(r).objective == objective::KILL_CHAR_ON_FOOT && self.ped(player).entering_car() && dist < 1.3 {
                    self.set_arrest_player(px, r, player);
                }
            }
            state::SEEK_POS => {
                if self.ped(player).ped_state == state::ARRESTED {
                    self.set_idle(px, r);
                    self.set_look_flag_entity(r, player, false);
                    self.set_look_timer(r, 1000);
                    self.restore_previous_objective(px, r);
                    return;
                }
                let getting_in = self.ped(player).my_vehicle.and_then(|v| self.try_veh(v)).is_some_and(|v| v.num_getting_in != 0);
                if getting_in {
                    // FIX_BUGS: as when arresting in a car without seeking first
                    self.ped_mut(r).distance_to_count_seek_done = 1.3;
                }
                if self.ped(r).flags.duck_and_cover || !self.seek(px, r) {
                    return;
                }
                let player_veh = self.player_vehicle();
                let (z, seek_z) = (self.epos(r).z, self.ped(r).seek_pos.z);
                if player_veh.is_none() && self.ped(player).entering_car() {
                    self.set_arrest_player(px, r, player);
                } else if 1.5 + z <= seek_z || z - 0.3 >= seek_z {
                    self.ped_mut(r).set_move_state(move_state::STILL);
                } else if let Some(v) = player_veh
                    && self.ent(v).phys.as_ref().is_some_and(|p| crate::vehicle::base::VehicleData::can_ped_enter_car(&self.ent(v).base.matrix, p))
                    && self.try_veh(v).is_some_and(|d| d.num_getting_in == 0)
                {
                    self.set_car_jack(px, r, v);
                }
            }
            state::SEEK_ENTITY => {
                let Some(t) = self.ped(r).seek_target else {
                    self.restore_previous_state(px, r);
                    return;
                };
                self.ped_mut(r).seek_pos = self.epos(t);
                if !self.seek(px, r) {
                    return;
                }
                let ps = self.ped(player).ped_state;
                if self.ped(r).objective == objective::KILL_CHAR_ON_FOOT
                    && dist < 2.5
                    && (ps == state::ARRESTED || ps == state::ENTER_CAR || ps == state::CARJACK && dist < 1.3)
                {
                    self.set_arrest_player(px, r, player);
                } else {
                    self.restore_previous_state(px, r);
                }
            }
            _ => {}
        }
    }

    /// A cop far from the camera in a no-police zone shoots, or goes away if nobody would see.
    fn cop_shoot_in_disabled_zone(&mut self, px: &mut Px, r: EntityRef) {
        let mut dont_shoot = false;
        if px.hooks.is_on_screen(r) && px.hooks.cull_zone_visible(r) {
            let seed = self.ent(r).base.random_seed as u32;
            if self.ents.timer.frame_counter.wrapping_add(seed) & 0x1F == 17 {
                let look = self.epos(r) + Vec3::new(0.0, 0.0, 0.7);
                let cam = px.hooks.camera_position();
                let kinds = Kinds { buildings: true, ..Kinds::NONE };
                // he is far away, hit something and now has a building in front of him: nobody needs him
                if self.process_line_of_sight(cam, look, kinds, LineOpts::default()).is_some() {
                    self.flag_to_destroy_when_next_processed(r);
                    dont_shoot = true;
                }
            }
        } else {
            self.flag_to_destroy_when_next_processed(r);
            dont_shoot = true;
        }
        if !dont_shoot {
            let target = self.ped(r).ped_in_objective;
            let pd = self.ped_mut(r);
            pd.flags.stop_and_shoot = true;
            pd.flags.kinda_stay_in_same_place = true;
            pd.flags.is_pointing_gun_at = true;
            self.set_attack(px, r, target);
        }
    }
}
