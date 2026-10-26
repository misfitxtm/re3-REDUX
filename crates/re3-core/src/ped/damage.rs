//! `PedFight.cpp`: hurting and killing peds, body parts coming off, bumping into other peds and being run over.
use super::ctx::Px;
use super::data::*;
use super::pending::Hit;
use super::types::ped_type;
use crate::anim::tables_gta3::anim;
use crate::audio::sound;
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::mi;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::{Vec2, Vec3};

/// The animation a hit from direction 0 to 3 knocks the ped down with.
fn high_impact(direction: u8) -> Option<u32> {
    match direction {
        0 => Some(anim::STD_HIGHIMPACT_FRONT),
        1 => Some(anim::STD_HIGHIMPACT_LEFT),
        2 => Some(anim::STD_HIGHIMPACT_BACK),
        3 => Some(anim::STD_HIGHIMPACT_RIGHT),
        _ => None,
    }
}

/// What a death is made of: the animation and its blend, plus whether the ped lingers.
struct Death {
    anim: u32,
    delta: f32,
    speed: f32,
    will_linger: bool,
    head_shot: bool,
}

impl World {
    /// `FindPlayerVehicle()`.
    pub fn find_player_vehicle(&self) -> Option<EntityRef> {
        let p = self.try_ped(self.ents.player_ped?)?;
        if p.in_vehicle { p.my_vehicle } else { None }
    }

    /// The animation a fallen or fallen-back ped dies with on the ground (`HIT_FLOOR`).
    fn floor_death(&self, r: EntityRef) -> u32 {
        if self.ped_first_assoc(r, crate::anim::flags::FRONTAL).is_some() { anim::STD_HIT_FLOOR_FRONT } else { anim::STD_HIT_FLOOR }
    }

    /// `CPed::InflictDamage`: true if the ped died of it.
    pub fn inflict_damage(&mut self, px: &mut Px, r: EntityRef, hit: Hit) -> bool {
        let Hit { by: damaged_by, weapon: method, damage, piece: ped_piece, direction } = hit;
        let player = self.ents.player_ped == Some(r);
        if player {
            if self.ped(r).player.as_ref().is_some_and(|p| !p.can_be_damaged) {
                return false;
            }
            self.annoy_player_ped(r, false);
        }
        if self.ped(r).dying_or_dead() {
            return false;
        }
        let flags = self.ent(r).base.flags;
        if !flags.uses_collision && method != wt::DROWNING {
            return false;
        }
        if flags.only_damaged_by_player && damaged_by != self.ents.player_ped && damaged_by != self.find_player_vehicle() && method != wt::DROWNING && method != wt::EXPLOSION {
            return false;
        }
        let pd = self.ped(r);
        let mut health_impact = if pd.is_player() { damage * 0.33 } else { damage * self.ents.ped_stats.0[pd.ped_stats].defend_weakness };

        let mut death = Death { anim: anim::STD_KO_FRONT, delta: 4.0, speed: 0.0, will_linger: false, head_shot: false };
        let mut detect_die_anim = true;
        if matches!(pd.ped_state, state::FALL | state::GETUP) {
            if !self.is_ped_head_above_pos(r, -0.3) {
                death.anim = self.floor_death(r);
                death.delta *= 2.0;
                death.speed = 0.5;
                detect_die_anim = false;
            } else if pd.ped_state == state::FALL {
                death.anim = anim::STD_NUM;
                detect_die_anim = false;
            }
        }
        if detect_die_anim && !self.pick_death(px, r, method, ped_piece, direction, health_impact, damaged_by, &mut death) {
            return false;
        }

        let pd = self.ped(r);
        if pd.armour != 0.0 && method != wt::DROWNING {
            if player {
                px.hooks.player_lost_armour_or_health(true);
            }
            let armour = self.ped(r).armour;
            if health_impact < armour {
                self.ped_mut(r).armour = armour - health_impact;
                health_impact = 0.0;
            } else {
                health_impact -= armour;
                self.ped_mut(r).armour = 0.0;
            }
        }
        if health_impact != 0.0 {
            if player {
                px.hooks.player_lost_armour_or_health(false);
            }
            self.ped_mut(r).last_wep_dam = method as i8;
        }
        if self.ped(r).health - health_impact >= 1.0 && !death.will_linger {
            self.ped_mut(r).health -= health_impact;
            return false;
        }
        if self.ped(r).in_vehicle {
            if method != wt::DROWNING {
                self.ped_mut(r).health = 1.0;
                return false;
            }
            self.ped_mut(r).health = 0.0;
            if player && let Some(v) = self.ped(r).my_vehicle {
                self.get_mut(v).expect("vehicle").base.status = crate::entity::status::PLAYER_DISABLED;
            }
            self.set_die(px, r, anim::STD_NUM, 4.0, 0.0);
            true
        } else {
            self.ped_mut(r).health = 0.0;
            self.set_die(px, r, death.anim, death.delta, death.speed);
            if damaged_by == self.ents.player_ped || (damaged_by.is_some() && damaged_by == self.find_player_vehicle()) {
                px.hooks.register_kill(r, method, true, death.head_shot);
                self.ped_mut(r).threat_entity = self.ents.player_ped;
            } else {
                px.hooks.register_kill(r, method, false, false);
            }
            if method == wt::DROWNING {
                self.ped_mut(r).flags.is_in_the_air = false;
            }
            true
        }
    }

