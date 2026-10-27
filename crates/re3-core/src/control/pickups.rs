//! `Pickups.cpp`: weapons, health, money and the other things lying around to be picked up (GTA III).
use crate::audio::sound;
use crate::entity::object::CreatedBy;
use crate::entity::{EntityRef, Kind};
use crate::math::Matrix;
use crate::modelinfo::indices_gta3::{mi, slot};
use crate::ped::ctx::Px;
use crate::vehicle::base::VehicleData;
use crate::weapons::explosion::ExplosionType;
use crate::weapons::info::weapon_type as wt;
use crate::world::World;
use glam::Vec3;

pub const NUMGENERALPICKUPS: usize = 320;
pub const NUMSCRIPTEDPICKUPS: usize = 16;
pub const NUMPICKUPS: usize = NUMGENERALPICKUPS + NUMSCRIPTEDPICKUPS;
pub const NUMCOLLECTEDPICKUPS: usize = 20;

/// `ePickupType`.
pub mod pickup_type {
    pub const NONE: u8 = 0;
    pub const IN_SHOP: u8 = 1;
    pub const ON_STREET: u8 = 2;
    pub const ONCE: u8 = 3;
    pub const ONCE_TIMEOUT: u8 = 4;
    pub const COLLECTABLE1: u8 = 5;
    pub const IN_SHOP_OUT_OF_STOCK: u8 = 6;
    pub const MONEY: u8 = 7;
    pub const MINE_INACTIVE: u8 = 8;
    pub const MINE_ARMED: u8 = 9;
    pub const NAUTICAL_MINE_INACTIVE: u8 = 10;
    pub const NAUTICAL_MINE_ARMED: u8 = 11;
    pub const FLOATINGPACKAGE: u8 = 12;
    pub const FLOATINGPACKAGE_FLOATING: u8 = 13;
    pub const ON_STREET_SLOW: u8 = 14;
}
use pickup_type as pt;

// 20 entries like the original (a Miami leftover)
pub const AMMO_FOR_WEAPON: [u16; 20] = [0, 1, 45, 125, 25, 150, 300, 25, 5, 250, 5, 5, 0, 500, 0, 100, 0, 0, 0, 0];
pub const AMMO_FOR_WEAPON_ON_STREET: [u16; 20] = [0, 1, 9, 25, 5, 30, 60, 5, 1, 50, 1, 1, 0, 200, 0, 100, 0, 0, 0, 0];
pub const COST_OF_WEAPON: [u16; 20] = [0, 10, 250, 800, 1500, 3000, 5000, 10000, 25000, 25000, 2000, 2000, 0, 50000, 0, 3000, 0, 0, 0, 0];

/// `CPickup`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pickup {
    pub ty: u8,
    pub removed: bool,
    pub quantity: u16,
    pub object: Option<EntityRef>,
    pub timer: u32,
    pub model_index: i16,
    pub index: u16,
    pub pos: Vec3,
}

impl Default for Pickup {
    /// `CPickups::Init`'s state of a slot.
    fn default() -> Self {
        Self { ty: pt::NONE, removed: false, quantity: 0, object: None, timer: 0, model_index: 0, index: 1, pos: Vec3::ZERO }
    }
}

impl Pickup {
    fn is_mine(&self) -> bool {
        (pt::MINE_INACTIVE..=pt::FLOATINGPACKAGE_FLOATING).contains(&self.ty)
    }
}

/// The pickup camera (`CAMERA_PICKUP`) that looks at a camera pickup while a car drives over it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickupCam {
    pub vehicle: EntityRef,
    pub coors: Vec3,
    pub start_time: u32,
}

/// `CPickups`' statics.
#[derive(Debug, Clone)]
pub struct Pickups {
    pub pickups: Vec<Pickup>,
    pub collected: [i32; NUMCOLLECTEDPICKUPS],
    pub collected_index: i16,
    pub cam: Option<PickupCam>,
}

impl Default for Pickups {
    fn default() -> Self {
        Self { pickups: vec![Pickup::default(); NUMPICKUPS], collected: [0; NUMCOLLECTEDPICKUPS], collected_index: 0, cam: None }
    }
}

impl Pickups {
    /// `IsPickUpPickedUp`: true once for each pickup collected.
    pub fn is_pick_up_picked_up(&mut self, id: i32) -> bool {
        match self.collected.iter_mut().find(|c| **c == id) {
            Some(c) => {
                *c = 0;
                true
            }
            None => false,
        }
    }

    /// `PassTime`: a skip in time ages the timers.
    pub fn pass_time(&mut self, time: u32) {
        for p in self.pickups.iter_mut().filter(|p| p.ty != pt::NONE) {
            p.timer = p.timer.saturating_sub(time);
        }
    }

