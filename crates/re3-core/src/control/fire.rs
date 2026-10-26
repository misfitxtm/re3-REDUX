//! `Fire.cpp`: fires on the ground, on peds and on cars.
use crate::entity::EntityRef;
use crate::ped::ctx::{PedParticle, Px};
use crate::ped::data::*;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::Vec3;

/// `NUM_FIRES`.
pub const NUM_FIRES: usize = 40;
/// `CFireManager::MAX_FIREMEN_ATTENDING`.
const MAX_FIREMEN_ATTENDING: u32 = 2;

/// `CFire`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    pub is_ongoing: bool,
    pub is_script_fire: bool,
    pub propagation: bool,
    pub audio_set: bool,
    pub pos: Vec3,
    pub entity: Option<EntityRef>,
    pub source: Option<EntityRef>,
    pub extinguish_time: u32,
    pub start_time: u32,
    pub next_time_to_add_flames: u32,
    pub firemen_putting_out: u32,
    pub strength: f32,
}

impl Default for Fire {
    fn default() -> Self {
        Self {
            is_ongoing: false,
            is_script_fire: false,
            propagation: true,
            audio_set: true,
            pos: Vec3::ZERO,
            entity: None,
            source: None,
            extinguish_time: 0,
            start_time: 0,
            next_time_to_add_flames: 0,
            firemen_putting_out: 0,
            strength: 0.8,
        }
    }
}

/// `CFireManager`.
#[derive(Debug, Clone, PartialEq)]
pub struct FireManager {
    pub total_fires: u32,
    pub fires: Vec<Fire>,
}

impl Default for FireManager {
    fn default() -> Self {
        Self { total_fires: 0, fires: vec![Fire::default(); NUM_FIRES] }
    }
}

impl FireManager {
    /// `GetNextFreeFire`.
    pub fn next_free(&self) -> Option<usize> {
        self.fires.iter().position(|f| !f.is_ongoing && !f.is_script_fire)
    }

    /// `FindNearestFire`: the closest fire with the fewest firemen on it, and how far it is.
    pub fn find_nearest(&self, pos: Vec3) -> (Option<usize>, f32) {
        let mut min_distance = 999999.0;
        for attending in 0..MAX_FIREMEN_ATTENDING {
            let mut found = None;
            min_distance = 999999.0;
            for (j, f) in self.fires.iter().enumerate() {
                if !f.is_ongoing || f.is_script_fire || f.firemen_putting_out != attending {
                    continue;
                }
                let d = (f.pos - pos).truncate().length();
                if d < min_distance {
                    min_distance = d;
                    found = Some(j);
                }
            }
            if found.is_some() {
                return (found, min_distance);
            }
        }
        (None, min_distance)
    }

    /// `FindFurthestFire_NeverMindFireMen`.
    pub fn find_furthest(&self, coords: Vec3, min_range: f32, max_range: f32) -> Option<usize> {
        let (mut furthest, mut last) = (None, 0.0);
        for (i, f) in self.fires.iter().enumerate() {
            if f.is_ongoing && !f.is_script_fire {
                let d = (f.pos - coords).truncate().length();
                if d > min_range && d < max_range && d > last {
                    last = d;
                    furthest = Some(i);
                }
            }
        }
        furthest
    }
}

impl World {
    /// `CFireManager::Update`.
    pub fn update_fires(&mut self, px: &mut Px) {
        for i in 0..NUM_FIRES {
            if self.ents.fires.fires[i].is_ongoing {
                self.process_fire(px, i);
            }
        }
    }

    /// `CFire::ReportThisFire`.
    fn report_fire(&mut self, px: &mut Px, pos: Vec3) {
        self.ents.fires.total_fires += 1;
        px.hooks.register_fire_event(pos, 1000);
    }

    /// `CFireManager::StartFire(pos, size, propagation)`.
    pub fn start_fire_at(&mut self, px: &mut Px, pos: Vec3, size: f32, propagation: bool) {
        let Some(i) = self.ents.fires.next_free() else { return };
        let now = self.now();
        self.ents.fires.fires[i] = Fire {
            is_ongoing: true,
            propagation,
            pos,
            extinguish_time: now + 10000,
            start_time: now + 400,
            strength: size,
            ..Default::default()
        };
        self.report_fire(px, pos);
    }