    /// The `switch (method)` of `InflictDamage`: chooses the death, false if the ped is immune to this.
    #[allow(clippy::too_many_arguments)]
    fn pick_death(&mut self, px: &mut Px, r: EntityRef, method: usize, piece: u8, direction: u8, health_impact: f32, damaged_by: Option<EntityRef>, death: &mut Death) -> bool {
        let flags = self.ent(r).base.flags;
        let on_floor_anim = |w: &World, death: &mut Death| {
            if w.is_ped_head_above_pos(r, -0.3) {
                death.anim = anim::STD_NUM;
            } else {
                death.anim = w.floor_death(r);
                death.delta *= 2.0;
                death.speed = 0.5;
            }
        };
        match method {
            wt::UNARMED | wt::BASEBALLBAT => {
                if flags.melee_proof {
                    return false;
                }
                if self.ped(r).ped_state == state::FALL {
                    on_floor_anim(self, death);
                } else if let Some(a) = high_impact(direction) {
                    death.anim = a;
                }
            }
            wt::COLT45 | wt::UZI | wt::SHOTGUN | wt::AK47 | wt::M16 | wt::SNIPERRIFLE => {
                if flags.bullet_proof {
                    return false;
                }
                let dont_remove_limb = if self.ped(r).is_player() || self.ped(r).flags.no_critical_hits {
                    true
                } else {
                    match method {
                        wt::SNIPERRIFLE | wt::M16 => false,
                        wt::SHOTGUN => px.rng.number() & 7 != 0,
                        _ => px.rng.number() & 15 != 0,
                    }
                };
                if dont_remove_limb {
                    death.anim = if method == wt::SHOTGUN { high_impact(direction).unwrap_or(death.anim) } else { anim::STD_KO_FRONT };
                    death.will_linger = false;
                } else {
                    match piece {
                        piece::TORSO => {
                            death.will_linger = false;
                            death.anim = anim::STD_KO_FRONT;
                        }
                        piece::MID => {
                            death.will_linger = false;
                            death.anim = anim::STD_KO_SHOT_STOMACH;
                        }
                        piece::LEFTARM => {
                            death.anim = anim::STD_KO_SHOT_ARM_L;
                            self.remove_body_part(px, r, node::UPPERARML, direction as i8);
                            death.will_linger = true;
                        }
                        piece::RIGHTARM => {
                            death.anim = anim::STD_KO_SHOT_ARM_R;
                            self.remove_body_part(px, r, node::UPPERARMR, direction as i8);
                            death.will_linger = true;
                        }
                        piece::LEFTLEG => {
                            death.anim = anim::STD_KO_SHOT_LEG_L;
                            self.remove_body_part(px, r, node::UPPERLEGL, direction as i8);
                            death.will_linger = true;
                        }
                        piece::RIGHTLEG => {
                            death.anim = anim::STD_KO_SHOT_LEG_R;
                            self.remove_body_part(px, r, node::UPPERLEGR, direction as i8);
                            death.will_linger = true;
                        }
                        piece::HEAD => {
                            death.anim = anim::STD_KO_SHOT_FACE;
                            self.remove_body_part(px, r, node::HEAD, direction as i8);
                            death.head_shot = true;
                            death.will_linger = true;
                        }
                        _ => {}
                    }
                }
            }
            wt::ROCKETLAUNCHER | wt::GRENADE | wt::EXPLOSION | wt::MOLOTOV => {
                if flags.explosion_proof {
                    return false;
                }
                if method != wt::MOLOTOV && px.hooks.nasty_game() && !self.ped(r).is_player() && !self.ped(r).in_vehicle {
                    let pd = self.ped(r);
                    if 1.0 + health_impact > pd.armour + pd.health {
                        let random = px.rng.number();
                        for (bit, part) in [(1, node::UPPERARML), (2, node::UPPERLEGR), (4, node::HEAD), (8, node::UPPERARMR), (0x10, node::UPPERLEGL)] {
                            if random & bit != 0 {
                                self.remove_body_part(px, r, part, direction as i8);
                            }
                        }
                        if self.ped(r).flags.body_part_just_came_off {
                            death.will_linger = true;
                        }
                    }
                }
                if let Some(a) = high_impact(direction) {
                    death.anim = a;
                }
            }
            wt::FLAMETHROWER => {
                if flags.fire_proof {
                    return false;
                }
                death.anim = anim::STD_KO_FRONT;
            }
            wt::RAMMEDBYCAR | wt::RUNOVERBYCAR => {
                if flags.collision_proof {
                    return false;
                }
                self.pick_car_death(px, r, method, piece, damaged_by, death);
            }
            wt::DROWNING => death.anim = anim::STD_DROWN,
            wt::FALL => {
                if flags.collision_proof {
                    return false;
                }
                if let Some(a) = high_impact(direction) {
                    death.anim = a;
                }
            }
            _ => {}
        }
        true
    }