    /// `GetActualPickupIndex`: the slot of a handle, or -1 when the pickup has been replaced.
    pub fn get_actual_pickup_index(&self, index: i32) -> i32 {
        if index == -1 {
            return -1;
        }
        let slot = index as u16 as usize;
        match self.pickups.get(slot) {
            Some(p) if ((index as u32) >> 16) as u16 == p.index => slot as i32,
            _ => -1,
        }
    }

    /// `GetNewUniquePickupIndex`.
    pub fn get_new_unique_pickup_index(&mut self, slot: usize) -> i32 {
        let p = &mut self.pickups[slot];
        p.index = if p.index >= 0xFFFE { 1 } else { p.index + 1 };
        slot as i32 | ((p.index as i32) << 16)
    }

    /// `AddToCollectedPickupsArray`.
    pub fn add_to_collected_pickups_array(&mut self, index: usize) {
        self.collected[self.collected_index as usize] = index as i32 | ((self.pickups[index].index as i32) << 16);
        self.collected_index += 1;
        if self.collected_index as usize >= NUMCOLLECTEDPICKUPS {
            self.collected_index = 0;
        }
    }

    /// `ModelForWeapon`.
    pub fn model_for_weapon(weapon: usize) -> i16 {
        match weapon {
            wt::BASEBALLBAT => mi::BASEBALL_BAT,
            wt::COLT45 => mi::COLT,
            wt::UZI => mi::UZI,
            wt::SHOTGUN => mi::SHOTGUN,
            wt::AK47 => mi::AK47,
            wt::M16 => mi::M16,
            wt::SNIPERRIFLE => mi::SNIPER,
            wt::ROCKETLAUNCHER => mi::ROCKETLAUNCHER,
            wt::FLAMETHROWER => mi::FLAMETHROWER,
            wt::MOLOTOV => mi::MOLOTOV,
            wt::GRENADE => mi::GRENADE,
            _ => 0,
        }
    }
}

impl World {
    fn pickup_model(&self, slot: usize) -> i16 {
        self.ents.indices.gta3_slot(slot)
    }

    /// `WeaponForModel`.
    pub fn weapon_for_model(&self, model: i16) -> usize {
        if model == self.pickup_model(slot::PICKUP_BODYARMOUR) {
            return wt::ARMOUR;
        }
        match model {
            mi::GRENADE => wt::GRENADE,
            mi::AK47 => wt::AK47,
            mi::BASEBALL_BAT => wt::BASEBALLBAT,
            mi::COLT => wt::COLT45,
            mi::MOLOTOV => wt::MOLOTOV,
            mi::ROCKETLAUNCHER => wt::ROCKETLAUNCHER,
            mi::SHOTGUN => wt::SHOTGUN,
            mi::SNIPER => wt::SNIPERRIFLE,
            mi::UZI => wt::UZI,
            mi::M16 => wt::M16,
            mi::FLAMETHROWER => wt::FLAMETHROWER,
            _ => wt::UNARMED,
        }
    }

    /// `GiveUsAPickUpObject`: the object that shows the pickup.
    fn give_us_a_pick_up_object(&mut self, px: &mut Px, i: usize) -> Option<EntityRef> {
        let p = self.ents.pickups.pickups[i];
        let r = self.new_object(p.model_index, Matrix::default(), px.rng)?;
        let bonus = p.model_index == self.pickup_model(slot::PICKUP_BONUS);
        let e = self.get_mut(r)?;
        e.base.set_position(p.pos);
        e.base.set_orientation(0.0, 0.0, -std::f32::consts::FRAC_PI_2);
        e.base.flags.explosion_proof = true;
        e.base.flags.uses_collision = false;
        if let Some(ph) = e.phys.as_mut() {
            ph.flags.affected_by_gravity = false;
        }
        let scorched = p.ty == pt::IN_SHOP_OUT_OF_STOCK;
        e.base.flags.render_scorched |= scorched;
        if let Kind::Object(o) = &mut e.kind {
            o.created_by = CreatedBy::Mission;
            o.is_pickup = true;
            o.bonus_value = if bonus { p.quantity as i8 } else { 0 };
            (o.pickup_with_message, o.out_of_stock) = match p.ty {
                pt::IN_SHOP => (true, false),
                pt::IN_SHOP_OUT_OF_STOCK => (false, true),
                pt::FLOATINGPACKAGE_FLOATING => (o.pickup_with_message, o.out_of_stock),
                _ => (false, false),
            };
        }
        Some(r)
    }

    /// The slot's object leaves the world and the pool.
    fn pickup_drop_object(&mut self, i: usize) {
        if let Some(o) = self.ents.pickups.pickups[i].object.take() {
            self.delete(o);
        }
    }

    /// `CPickup::Remove`.
    fn pickup_remove(&mut self, i: usize) {
        self.pickup_drop_object(i);
        let p = &mut self.ents.pickups.pickups[i];
        p.removed = true;
        p.ty = pt::NONE;
    }