    /// `CFireManager::StartFire(entityOnFire, fleeFrom, strength, propagation)`: the new fire, if there was room and the entity could burn.
    pub fn start_fire_on(&mut self, px: &mut Px, target: EntityRef, flee_from: Option<EntityRef>, strength: f32, propagation: bool) -> Option<usize> {
        let now = self.now();
        let is_ped = self.is_ped(target);
        if is_ped {
            let pd = self.ped(target);
            if pd.fire.is_some() || !pd.is_ped_in_control() {
                return None;
            }
        } else if let Some(v) = self.try_veh(target) {
            if v.car_fire.is_some() {
                return None;
            }
            if v.is_car() && self.car_ref(target).is_some_and(|c| c.damage.engine_status >= 225) {
                return None;
            }
        }
        let i = self.ents.fires.next_free()?;
        let pos = self.epos(target);
        if is_ped {
            self.ped_mut(target).fire = Some(i);
            if self.ents.player_ped != Some(target) {
                match flee_from {
                    Some(f) => self.set_flee(px, target, f, 10000),
                    None => {
                        self.set_flee_pos(target, glam::Vec2::new(pos.x, pos.y), 10000);
                        self.ped_mut(target).flee_from = None;
                    }
                }
                self.get_mut(target).expect("ped").base.flags.draw_last = false;
                self.ped_mut(target).set_move_state(move_state::SPRINT);
                self.set_move_anim(px, target);
                self.ped_mut(target).ped_state = state::ON_FIRE;
            }
            if let Some(f) = flee_from {
                let event = if self.ped(target).ped_type == super::super::ped::types::ped_type::COP { event_type::COP_SET_ON_FIRE } else { event_type::PED_SET_ON_FIRE };
                px.hooks.register_event_with_culprit(event, target, f, 10000);
            }
        } else if let Some(v) = self.try_veh_mut(target) {
            v.car_fire = Some(i);
            if let Some(f) = flee_from {
                px.hooks.register_event_with_culprit(event_type::CAR_SET_ON_FIRE, target, f, 10000);
            }
        }
        let extinguish_time = if is_ped && self.ped(target).is_player() {
            now + 3333
        } else if self.is_vehicle_ref(target) {
            now + px.rng.range_i32(4000, 5000) as u32
        } else {
            now + px.rng.range_i32(10000, 11000) as u32
        };
        self.ents.fires.fires[i] = Fire {
            is_ongoing: true,
            propagation,
            audio_set: true,
            pos,
            entity: Some(target),
            source: flee_from,
            extinguish_time,
            start_time: now + 400,
            strength,
            ..Default::default()
        };
        self.report_fire(px, pos);
        Some(i)
    }

    /// `CFireManager::ExtinguishPoint`.
    pub fn extinguish_fires_at(&mut self, px: &mut Px, point: Vec3, range: f32) {
        for i in 0..NUM_FIRES {
            let f = &self.ents.fires.fires[i];
            if f.is_ongoing && (point - f.pos).length_squared() < range * range {
                self.extinguish_fire_index(px, i);
            }
        }
    }

    /// `CFire::Extinguish`.
    pub fn extinguish_fire_index(&mut self, px: &mut Px, i: usize) {
        if !self.ents.fires.fires[i].is_ongoing {
            return;
        }
        if !self.ents.fires.fires[i].is_script_fire {
            self.ents.fires.total_fires = self.ents.fires.total_fires.saturating_sub(1);
        }
        let f = &mut self.ents.fires.fires[i];
        f.extinguish_time = 0;
        f.is_ongoing = false;
        if let Some(e) = f.entity.take() {
            if self.is_ped(e) {
                self.restore_previous_state(px, e);
                self.ped_mut(e).fire = None;
            } else if let Some(v) = self.try_veh_mut(e) {
                v.car_fire = None;
            }
        }
    }