    /// The car-hit part of the death choice.
    fn pick_car_death(&mut self, px: &mut Px, r: EntityRef, method: usize, piece: u8, damaged_by: Option<EntityRef>, death: &mut Death) {
        let random = px.rng.number() & 3;
        let diving = self.ped(r).ped_state == state::DIVE_AWAY;
        let (left_arm, right_arm, mid) = (piece == self::piece::LEFTARM, piece == self::piece::RIGHTARM, piece == self::piece::MID);
        death.anim = match random {
            0 => {
                if (!left_arm || random <= 1) && (!mid || random != 1) {
                    if (right_arm && random > 1) || (mid && random == 2) { anim::STD_HIGHIMPACT_RIGHT } else { anim::STD_HIGHIMPACT_FRONT }
                } else {
                    anim::STD_HIGHIMPACT_LEFT
                }
            }
            1 => {
                if diving { anim::STD_SPINFORWARD_LEFT } else { anim::STD_HIGHIMPACT_LEFT }
            }
            2 => {
                if (!left_arm || random <= 1) && (!mid || random != 1) {
                    if (!right_arm || random <= 1) && (!mid || random != 2) { anim::STD_HIGHIMPACT_BACK } else { anim::STD_SPINFORWARD_RIGHT }
                } else {
                    anim::STD_SPINFORWARD_LEFT
                }
            }
            _ => {
                if diving { anim::STD_SPINFORWARD_RIGHT } else { anim::STD_HIGHIMPACT_RIGHT }
            }
        };
        if let Some(v) = damaged_by {
            let speed = self.get(v).and_then(|e| e.phys.as_ref()).map_or(0.0, |p| p.move_speed.length());
            if method == wt::RAMMEDBYCAR {
                death.delta = 8.0 * speed + 4.0;
            } else {
                death.delta = 12.0 * speed + 4.0;
                death.speed = 16.0 * speed + 1.0;
            }
        }
    }

    /// `CPed::RemoveBodyPart`: marks the part as gone; the renderer spawns it and the blood.
    pub fn remove_body_part(&mut self, px: &mut Px, r: EntityRef, node_id: usize, direction: i8) {
        px.hooks.body_part_removed(r, node_id, direction);
        let pd = self.ped_mut(r);
        pd.flags.body_part_just_came_off = true;
        pd.body_part_bleeding = node_id as i8;
    }

    /// `CPed::ApplyHeadShot`.
    pub fn apply_head_shot(&mut self, px: &mut Px, r: EntityRef, pos: Vec3, even_on_player: bool) {
        if self.ped(r).is_player() && !even_on_player {
            return;
        }
        px.hooks.head_shot_effect(r, pos + Vec3::new(0.0, 0.0, 0.1));
        self.set_die(px, r, anim::STD_KO_FRONT, 4.0, 0.0);
        let now = self.now();
        let pd = self.ped_mut(r);
        pd.flags.body_part_just_came_off = true;
        pd.ped_state_timer = now.wrapping_add(150);
    }

    /// `CPed::DoesLOSBulletHitPed` (FIX_BUGS: a line of sight always hits).
    pub fn does_los_bullet_hit_ped(&self, _col_point_z: f32) -> u8 {
        1
    }