    /// `CPickup::CanBePickedUp`.
    fn pickup_can_be_picked_up(&self, px: &mut Px, player: EntityRef, model: i16) -> bool {
        let (armour, health) = (self.ped(player).armour, self.ped(player).health);
        let wanted = self.ped(player).player.as_deref().map_or(0, |p| p.wanted.level);
        let hide = |px: &mut Px| px.hooks.player_on_mission() || self.ents.darkel.frenzy_on_going() || !px.hooks.nasty_game();
        let cannot = (model == self.pickup_model(slot::PICKUP_BODYARMOUR) && armour > 99.5)
            || (model == self.pickup_model(slot::PICKUP_HEALTH) && health > 99.5)
            || (model == self.pickup_model(slot::PICKUP_BRIBE) && wanted == 0)
            || (model == self.pickup_model(slot::PICKUP_KILLFRENZY) && hide(px));
        !cannot
    }

    /// `IsSphereTouchingVehicle`.
    fn is_sphere_touching_vehicle(&self, veh: EntityRef, sphere: Vec3, radius: f32) -> bool {
        match (self.get(veh), self.col_model_of(veh)) {
            (Some(e), Some(col)) => VehicleData::is_sphere_touching(&e.base.matrix, &col, sphere, radius),
            _ => false,
        }
    }

    fn any_vehicle_touching(&self, sphere: Vec3, radius: f32) -> bool {
        self.vehicle_refs().into_iter().any(|v| self.is_sphere_touching_vehicle(v, sphere, radius))
    }

    /// `GivePlayerGoodiesWithPickUpMI`: the pickups that are not weapons.
    pub fn give_player_goodies_with_pick_up_mi(&mut self, px: &mut Px, model: i16) -> bool {
        let Some(player) = self.ents.player_ped else { return false };
        let now = self.now();
        let m = |s| self.pickup_model(s);
        if model == m(slot::PICKUP_ADRENALINE) {
            let pd = self.ped_mut(player);
            if let Some(p) = pd.player.as_deref_mut() {
                p.adrenaline_active = true;
                p.adrenaline_time = now + 20000;
                p.current_stamina = p.max_stamina;
            }
            px.hooks.front_end_sound_with(sound::PICKUP_ADRENALINE, 0);
        } else if model == m(slot::PICKUP_BODYARMOUR) {
            self.ped_mut(player).armour = 100.0;
            px.hooks.front_end_sound_with(sound::PICKUP_ARMOUR, 0);
        } else if model == m(slot::PICKUP_INFO) || model == m(slot::PICKUP_BONUS) || model == m(slot::PICKUP_KILLFRENZY) {
            px.hooks.front_end_sound_with(sound::PICKUP_BONUS, 0);
        } else if model == m(slot::PICKUP_HEALTH) {
            self.ped_mut(player).health = 100.0;
            px.hooks.front_end_sound_with(sound::PICKUP_HEALTH, 0);
        } else if model == m(slot::PICKUP_BRIBE) {
            let level = self.ped(player).player.as_deref().map_or(0, |p| p.wanted.level);
            if let Some(w) = self.player_wanted_mut() {
                w.set_wanted_level((level - 1).max(0), now);
            }
            px.hooks.front_end_sound_with(sound::PICKUP_BONUS, 0);
        } else {
            return false;
        }
        true
    }

    /// `CPickup::Update`: true when the player has just taken the pickup.
    fn pickup_update(&mut self, px: &mut Px, i: usize, player: EntityRef, vehicle: Option<EntityRef>) -> bool {
        let now = self.now();
        let p = self.ents.pickups.pickups[i];
        let player_pos = self.epos(player);
        if p.removed {
            if now > p.timer {
                // respawn the pickup if the player is far enough
                let dist = (player_pos.x - p.pos.x).powi(2) + (player_pos.y - p.pos.y).powi(2);
                if dist > 100.0 || p.ty == pt::IN_SHOP && dist > 2.4 {
                    let o = self.give_us_a_pick_up_object(px, i);
                    self.ents.pickups.pickups[i].object = o;
                    if let Some(o) = o {
                        let _ = self.add(o);
                        self.ents.pickups.pickups[i].removed = false;
                    }
                }
            }
            return false;
        }
        let Some(obj) = p.object else { return false };
        let model = self.get(obj).map_or(-1, |e| e.base.model_index);
        let opos = self.epos(obj);
        let mut result = false;
        let m = |w: &World, s| w.pickup_model(s);
        if !p.is_mine() {
            let on_foot_touch = |pp: Vec3| (pp.z - opos.z).abs() < 2.0 && (pp.x - opos.x).powi(2) + (pp.y - opos.y).powi(2) < 1.8;
            let touched = if model == m(self, slot::PICKUP_BRIBE) {
                match vehicle {
                    Some(v) => self.is_sphere_touching_vehicle(v, opos, 2.0),
                    None => on_foot_touch(player_pos),
                }
            } else if model == m(self, slot::PICKUP_CAMERA) {
                vehicle.is_some_and(|v| self.is_sphere_touching_vehicle(v, opos, 2.0))
            } else {
                vehicle.is_none() && on_foot_touch(player_pos)
            };
            if touched && self.pickup_can_be_picked_up(px, player, model) {
                px.hooks.start_pad_shake(120, 100);
                result = self.pickup_touched(px, i, player, vehicle, obj, model);
            }
        } else {
            self.pickup_update_mine(px, i, vehicle, obj, &mut result);
        }
        let p = self.ents.pickups.pickups[i];
        if !p.removed && (p.ty == pt::ONCE_TIMEOUT || p.ty == pt::MONEY) && now > p.timer {
            self.pickup_remove(i);
        }
        result
    }