    /// `CFire::ProcessFire`.
    fn process_fire(&mut self, px: &mut Px, i: usize) {
        let now = self.now();
        let step = self.time_step();
        let (entity, source) = (self.ents.fires.fires[i].entity, self.ents.fires.fires[i].source);
        if let Some(e) = entity.filter(|&e| self.get(e).is_some()) {
            self.ents.fires.fires[i].pos = self.epos(e);
            if self.is_ped(e) {
                if self.ped(e).fire != Some(i) {
                    self.extinguish_fire_index(px, i);
                    return;
                }
                if self.ped(e).move_state != move_state::RUN {
                    self.ents.fires.fires[i].pos.z -= 1.0;
                }
                let pd = self.ped(e);
                if pd.in_vehicle && pd.my_vehicle.is_some() {
                    if let Some(v) = pd.my_vehicle
                        && self.try_veh(v).is_some_and(|d| d.is_car())
                    {
                        self.try_veh_mut(v).expect("vehicle").health = 75.0;
                    }
                } else {
                    let hit = super::super::ped::pending::Hit { by: source, weapon: wt::FLAMETHROWER, damage: 1.2 * step, piece: piece::TORSO, direction: 0 };
                    let hurt = self.inflict_damage(px, e, hit);
                    if hurt && self.ents.player_ped != Some(e) {
                        self.get_mut(e).expect("ped").base.flags.render_scorched = true;
                    }
                }
            } else if self.is_vehicle_ref(e) {
                if self.try_veh(e).is_some_and(|v| v.car_fire != Some(i)) {
                    self.extinguish_fire_index(px, i);
                    return;
                }
                if !self.ents.fires.fires[i].is_script_fire {
                    self.burn_vehicle(px, e, source, 1.2 * step);
                }
            }
        }
        let pos = self.ents.fires.fires[i].pos;
        if let Some(player) = self.ents.player_ped.filter(|_| self.player_vehicle().is_none()) {
            let pd = self.ped(player);
            let fire_proof = self.ent(player).base.flags.fire_proof;
            if pd.fire.is_none() && !fire_proof && (self.epos(player) - pos).length_squared() < 2.0 {
                self.do_stuff_to_go_on_fire(px, player);
                self.start_fire_on(px, player, source, 0.8, true);
            }
        }
        if now > self.ents.fires.fires[i].next_time_to_add_flames {
            self.ents.fires.fires[i].next_time_to_add_flames = now + 80;
            let mut fire_pos = pos;
            if let Some(e) = entity.filter(|&e| self.try_veh(e).is_some_and(|v| v.is_car())) {
                let headlights = self.ents.models.get(self.ent(e).base.model_index as i32).and_then(|m| match &m.kind {
                    crate::modelinfo::Kind::Vehicle(v) => v.positions.first().copied(),
                    _ => None,
                });
                if let Some(h) = headlights {
                    let p = self.ent(e).base.matrix.transform_point(h);
                    fire_pos = Vec3::new(p.x, p.y, p.z + 0.15);
                }
            }
            let strength = self.ents.fires.fires[i].strength;
            let rise = px.rng.range_f32(0.0125, 0.1) * strength;
            px.hooks.ped_particle(PedParticle::CarFlame, fire_pos, Vec3::new(0.0, 0.0, rise), strength);
            // three rolls the original throws away
            for _ in 0..3 {
                px.rng.number();
            }
            px.hooks.ped_particle(PedParticle::CarFlameSmoke, fire_pos, Vec3::ZERO, 0.0);
        }
        let f = &mut self.ents.fires.fires[i];
        if now < f.extinguish_time || f.is_script_fire {
            if now > f.start_time {
                f.start_time = now + 400;
            }
            let rand = (px.rng.number() & 127) as f32;
            px.hooks.fire_light(pos, rand / 128.0, rand / 128.0, entity.is_none(), rand as i32 / 2);
        } else {
            self.extinguish_fire_index(px, i);
        }
    }

    /// `veh->InflictDamage(source, WEAPONTYPE_FLAMETHROWER, damage)` on a vehicle in the pool.
    fn burn_vehicle(&mut self, px: &mut Px, veh: EntityRef, source: Option<EntityRef>, damage: f32) {
        let Some(mut e) = self.ents.pools.take(veh) else { return };
        self.inflict_vehicle_damage(px.rng, &mut e, source, crate::vehicle::base::weapon::FLAMETHROWER, damage);
        self.ents.pools.restore(veh, e);
    }

    /// `CPlayerPed::DoStuffToGoOnFire`.
    pub fn do_stuff_to_go_on_fire(&mut self, px: &mut Px, player: EntityRef) {
        if self.ped(player).ped_state == state::SNIPER_MODE {
            px.hooks.clear_player_weapon_mode();
        }
    }
}