    /// `CPed::PlacePedOnDryLand`: after falling in the water, find the shore.
    pub fn place_ped_on_dry_land(&mut self, r: EntityRef) -> bool {
        use crate::world::Kinds;
        let pos = self.epos(r);
        let water_level = self.ents.water.height(pos.x, pos.y, pos.z, true).unwrap_or(0.0);
        let potential_ground = Vec3::new(pos.x, pos.y, water_level);
        let Some((_, contact)) = self.test_sphere_against_world_point(potential_ground, 5.0, None, Kinds { buildings: true, ..Kinds::NONE }, false) else { return false };
        let mut dist = contact - pos;
        dist.z = 0.0;
        let dist = dist.normalize_or_zero();
        let solid = Kinds { buildings: true, vehicles: true, objects: true, ..Kinds::NONE };
        let mut pos_to_check = dist * 0.5 + contact;
        pos_to_check.z = 3.0 + water_level;
        if let Some(h) = self.process_vertical_line(pos_to_check, water_level - 1.0, solid, false, None)
            && h.point.point.z >= water_level
        {
            pos_to_check.z = 0.8 + h.point.point.z;
            self.stand_ped_at(r, pos_to_check);
            return true;
        }
        pos_to_check = dist * 5.0 + pos;
        pos_to_check.z = 3.0 + water_level;
        let Some(h) = self.process_vertical_line(pos_to_check, water_level - 1.0, solid, false, None) else { return false };
        if h.point.point.z < water_level {
            return false;
        }
        pos_to_check.z = 0.8 + h.point.point.z;
        self.stand_ped_at(r, pos_to_check);
        true
    }

    fn stand_ped_at(&mut self, r: EntityRef, pos: Vec3) {
        self.get_mut(r).expect("ped").base.set_position(pos);
        let pd = self.ped_mut(r);
        pd.flags.is_standing = true;
        pd.flags.was_standing = true;
    }

    /// `CPed::CollideWithPed`: what two peds do when they bump into each other.
    pub fn collide_with_ped(&mut self, px: &mut Px, r: EntityRef, other: EntityRef) {
        let now = self.now();
        let we_mission = self.ped(r).char_created_by == MISSION_CHAR;
        let he_mission = self.ped(other).char_created_by == MISSION_CHAR;
        let (pos, other_pos) = (self.epos(r), self.epos(other));
        let pos_diff = other_pos - pos;
        let (our_m, their_m) = (self.ent(r).base.matrix, self.ent(other).base.matrix);
        let other_player = self.ped(other).is_player();
        if !(we_mission || !other_player || self.ped(other).ped_state != state::MAKE_CALL) {
            let running = matches!(self.ped(r).move_state, move_state::RUN | move_state::SPRINT);
            self.set_find_path_and_flee(px, r, other, 5000, !running);
            return;
        }
        let we_dont_look_to_him = pos_diff.dot(our_m.forward) > 0.0;
        let he_looks_to_us = pos_diff.dot(their_m.forward) < 0.0;
        let pd = self.ped(r);
        let (our_stats, their_stats) = (self.ents.ped_stats.0[pd.ped_stats].clone(), self.ents.ped_stats.0[self.ped(other).ped_stats].clone());
        if !matches!(pd.move_state, move_state::NONE | move_state::STILL) {
            let our_speed = pd.player.as_ref().map_or(0.0, |p| p.move_speed);
            let slow = (!pd.is_player() || our_speed <= 1.8)
                && (pd.is_player() || (he_mission && we_mission) || (pd.move_state != move_state::RUN && pd.move_state != move_state::SPRINT));
            if slow {
                self.collide_walking(px, r, other, we_mission, he_mission, we_dont_look_to_him, he_looks_to_us, pos_diff, &our_stats, &their_stats);
            } else if their_stats.defend_weakness <= 1.5 || he_mission {
                self.collide_shove(px, r, other, he_looks_to_us, pos_diff);
            } else {
                self.collide_knock_down(px, r, other, he_looks_to_us, pos_diff);
            }
        } else {
            if !pd.is_ped_in_control() {
                return;
            }
            let their_move = self.ped(other).move_state;
            if matches!(their_move, move_state::NONE | move_state::STILL) {
                return;
            }
            let (ours, theirs) = (pd.ped_type, self.ped(other).ped_type);
            if ours != theirs || ours == ped_type::CIVMALE || ours == ped_type::CIVFEMALE {
                if !we_mission && he_looks_to_us && i32::from(our_stats.fear) > 100 - i32::from(their_stats.temper) {
                    if px.rng.number() & 1 != 0 && now < self.ped(r).ped_state_timer {
                        self.set_evasive_step(px, r, other, 2);
                        self.ped_mut(r).ped_state_timer = now.wrapping_add(3000);
                    } else if their_move > move_state::WALK {
                        self.set_wait_state(px, r, wait_state::PLAYANIM_DUCK, Some(2000));
                    }
                }
            } else if he_looks_to_us && self.ped(other).ped_state != state::STEP_AWAY && self.ped(r).ped_state != state::STEP_AWAY && now > self.ped(r).ped_state_timer {
                self.set_evasive_step(px, r, other, 1);
                self.ped_mut(r).ped_state_timer = now.wrapping_add(3000);
            }
        }
        if self.ped(r).is_player() {
            self.set_look_flag_entity(r, other, true);
            self.set_look_timer(r, 800);
        }
    }