    /// The player touched a pickup that can be taken: the `switch (m_eType)` of `CPickup::Update`.
    fn pickup_touched(&mut self, px: &mut Px, i: usize, player: EntityRef, vehicle: Option<EntityRef>, obj: EntityRef, model: i16) -> bool {
        let now = self.now();
        let p = self.ents.pickups.pickups[i];
        let weapon = self.weapon_for_model(model);
        let unarmed_slot = self.get_weapon_slot(player, wt::UNARMED);
        let select_if_unarmed = |w: &mut World| {
            if w.ped(player).player.as_deref().map(|p| p.selected_wep_slot) == unarmed_slot {
                let s = w.get_weapon_slot(player, weapon).unwrap_or(weapon);
                if let Some(pl) = w.ped_mut(player).player.as_deref_mut() {
                    pl.selected_wep_slot = s;
                }
            }
        };
        let weapon_sound = (model - mi::GRENADE) as i32;
        match p.ty {
            pt::IN_SHOP => {
                let cost = COST_OF_WEAPON[weapon] as i32;
                if self.ents.player_info.money < cost {
                    self.ents.garages.trigger_message("PU_MONY", -1, 6000, -1, now);
                    return false;
                }
                self.ents.player_info.money -= cost;
                if !self.give_player_goodies_with_pick_up_mi(px, model) {
                    self.give_weapon(player, weapon, AMMO_FOR_WEAPON[weapon] as i32);
                    let s = self.get_weapon_slot(player, weapon).unwrap_or(weapon);
                    if let Some(pl) = self.ped_mut(player).player.as_deref_mut() {
                        pl.selected_wep_slot = s;
                    }
                    px.hooks.front_end_sound_with(sound::PICKUP_WEAPON_BOUGHT, weapon_sound);
                }
                self.pickup_drop_object(i);
                let p = &mut self.ents.pickups.pickups[i];
                (p.timer, p.removed) = (now + 5000, true);
            }
            pt::ON_STREET | pt::ON_STREET_SLOW => {
                if !self.give_player_goodies_with_pick_up_mi(px, model) {
                    if weapon != wt::UNARMED {
                        let ammo = if p.quantity != 0 { p.quantity } else { AMMO_FOR_WEAPON_ON_STREET[weapon] };
                        self.give_weapon(player, weapon, ammo as i32);
                        select_if_unarmed(self);
                        px.hooks.front_end_sound_with(sound::PICKUP_WEAPON, weapon_sound);
                    } else if model == self.pickup_model(slot::PICKUP_CAMERA)
                        && let Some(v) = vehicle
                    {
                        px.hooks.front_end_sound_with(sound::PICKUP_BONUS, 0);
                        self.ents.pickups.cam = Some(PickupCam { vehicle: v, coors: self.epos(obj), start_time: now });
                    }
                }
                let delay = match p.ty {
                    pt::ON_STREET => 30000,
                    _ if model == self.pickup_model(slot::PICKUP_BRIBE) => 300000,
                    _ => 720000,
                };
                self.pickup_drop_object(i);
                let p = &mut self.ents.pickups.pickups[i];
                p.timer = now + delay;
                p.removed = true;
            }
            pt::ONCE | pt::ONCE_TIMEOUT => {
                if !self.give_player_goodies_with_pick_up_mi(px, model) {
                    if weapon != wt::UNARMED {
                        let ammo = if p.quantity != 0 { p.quantity } else { AMMO_FOR_WEAPON[weapon] };
                        self.give_weapon(player, weapon, ammo as i32);
                        select_if_unarmed(self);
                    }
                    px.hooks.front_end_sound_with(sound::PICKUP_WEAPON, weapon_sound);
                }
                self.pickup_remove(i);
            }
            pt::COLLECTABLE1 => {
                let pi = &mut self.ents.player_info;
                pi.collected_packages += 1;
                pi.money += 1000;
                if pi.collected_packages == pi.total_packages {
                    pi.money += 1_000_000;
                    self.ents.garages.trigger_message("CO_ALL", -1, 5000, -1, now);
                } else {
                    let (n, total) = (pi.collected_packages, pi.total_packages);
                    self.ents.garages.trigger_message("CO_ONE", n, 5000, total, now);
                }
                self.pickup_remove(i);
                px.hooks.front_end_sound_with(sound::PICKUP_HIDDEN_PACKAGE, 0);
            }
            pt::MONEY => {
                self.ents.player_info.money += p.quantity as i32;
                self.pickup_remove(i);
                px.hooks.front_end_sound_with(sound::PICKUP_MONEY, 0);
            }
            _ => return false,
        }
        true
    }