    /// Two walking peds meet: walk round, step aside, shout or fight.
    #[allow(clippy::too_many_arguments)]
    fn collide_walking(&mut self, px: &mut Px, r: EntityRef, other: EntityRef, we_mission: bool, he_mission: bool, we_dont_look_to_him: bool, he_looks_to_us: bool, pos_diff: Vec3, ours: &super::types::PedStats, theirs: &super::types::PedStats) {
        let now = self.now();
        let pd = self.ped(r);
        if pd.objective != objective::FOLLOW_CHAR_IN_FORMATION && pd.objective != objective::GOTO_CHAR_ON_FOOT {
            if now <= pd.ped_state_timer {
                return;
            }
            let other_move = self.ped(other).move_state;
            if he_mission || (!we_mission && other_move != move_state::STILL) {
                let weapon_ours = pd.weapons[pd.current_weapon as usize].ty;
                let _ = weapon_ours;
                if we_mission && matches!(pd.ped_state, state::SEEK_POS | state::SEEK_ENTITY) {
                    if other_move != move_state::STILL && (!self.ped(other).is_player() || px.hooks.player_controls_disabled()) {
                        let seek = pd.seek_pos;
                        let (p, o) = (self.epos(r), self.epos(other));
                        let our_dist = (p.x - seek.x).powi(2) + (p.y - seek.y).powi(2);
                        let his_dist = (o.x - seek.x).powi(2) + (o.y - seek.y).powi(2);
                        if our_dist <= his_dist {
                            self.set_wait_state(px, other, wait_state::CROSS_ROAD_LOOK, Some(1000));
                            self.ped_mut(other).ped_state_timer = now.wrapping_add(1000);
                        } else {
                            self.set_wait_state(px, r, wait_state::CROSS_ROAD_LOOK, Some(500));
                            self.ped_mut(r).ped_state_timer = now.wrapping_add(500);
                        }
                    } else if other_move == move_state::STILL {
                        self.set_direction_to_walk_around_object(px, r, other);
                    }
                } else if we_mission
                    || i32::from(ours.fear) <= 100 - i32::from(theirs.temper)
                    || ((self.ped(other).is_player() || matches!(other_move, move_state::NONE | move_state::STILL)) && (!self.ped(other).is_player() || self.ped(other).player.as_ref().map_or(0.0, |p| p.move_speed) <= 1.0))
                {
                    self.set_direction_to_walk_around_object(px, r, other);
                    if !we_mission {
                        self.say(px, r, sound::PED_CHAT);
                    }
                } else {
                    self.set_evasive_step(px, r, other, 2);
                }
            } else {
                let pd = self.ped(r);
                let unarmed = pd.weapons[pd.current_weapon as usize].ty == wt::UNARMED;
                let their = self.ped(other);
                let their_unarmed = their.weapons[their.current_weapon as usize].ty == wt::UNARMED;
                if ours.temper <= ours.fear || !unarmed || we_mission || their.ped_type == ped_type::CIVFEMALE || their.ped_type == pd.ped_type || !their_unarmed {
                    self.set_direction_to_walk_around_object(px, r, other);
                    self.say(px, r, sound::PED_CHAT);
                } else {
                    self.turn_body(r);
                    self.set_attack(px, r, other);
                }
                let wait = px.rng.range_i32(250, 450) as u32;
                self.ped_mut(r).ped_state_timer = now.wrapping_add(wait);
            }
        } else if pd.ped_in_objective.is_some_and(|t| t == other) && now > pd.ped_state_timer {
            if he_looks_to_us {
                self.set_evasive_step(px, r, other, 1);
                self.ped_mut(r).ped_state_timer = now.wrapping_add(3000);
            }
        } else if we_dont_look_to_him && pd.is_ped_in_control() && pd.ped_stats != self.ped(other).ped_stats {
            if i32::from(theirs.fear) <= 100 - i32::from(ours.temper) {
                let (our_m, their_m) = (self.ent(r).base.matrix, self.ent(other).base.matrix);
                if self.ped(other).is_player() {
                    // he is on our right side
                    let rate = self.ped(r).heading_rate;
                    self.ped_mut(r).rotation_cur += if pos_diff.dot(our_m.right) <= 0.0 { -rate } else { rate };
                } else {
                    let rate = self.ped(other).heading_rate;
                    self.ped_mut(other).rotation_cur += if pos_diff.dot(their_m.right) <= 0.0 { -rate } else { rate };
                }
            } else {
                self.set_look_flag_entity(r, other, false);
                self.turn_body(r);
                if let Some(a) = self.ped_blend_anim(r, crate::anim::assoc_group::STD, anim::STD_PARTIAL_PUNCH, 8.0).and_then(|i| self.ped_assoc_at(r, i)) {
                    a.flags |= crate::anim::flags::FADE_OUT_WHEN_DONE;
                }
                if !he_mission {
                    let dir = self.get_local_direction(other, Vec2::new(pos_diff.x, pos_diff.y));
                    self.start_fight_defend(px, other, dir, hit_level::HIGH, 5);
                }
            }
        }
    }