    /// The mines and floating packages of `CPickup::Update`.
    fn pickup_update_mine(&mut self, px: &mut Px, i: usize, vehicle: Option<EntityRef>, obj: EntityRef, result: &mut bool) {
        let now = self.now();
        let ty = self.ents.pickups.pickups[i].ty;
        let opos = self.epos(obj);
        let water = |w: &World, p: Vec3| w.ents.water.height(p.x, p.y, p.z + 5.0, false);
        let set_z = |w: &mut World, z: f32| {
            if let Some(e) = w.get_mut(obj) {
                e.base.matrix.pos.z = z;
            }
        };
        let mut ty_now = ty;
        if (ty == pt::NAUTICAL_MINE_INACTIVE || ty == pt::NAUTICAL_MINE_ARMED)
            && let Some(z) = water(self, opos)
        {
            set_z(self, z + 0.6);
        }
        let opos = self.epos(obj);
        match ty {
            pt::MINE_INACTIVE => {
                if vehicle.is_some_and(|v| !self.is_sphere_touching_vehicle(v, opos, 2.0)) {
                    ty_now = pt::MINE_ARMED;
                    self.ents.pickups.pickups[i].timer = now + 10000;
                }
            }
            pt::NAUTICAL_MINE_INACTIVE => {
                if !self.any_vehicle_touching(opos, 1.5) {
                    ty_now = pt::NAUTICAL_MINE_ARMED;
                    self.ents.pickups.pickups[i].timer = now + 10000;
                }
            }
            pt::NAUTICAL_MINE_ARMED | pt::MINE_ARMED => {
                if now > self.ents.pickups.pickups[i].timer || self.any_vehicle_touching(opos, 1.5) {
                    let camera = px.hooks.camera_position();
                    self.add_explosion(px.rng, camera, None, None, ExplosionType::Mine, opos, 0);
                    self.pickup_remove(i);
                    return;
                }
            }
            pt::FLOATINGPACKAGE => {
                let step = self.time_step();
                if let Some(e) = self.get_mut(obj)
                    && let Some(ph) = e.phys.as_mut()
                {
                    ph.move_speed.z -= 0.01 * step;
                    let v = ph.move_speed;
                    e.base.matrix.pos += v * step;
                }
                let opos = self.epos(obj);
                if water(self, opos).is_some_and(|z| z >= opos.z) {
                    ty_now = pt::FLOATINGPACKAGE_FLOATING;
                }
            }
            pt::FLOATINGPACKAGE_FLOATING => {
                if let Some(z) = water(self, opos) {
                    set_z(self, z);
                }
                let opos = self.epos(obj);
                if vehicle.is_some_and(|v| self.is_sphere_touching_vehicle(v, opos, 2.0)) {
                    self.pickup_remove(i);
                    *result = true;
                    px.hooks.front_end_sound_with(sound::PICKUP_FLOAT_PACKAGE, 0);
                    return;
                }
            }
            _ => {}
        }
        self.ents.pickups.pickups[i].ty = ty_now;
    }

    /// `CPickups::GenerateNewOne`: the handle of the new pickup, or -1 when there is no room.
    pub fn generate_new_pickup(&mut self, px: &mut Px, pos: Vec3, model: i16, ty: u8, quantity: u32) -> i32 {
        let now = self.now();
        let pk = &self.ents.pickups.pickups;
        let general = &pk[..NUMGENERALPICKUPS];
        let free = if ty == pt::FLOATINGPACKAGE || ty == pt::NAUTICAL_MINE_INACTIVE { pk.iter().rposition(|p| p.ty == pt::NONE) } else { general.iter().position(|p| p.ty == pt::NONE) };
        // no room: replace some money, or else some pickup that times out
        let Some(slot) = free.or_else(|| general.iter().position(|p| p.ty == pt::MONEY)).or_else(|| general.iter().position(|p| p.ty == pt::ONCE_TIMEOUT)) else { return -1 };
        let p = &mut self.ents.pickups.pickups[slot];
        p.ty = ty;
        p.removed = false;
        p.quantity = quantity as u16;
        match ty {
            pt::ONCE_TIMEOUT => p.timer = now + 20000,
            pt::MONEY => p.timer = now + 30000,
            pt::MINE_INACTIVE | pt::MINE_ARMED => (p.ty, p.timer) = (pt::MINE_INACTIVE, now + 1500),
            pt::NAUTICAL_MINE_INACTIVE | pt::NAUTICAL_MINE_ARMED => (p.ty, p.timer) = (pt::NAUTICAL_MINE_INACTIVE, now + 1500),
            _ => {}
        }
        p.model_index = model;
        p.pos = pos;
        let o = self.give_us_a_pick_up_object(px, slot);
        self.ents.pickups.pickups[slot].object = o;
        if let Some(o) = o {
            let _ = self.add(o);
        }
        self.ents.pickups.get_new_unique_pickup_index(slot)
    }