    /// A lightweight ped is shoved aside by a running one.
    fn collide_shove(&mut self, px: &mut Px, r: EntityRef, other: EntityRef, he_looks_to_us: bool, pos_diff: Vec3) {
        let now = self.now();
        let (our_m, their_m) = (self.ent(r).base.matrix, self.ent(other).base.matrix);
        let their_running = matches!(self.ped(other).move_state, move_state::RUN | move_state::SPRINT);
        let animation = if he_looks_to_us && pos_diff.dot(their_m.right) > 0.0 {
            self.shove_ped(r, our_m.right);
            if !their_running { anim::STD_HIT_LEFT } else { anim::STD_HITBYGUN_LEFT }
        } else if he_looks_to_us {
            self.shove_ped(r, -our_m.right);
            if !their_running { anim::STD_HIT_RIGHT } else { anim::STD_HITBYGUN_RIGHT }
        } else if !their_running {
            anim::STD_HIT_BACK
        } else {
            anim::STD_HITBYGUN_BACK
        };
        if self.ped(other).is_ped_in_control() && now > self.ped(other).ped_state_timer {
            if let Some(a) = self.ped_blend_anim(other, crate::anim::assoc_group::STD, animation, 8.0).and_then(|i| self.ped_assoc_at(other, i)) {
                a.flags |= crate::anim::flags::FADE_OUT_WHEN_DONE;
            }
            self.ped_mut(other).ped_state_timer = now.wrapping_add(1000);
            if self.ped(r).ped_state == state::ATTACK {
                px.hooks.play_one_shot(r, sound::FIGHT_PUNCH_39, 0.0);
            }
        }
    }

    /// A heavy collision knocks the other ped over.
    fn collide_knock_down(&mut self, px: &mut Px, r: EntityRef, other: EntityRef, he_looks_to_us: bool, pos_diff: Vec3) {
        let (our_m, their_m) = (self.ent(r).base.matrix, self.ent(other).base.matrix);
        let animation = if pos_diff.dot(their_m.right) <= 0.0 {
            self.shove_ped(r, -our_m.right);
            if he_looks_to_us { anim::STD_HIGHIMPACT_RIGHT } else { anim::STD_SPINFORWARD_RIGHT }
        } else {
            self.shove_ped(r, our_m.right);
            if he_looks_to_us { anim::STD_HIGHIMPACT_LEFT } else { anim::STD_SPINFORWARD_LEFT }
        };
        if self.ped(r).ped_state == state::ATTACK && self.ped(other).is_ped_in_control() {
            px.hooks.play_one_shot(r, sound::FIGHT_PUNCH_39, 0.0);
        }
        self.set_fall(px, other, 3000, animation, false);
    }