    /// `GenerateNewOne_WeaponType`.
    pub fn generate_new_pickup_weapon_type(&mut self, px: &mut Px, pos: Vec3, weapon: usize, ty: u8, quantity: u32) -> i32 {
        self.generate_new_pickup(px, pos, Pickups::model_for_weapon(weapon), ty, quantity)
    }

    /// `RemovePickUp`.
    pub fn remove_pick_up(&mut self, handle: i32) {
        let i = self.ents.pickups.get_actual_pickup_index(handle);
        if i < 0 {
            return;
        }
        self.pickup_drop_object(i as usize);
        let p = &mut self.ents.pickups.pickups[i as usize];
        (p.ty, p.removed) = (pt::NONE, true);
    }

    /// `RemoveAllFloatingPickups`.
    pub fn remove_all_floating_pickups(&mut self) {
        for i in 0..NUMPICKUPS {
            if matches!(self.ents.pickups.pickups[i].ty, pt::FLOATINGPACKAGE | pt::FLOATINGPACKAGE_FLOATING) {
                self.pickup_drop_object(i);
            }
        }
    }

    /// `CPickups::Update`: a sixth of the general pickups each frame, all the scripted ones.
    pub fn update_pickups(&mut self, px: &mut Px) {
        // the original's FIX_BUGS: nothing happens in a replay
        if self.ents.timer.playing_back {
            return;
        }
        let Some(player) = self.ents.player_ped else { return };
        self.update_pickup_camera(px);
        const SPAN: usize = 6;
        let part = self.ents.timer.frame_counter as usize % SPAN;
        let general = NUMGENERALPICKUPS * part / SPAN..NUMGENERALPICKUPS * (part + 1) / SPAN;
        for i in general.chain(NUMGENERALPICKUPS..NUMPICKUPS) {
            let vehicle = self.find_player_vehicle();
            if self.ents.pickups.pickups[i].ty != pt::NONE && self.pickup_update(px, i, player, vehicle) {
                self.ents.pickups.add_to_collected_pickups_array(i);
            }
        }
    }

    /// The `CAMERA_PICKUP` part of `CPickups::Update`.
    fn update_pickup_camera(&mut self, px: &mut Px) {
        let Some(cam) = self.ents.pickups.cam else { return };
        let (now, player_vehicle) = (self.now(), self.find_player_vehicle());
        let player_pos = self.player_info_pos();
        let dist = (cam.coors - player_pos).truncate().length();
        let mult = if dist < 10.0 { 1.0 - dist / 10.0 } else { 0.0 };
        let height = self.col_model_of(cam.vehicle).map_or(0.0, |c| c.bounding_box.max.z - c.bounding_box.min.z);
        if now.wrapping_sub(cam.start_time) > 750 {
            let mut pos = cam.coors;
            pos.z += (height + 2.0) * mult;
            px.hooks.camera_fixed_for_pickup(pos);
        }
        if player_vehicle != Some(cam.vehicle) || (cam.coors - player_pos).length() > 40.0 || now.wrapping_sub(cam.start_time) > 60000 {
            px.hooks.camera_restore_with_jump_cut();
            self.ents.pickups.cam = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::general::Rng;
    use crate::ped::ctx::PedHooks;
    use crate::ped::types::ped_type;

    /// Records what the pickups play.
    #[derive(Default)]
    struct Spy {
        sounds: Vec<(u16, i32)>,
    }
    impl PedHooks for Spy {
        fn front_end_sound_with(&mut self, sound: u16, param: i32) {
            self.sounds.push((sound, param));
        }
    }

    fn world() -> (World, EntityRef) {
        let mut w = World::new(crate::modelinfo::ModelInfoStore::new(re3_formats::datafile::Game::Gta3), crate::temp_col::TempColModels::new(re3_formats::datafile::Game::Gta3));
        let (mut rng, mut hooks) = (Rng::default(), crate::ped::ctx::NoPedHooks);
        let p = w.create_ped(&mut Px { rng: &mut rng, hooks: &mut hooks }, ped_type::PLAYER1, -1, None).unwrap();
        w.ped_mut(p).player = Some(Default::default());
        w.ents.player_ped = Some(p);
        (w, p)
    }

    fn put(w: &mut World, r: EntityRef, pos: Vec3) {
        w.get_mut(r).unwrap().base.set_position(pos);
    }

    fn new_pickup(w: &mut World, spy: &mut Spy, ty: u8, model: i16, quantity: u32) -> i32 {
        let mut rng = Rng::default();
        w.generate_new_pickup(&mut Px { rng: &mut rng, hooks: spy }, Vec3::new(10.0, 0.0, 0.0), model, ty, quantity)
    }

    fn update(w: &mut World, spy: &mut Spy) {
        let mut rng = Rng::default();
        w.update_pickups(&mut Px { rng: &mut rng, hooks: spy });
    }

    #[test]
    fn a_handle_goes_stale_once_its_slot_is_used_again() {
        let (mut w, _) = world();
        let mut spy = Spy::default();
        let h = new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0);
        assert_eq!(w.ents.pickups.get_actual_pickup_index(h), 0);
        assert_eq!(h, 2 << 16);
        w.remove_pick_up(h);
        assert_eq!(w.ents.pickups.pickups[0].ty, pt::NONE);
        let h2 = new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0);
        assert_eq!((w.ents.pickups.get_actual_pickup_index(h), w.ents.pickups.get_actual_pickup_index(h2), w.ents.pickups.get_actual_pickup_index(-1)), (-1, 0, -1));
    }

    #[test]
    fn standing_on_a_street_weapon_gives_it_and_the_pickup_comes_back_later() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        let h = new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0);
        put(&mut w, player, Vec3::new(10.0, 0.5, 0.0));
        w.ents.timer.time_in_ms = 1000;
        update(&mut w, &mut spy);
        assert_eq!(w.ped(player).weapons[wt::COLT45].ammo_total, 9);
        assert_eq!(w.ped(player).player.as_deref().unwrap().selected_wep_slot, wt::COLT45);
        let p = w.ents.pickups.pickups[0];
        assert!(p.removed && p.object.is_none() && p.timer == 31000 && p.ty == pt::ON_STREET);
        assert_eq!(spy.sounds, vec![(sound::PICKUP_WEAPON, (mi::COLT - mi::GRENADE) as i32)]);
        // the player's own record of it, once
        assert!(w.ents.pickups.is_pick_up_picked_up(h));
        assert!(!w.ents.pickups.is_pick_up_picked_up(h));
        // far away and after the timer the object is back
        put(&mut w, player, Vec3::new(100.0, 0.0, 0.0));
        w.ents.timer.time_in_ms = 31001;
        update(&mut w, &mut spy);
        assert!(!w.ents.pickups.pickups[0].removed && w.ents.pickups.pickups[0].object.is_some());
    }

    #[test]
    fn a_pickup_out_of_reach_stays_put() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0);
        put(&mut w, player, Vec3::new(10.0, 3.0, 0.0));
        update(&mut w, &mut spy);
        assert!(!w.ents.pickups.pickups[0].removed && w.ents.pickups.pickups[0].object.is_some());
        put(&mut w, player, Vec3::new(10.0, 0.0, 5.0));
        update(&mut w, &mut spy);
        assert!(!w.ents.pickups.pickups[0].removed);
    }

    #[test]
    fn money_is_added_once_and_the_pickup_goes() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        new_pickup(&mut w, &mut spy, pt::MONEY, 0, 250);
        put(&mut w, player, Vec3::new(10.0, 0.0, 0.0));
        update(&mut w, &mut spy);
        assert_eq!((w.ents.player_info.money, w.ents.pickups.pickups[0].ty), (250, pt::NONE));
        assert_eq!(spy.sounds, vec![(sound::PICKUP_MONEY, 0)]);
    }

    #[test]
    fn a_shop_pickup_needs_the_money_and_is_restocked_after_five_seconds() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        new_pickup(&mut w, &mut spy, pt::IN_SHOP, mi::UZI, 0);
        put(&mut w, player, Vec3::new(10.0, 0.0, 0.0));
        w.ents.player_info.money = 799;
        update(&mut w, &mut spy);
        assert_eq!(w.ents.garages.message_id, "PU_MONY");
        assert!(!w.ents.pickups.pickups[0].removed);
        w.ents.player_info.money = 1000;
        w.ents.timer.time_in_ms = 100;
        update(&mut w, &mut spy);
        assert_eq!((w.ents.player_info.money, w.ents.pickups.pickups[0].timer, w.ped(player).weapons[wt::UZI].ammo_total), (200, 5100, 125));
        assert_eq!(spy.sounds.last(), Some(&(sound::PICKUP_WEAPON_BOUGHT, (mi::UZI - mi::GRENADE) as i32)));
        // a shop pickup waits for the player to step well away
        w.ents.timer.time_in_ms = 6000;
        put(&mut w, player, Vec3::new(11.0, 0.0, 0.0));
        update(&mut w, &mut spy);
        assert!(w.ents.pickups.pickups[0].removed);
        put(&mut w, player, Vec3::new(13.0, 0.0, 0.0));
        update(&mut w, &mut spy);
        assert!(!w.ents.pickups.pickups[0].removed);
    }

    #[test]
    fn hidden_packages_pay_and_the_last_one_pays_a_million() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        w.ents.player_info.total_packages = 2;
        new_pickup(&mut w, &mut spy, pt::COLLECTABLE1, mi::COLT, 0);
        put(&mut w, player, Vec3::new(10.0, 0.0, 0.0));
        update(&mut w, &mut spy);
        assert_eq!((w.ents.player_info.collected_packages, w.ents.player_info.money, w.ents.garages.message_id.as_str()), (1, 1000, "CO_ONE"));
        new_pickup(&mut w, &mut spy, pt::COLLECTABLE1, mi::COLT, 0);
        update(&mut w, &mut spy);
        assert_eq!((w.ents.player_info.money, w.ents.garages.message_id.as_str()), (1_002_000, "CO_ALL"));
    }

    #[test]
    fn a_timed_pickup_vanishes_when_its_time_is_up() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        new_pickup(&mut w, &mut spy, pt::ONCE_TIMEOUT, mi::COLT, 0);
        put(&mut w, player, Vec3::new(500.0, 0.0, 0.0));
        w.ents.timer.time_in_ms = 20001;
        update(&mut w, &mut spy);
        assert_eq!(w.ents.pickups.pickups[0].ty, pt::NONE);
        assert_eq!(w.ents.pools.objects.used(), 0);
    }

    #[test]
    fn an_armed_mine_blows_up_when_its_timer_runs_out() {
        let (mut w, player) = world();
        let mut spy = Spy::default();
        put(&mut w, player, Vec3::new(500.0, 0.0, 0.0));
        new_pickup(&mut w, &mut spy, pt::MINE_ARMED, mi::COLT, 0);
        // a mine starts inactive and arms itself once no car is on it
        assert_eq!(w.ents.pickups.pickups[0].ty, pt::MINE_INACTIVE);
        w.ents.pickups.pickups[0].ty = pt::MINE_ARMED;
        w.ents.pickups.pickups[0].timer = 5000;
        w.ents.timer.time_in_ms = 5001;
        update(&mut w, &mut spy);
        assert_eq!(w.ents.pickups.pickups[0].ty, pt::NONE);
    }

    #[test]
    fn a_full_table_replaces_money_first_and_then_timed_pickups() {
        let (mut w, _) = world();
        let mut spy = Spy::default();
        for _ in 0..NUMGENERALPICKUPS {
            new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0);
        }
        assert_eq!(new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0), -1);
        w.ents.pickups.pickups[7].ty = pt::ONCE_TIMEOUT;
        w.ents.pickups.pickups[9].ty = pt::MONEY;
        assert_eq!(new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0) & 0xFFFF, 9);
        assert_eq!(new_pickup(&mut w, &mut spy, pt::ON_STREET, mi::COLT, 0) & 0xFFFF, 7);
        // floating packages and nautical mines take the scripted slots from the end
        assert_eq!(new_pickup(&mut w, &mut spy, pt::FLOATINGPACKAGE, mi::COLT, 0) & 0xFFFF, NUMPICKUPS as i32 - 1);
    }

    #[test]
    fn time_passing_ages_the_timers_and_floating_pickups_can_be_cleared() {
        let (mut w, _) = world();
        let mut spy = Spy::default();
        new_pickup(&mut w, &mut spy, pt::MONEY, 0, 5);
        new_pickup(&mut w, &mut spy, pt::FLOATINGPACKAGE, mi::COLT, 0);
        w.ents.pickups.pickups[0].timer = 1500;
        w.ents.pickups.pass_time(1000);
        assert_eq!(w.ents.pickups.pickups[0].timer, 500);
        w.ents.pickups.pass_time(1000);
        assert_eq!(w.ents.pickups.pickups[0].timer, 0);
        w.remove_all_floating_pickups();
        assert!(w.ents.pickups.pickups[NUMPICKUPS - 1].object.is_none() && w.ents.pickups.pickups[0].object.is_some());
    }

    #[test]
    fn weapons_and_their_models_map_both_ways() {
        let (w, _) = world();
        for t in [wt::BASEBALLBAT, wt::COLT45, wt::UZI, wt::SHOTGUN, wt::AK47, wt::M16, wt::SNIPERRIFLE, wt::ROCKETLAUNCHER, wt::FLAMETHROWER, wt::MOLOTOV, wt::GRENADE] {
            assert_eq!(w.weapon_for_model(Pickups::model_for_weapon(t)), t);
        }
        assert_eq!(Pickups::model_for_weapon(wt::DETONATOR), 0);
    }
}