    /// `ApplyMoveForce` along `dir` with a little lift.
    fn shove_ped(&mut self, r: EntityRef, mut dir: Vec3) {
        dir.z += 0.1;
        if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
            p.apply_move_force(dir);
        }
    }

    /// `CPed::KillPedWithCar`.
    pub fn kill_ped_with_car(&mut self, px: &mut Px, r: EntityRef, car: EntityRef, impulse: f32) {
        let st = self.ped(r).ped_state;
        if matches!(st, state::FALL | state::DIE) {
            let car_is_player = self.ent(car).base.status == crate::entity::status::PLAYER;
            if self.ped(r).colliding_entity.is_none() || car_is_player {
                self.ped_mut(r).colliding_entity = Some(car);
            }
            return;
        }
        if st == state::DEAD {
            return;
        }
        if let Some(s) = self.ped(r).cur_surface
            && self.try_veh(s).is_some_and(|v| v.is_boat() || self.ped(r).is_player())
        {
            return;
        }
        let dist_vec = self.epos(r) - self.epos(car);
        let model = self.ent(car).base.model_index;
        let player = self.ped(r).is_player();
        let car_matrix = self.ent(car).base.matrix;
        let car_speed = self.ent(car).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        if (impulse > 12.0 || model == mi::TRAIN) && !player {
            self.run_over_ped(px, r, car, dist_vec, car_speed, car_matrix);
        } else if (self.ent(r).phys.as_ref().is_some_and(|p| p.damage_normal.z < -0.8) && impulse > 3.0) || (impulse > 6.0 && (!player || impulse > 10.0)) {
            self.ped_mut(r).flags.is_standing = false;
            let ped_speed = car_speed;
            let fall_direction = self.get_local_direction(r, Vec2::new(-ped_speed.x, -ped_speed.y)) as u8;
            let damage = if player && model == mi::TRAIN { 150.0 } else { 30.0 };
            self.inflict_damage(px, r, Hit { by: Some(car), weapon: wt::RAMMEDBYCAR, damage, piece: piece::TORSO, direction: fall_direction });
            self.set_fall(px, r, 1000, anim::STD_HIGHIMPACT_FRONT + u32::from(fall_direction), true);
            let normal_z = self.ent(r).phys.as_ref().map_or(0.0, |p| p.damage_normal.z);
            let hit_wall = self.ent(r).base.flags.has_hit_wall;
            if self.ped(r).on_ground() && self.ped(r).colliding_entity.is_none() && (!player || hit_wall || model == mi::TRAIN || normal_z < -0.8) {
                self.ped_mut(r).colliding_entity = Some(car);
            }
            self.ped_mut(r).flags.knocked_up_into_air = false;
            if model != mi::TRAIN
                && !hit_wall
                && let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut())
            {
                p.move_speed = car_speed * 0.75;
            }
            if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
                p.move_speed.z = 0.0;
            }
            let push = dist_vec.normalize_or_zero() * -60.0;
            if let Some(p) = self.get_mut(car).and_then(|e| e.phys.as_mut()) {
                p.apply_move_force(push);
            }
            self.say(px, r, sound::PED_DEFEND);
        }
    }

    /// The fast, heavy hit of `KillPedWithCar`: the ped is flung or run over.
    fn run_over_ped(&mut self, px: &mut Px, r: EntityRef, car: EntityRef, mut dist_vec: Vec3, car_speed: Vec3, car_matrix: crate::math::Matrix) {
        let model = self.ent(car).base.model_index;
        let mut node_to_damage = node::TORSO;
        let mut kill_method = wt::RAMMEDBYCAR;
        let rand_val = px.rng.number() & 3;
        if self.find_player_vehicle() == Some(car) {
            let mass = self.ent(car).phys.as_ref().map_or(1.0, |p| p.mass);
            let freq = (100.0 * car_speed.length() * 2000.0 / mass + 80.0).min(250.0);
            let shake_freq = freq as u8;
            px.hooks.start_pad_shake(40000 / u32::from(shake_freq.max(1)), shake_freq);
        }
        self.ped_mut(r).flags.is_standing = false;
        let ped_speed = self.ent(r).phys.as_ref().map_or(Vec3::ZERO, |p| p.move_speed);
        let mut damage_dir = self.get_local_direction(r, Vec2::new(-ped_speed.x, -ped_speed.y)) as u8;
        let col = self.col_model_of(car);
        let (col_max, col_min) = col.as_ref().map_or((Vec3::ZERO, Vec3::ZERO), |c| (c.bounding_box.max, c.bounding_box.min));
        let right_dot = dist_vec.dot(car_matrix.right);
        let nasty = px.hooks.nasty_game();
        let set_run_over = |w: &mut World, node: &mut usize, method: &mut usize, dir: &mut u8, hit_node: usize, splat: bool, px: &mut Px| {
            *method = wt::RUNOVERBYCAR;
            *node = hit_node;
            if let Some(p) = w.get_mut(r).and_then(|e| e.phys.as_mut()) {
                p.move_speed = car_speed * 0.9;
                p.move_speed.z = 0.0;
            }
            if *dir == 1 || *dir == 3 {
                *dir = 2;
            }
            if splat && nasty {
                px.hooks.play_one_shot(r, sound::SPLATTER, 0.0);
            }
        };
        if model == mi::TRAIN {
            set_run_over(self, &mut node_to_damage, &mut kill_method, &mut damage_dir, node::HEAD, true, px);
        } else if car_speed.dot(car_matrix.forward) >= 0.0 {
            // the car does not look at the ped
            if 0.99 * col_max.x < right_dot.abs() {
                node_to_damage = if right_dot <= 0.0 { node::UPPERARML } else { node::UPPERARMR };
                if dist_vec.dot(car_matrix.forward).abs() < 0.85 * col_max.y {
                    let arm = node_to_damage;
                    set_run_over(self, &mut node_to_damage, &mut kill_method, &mut damage_dir, arm, true, px);
                }
            } else {
                let front_dot = dist_vec.dot(car_matrix.forward);
                if (front_dot <= 0.1 || rand_val == 1) && rand_val != 0 {
                    set_run_over(self, &mut node_to_damage, &mut kill_method, &mut damage_dir, node::HEAD, true, px);
                } else {
                    node_to_damage = node::MID;
                    let front_z = car_matrix.forward.z;
                    let ped_z = self.epos(r).z;
                    let (mut car_highest_z, car_length);
                    if front_z < -0.2 {
                        // highest point of the car's back
                        car_highest_z = car_matrix.transform_point(Vec3::new(0.0, col_min.y, col_max.z)).z;
                        car_length = col_max.y - col_min.y;
                    } else if front_z > 0.1 {
                        // highest point of the car's front
                        car_highest_z = car_matrix.transform_point(Vec3::new(0.0, col_max.y, col_max.z)).z;
                        let highest_z_dist = car_highest_z - ped_z;
                        if highest_z_dist > 0.0 {
                            let mut p = self.epos(r);
                            p.z += 0.5 * highest_z_dist;
                            self.get_mut(r).expect("ped").base.set_position(p);
                            car_highest_z += highest_z_dist * 0.25;
                        }
                        car_length = col_max.y;
                    } else {
                        car_highest_z = car_matrix.transform_point(Vec3::new(0.0, col_max.y, col_max.z)).z;
                        car_length = col_max.y;
                    }
                    let ped_jump_speed = (car_highest_z - self.epos(r).z) / (car_length / car_speed.length());
                    let unknown = ((px.rng.number() % 256) as f32 as f64 * 0.002 + 1.5) as f32 * ped_jump_speed;
                    // after this point dist_vec is not the distance any more
                    dist_vec = car_speed.normalize_or_zero() * (0.2 * unknown);
                    if damage_dir != 1 && damage_dir != 3 {
                        dist_vec.z += unknown;
                    } else {
                        dist_vec.z += 1.5 * unknown;
                    }
                    if let Some(p) = self.get_mut(r).and_then(|e| e.phys.as_mut()) {
                        p.move_speed = dist_vec;
                    }
                    damage_dir += 2;
                    if damage_dir > 3 {
                        damage_dir -= 4;
                    }
                    if self.try_veh(car).is_some_and(|v| v.is_car()) {
                        self.knock_off_bonnet(px, car, car_matrix);
                    }
                }
            }
        }
        if let Some(d) = self.try_veh(car).and_then(|v| v.driver) {
            let event = if self.ped(r).ped_type == ped_type::COP { event_type::HIT_AND_RUN_COP } else { event_type::HIT_AND_RUN };
            let _ = d;
            px.hooks.register_event(event, r, 1000);
        }
        let piece = match node_to_damage {
            node::HEAD => piece::HEAD,
            node::UPPERARML => piece::LEFTARM,
            node::UPPERARMR => piece::RIGHTARM,
            _ => piece::MID,
        };
        self.inflict_damage(px, r, Hit { by: Some(car), weapon: kill_method, damage: 1000.0, piece, direction: damage_dir });
        if self.ped(r).dying_or_dead() && self.ped(r).flags.is_ped_die_anim_playing && self.ped(r).colliding_entity.is_none() {
            self.ped_mut(r).colliding_entity = Some(car);
        }
        self.ped_mut(r).flags.knocked_up_into_air = node_to_damage == node::MID;
        let dir = dist_vec.normalize_or_zero();
        if let Some(p) = self.get_mut(car).and_then(|e| e.phys.as_mut()) {
            p.apply_move_force(dir * -100.0);
        }
        self.say(px, r, sound::PED_DEFEND);
    }

    /// The bonnet of a car that hit a ped flies off with a spin.
    fn knock_off_bonnet(&mut self, px: &mut Px, car: EntityRef, car_matrix: crate::math::Matrix) {
        let Some(bonnet) = self.remove_bonnet_in_ped_collision(car, px.rng) else { return };
        let side = if px.rng.number() & 1 != 0 { 0.1 } else { -0.1 };
        let kick = Vec3::new(side, 0.0, 0.5);
        let world_kick = car_matrix.right * kick.x + car_matrix.forward * kick.y + car_matrix.up * kick.z;
        if let Some(e) = self.get_mut(bonnet)
            && let Some(p) = e.phys.as_mut()
        {
            p.move_speed += world_kick;
            let m = e.base.matrix;
            p.apply_turn_force(&m, car_matrix.up * 10.0, car_matrix.forward);
        }
    }
}
