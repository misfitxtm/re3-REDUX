//! `Population.cpp`: counting, choosing and creating the ambient peds, and taking the far ones away again.
use super::ctx::{PedHooks, Px};
use super::cop::cop_type;
use super::data::*;
use super::types::{ped_type, NUM_GANGS};
use crate::anim::assoc_group;
use crate::control::pathfind::PathEnv;
use crate::entity::{status, EntityRef, Kind, PoolKind};
use crate::general::Rng;
use crate::math::Matrix;
use crate::modelinfo::indices_gta3::mi;
use crate::weapons::info::weapon_type as wt;
use crate::world::{Kinds, World};
use glam::Vec3;

const MIN_CREATION_DIST: f32 = 40.0;
const CREATION_RANGE: f32 = 10.0;
const OFFSCREEN_CREATION_MULT: f32 = 0.5;
const PED_REMOVE_DIST: f32 = MIN_CREATION_DIST + CREATION_RANGE + 1.0;
const PED_REMOVE_DIST_SPECIAL: f32 = MIN_CREATION_DIST + CREATION_RANGE + 15.0;
pub const NUM_PED_GROUPS: usize = 31;
pub const MODELS_PER_PED_GROUP: usize = 8;
pub const DEFAULT_MAX_PEDS: f32 = 25.0;
#[allow(clippy::approx_constant)]
const NOT_PI: f32 = 3.14;

/// `eLevelName`.
pub mod level {
    pub const GENERIC: i32 = 0;
    pub const INDUSTRIAL: i32 = 1;
    pub const COMMERCIAL: i32 = 2;
    pub const SUBURBAN: i32 = 3;
}

/// The part of `CZoneInfo` the population reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoneInfo {
    pub car_density: i16,
    pub car_threshold: [i16; 6],
    pub cop_threshold: i16,
    pub gang_threshold: [i16; NUM_GANGS],
    pub ped_density: u16,
    pub cop_density: u16,
    pub gang_density: [u16; NUM_GANGS],
    pub ped_group: u16,
}

/// The `CCarCtrl` counters the population compares with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarCounts {
    pub num_law_enforcer_cars: i32,
    pub num_random_cars: i32,
    pub num_firetrucks_on_duty: i32,
    pub num_ambulances_on_duty: i32,
    pub num_parked_cars: i32,
    pub num_mission_cars: i32,
    pub max_cars_in_use: i32,
    pub car_density_multiplier: f32,
}

impl Default for CarCounts {
    fn default() -> Self {
        Self { num_law_enforcer_cars: 0, num_random_cars: 0, num_firetrucks_on_duty: 0, num_ambulances_on_duty: 0, num_parked_cars: 0, num_mission_cars: 0, max_cars_in_use: 12, car_density_multiplier: 1.0 }
    }
}

/// `RegenerationPoint`: a transition area between two zones.
struct SafeZone {
    src_level: i32,
    dest_level: i32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    z1: f32,
    z2: f32,
    dest_pos_a: Vec3,
    dest_pos_b: Vec3,
    src_pos_a: Vec3,
    src_pos_b: Vec3,
}

const fn sz(src_level: i32, dest_level: i32, b: [f32; 6], p: [[f32; 3]; 4]) -> SafeZone {
    const fn v(a: [f32; 3]) -> Vec3 {
        Vec3::new(a[0], a[1], a[2])
    }
    SafeZone { src_level, dest_level, x1: b[0], x2: b[1], y1: b[2], y2: b[3], z1: b[4], z2: b[5], dest_pos_a: v(p[0]), dest_pos_b: v(p[1]), src_pos_a: v(p[2]), src_pos_b: v(p[3]) }
}

use level::{COMMERCIAL as COM, INDUSTRIAL as IND, SUBURBAN as SUB};

static SAFE_ZONES: [SafeZone; 8] = [
    sz(IND, COM, [400.0, 814.0, -954.0, -903.0, 30.0, 100.0], [[790.0, -917.0, 39.0], [775.0, -921.0, 39.0], [424.0, -942.0, 38.0], [439.0, -938.0, 38.0]]),
    sz(IND, COM, [555.0, 711.0, 118.0, 186.0, -30.0, -10.0], [[698.0, 182.0, -20.0], [681.0, 178.0, -20.0], [586.0, 144.0, -20.0], [577.0, 135.0, -20.0]]),
    sz(IND, COM, [626.0, 744.0, -124.0, -87.0, -20.0, -6.0], [[736.0, -117.0, -13.0], [730.0, -115.0, -13.0], [635.0, -93.0, -12.5], [650.0, -89.0, -12.5]]),
    sz(IND, COM, [645.0, 734.0, -780.0, -750.0, -25.0, -6.0], [[729.0, -764.0, -18.0], [720.0, -769.0, -17.0], [652.0, -774.0, -10.5], [659.0, -770.0, -10.5]]),
    sz(COM, SUB, [-532.0, -136.0, -668.0, -599.0, 34.0, 60.0], [[-172.0, -619.0, 44.0], [-183.0, -623.0, 44.0], [-511.0, -645.0, 41.0], [-493.0, -639.0, 41.5]]),
    sz(COM, SUB, [-325.0, -175.0, 27.0, 75.0, -30.0, -10.0], [[-185.0, 40.8, -20.5], [-202.0, 37.0, -20.5], [-315.0, 65.5, -20.5], [-306.0, 62.4, -20.5]]),
    sz(COM, SUB, [-410.0, -310.0, -1055.0, -1030.0, -20.0, -6.0], [[-321.0, -1043.0, -13.2], [-328.0, -1045.0, -13.2], [-398.0, -1044.0, -13.5], [-390.0, -1040.5, -13.5]]),
    sz(COM, SUB, [-425.0, -280.0, -471.0, -447.0, -20.0, -5.0], [[-292.0, -457.0, -11.6], [-310.0, -461.0, -11.6], [-413.0, -461.0, -11.5], [-399.0, -457.0, -11.3]]),
];

/// `CPopulation`'s statics.
#[derive(Debug, Clone)]
pub struct Population {
    pub ped_groups: [[i32; MODELS_PER_PED_GROUP]; NUM_PED_GROUPS],
    pub give_peds_weapons: bool,
    /// `m_AllRandomPedsThisType`: -1 for no override.
    pub all_random_peds_this_type: i32,
    pub ped_density_multiplier: f32,
    /// `CIniFile::PedNumberMultiplier`.
    pub ped_number_multiplier: f32,
    pub total_mission_peds: u32,
    pub max_peds_in_use: f32,
    pub num_civ_male: u32,
    pub num_civ_female: u32,
    pub num_cop: u32,
    pub num_emergency: u32,
    pub num_gang: [u32; NUM_GANGS],
    pub num_dummy: u32,
    pub zone_change_has_happened: bool,
    pub count_down_to_peds_at_start: i8,
    pub total_peds: u32,
    pub total_gang_peds: u32,
    pub total_civ_peds: u32,
    pub regeneration_point_a: Vec3,
    pub regeneration_point_b: Vec3,
    pub regeneration_front: Vec3,
}

impl Default for Population {
    /// `CPopulation::Initialise` without loading the groups or the zone change.
    fn default() -> Self {
        Self {
            ped_groups: [[0; MODELS_PER_PED_GROUP]; NUM_PED_GROUPS],
            give_peds_weapons: false,
            all_random_peds_this_type: -1,
            ped_density_multiplier: 1.0,
            ped_number_multiplier: 1.0,
            total_mission_peds: 0,
            max_peds_in_use: DEFAULT_MAX_PEDS,
            num_civ_male: 0,
            num_civ_female: 0,
            num_cop: 0,
            num_emergency: 0,
            num_gang: [0; NUM_GANGS],
            num_dummy: 0,
            zone_change_has_happened: false,
            count_down_to_peds_at_start: 2,
            total_peds: 0,
            total_gang_peds: 0,
            total_civ_peds: 0,
            regeneration_point_a: Vec3::ZERO,
            regeneration_point_b: Vec3::ZERO,
            regeneration_front: Vec3::ZERO,
        }
    }
}

impl Population {
    /// `LoadPedGroups`: `text` is pedgrp.dat, `lookup` turns a model name into its id.
    pub fn load_ped_groups(&mut self, text: &str, lookup: impl Fn(&str) -> Option<i32>) {
        for (g, names) in re3_formats::ped::parse_ped_groups(text, MODELS_PER_PED_GROUP).into_iter().take(NUM_PED_GROUPS).enumerate() {
            for (i, n) in names.iter().enumerate() {
                // a name the registry doesn't know leaves the slot at 0, like the original
                self.ped_groups[g][i] = lookup(n).unwrap_or(0);
            }
        }
    }

    /// `UpdatePedCount`.
    pub fn update_ped_count(&mut self, ty: usize, decrease: bool) {
        let d = |n: &mut u32| *n = if decrease { n.wrapping_sub(1) } else { *n + 1 };
        match ty {
            ped_type::CIVMALE | ped_type::CRIMINAL => d(&mut self.num_civ_male),
            ped_type::CIVFEMALE | ped_type::PROSTITUTE => d(&mut self.num_civ_female),
            ped_type::COP => d(&mut self.num_cop),
            ped_type::GANG1..=ped_type::GANG9 => d(&mut self.num_gang[ty - ped_type::GANG1]),
            ped_type::EMERGENCY | ped_type::FIREMAN => d(&mut self.num_emergency),
            ped_type::UNUSED2 => d(&mut self.num_dummy),
            _ => {}
        }
    }

    /// `ChooseCivilianOccupation`: -1 when the group's slot holds -1.
    pub fn choose_civilian_occupation(&self, rng: &mut Rng, group: usize) -> i32 {
        self.ped_groups[group][rng.range_i32(0, MODELS_PER_PED_GROUP as i32) as usize]
    }

    fn update_totals(&mut self) {
        self.total_civ_peds = self.num_civ_female + self.num_civ_male;
        self.total_gang_peds = self.num_gang.iter().sum();
        self.total_peds = self.num_dummy + self.num_emergency + self.num_cop + self.total_gang_peds + self.num_civ_female + self.num_civ_male;
    }
}

/// The model a cop type wears.
pub(crate) fn cop_model(kind: u8) -> i16 {
    match kind {
        cop_type::FBI => mi::FBI,
        cop_type::SWAT => mi::SWAT,
        cop_type::ARMY => mi::ARMY,
        _ => mi::COP,
    }
}

/// `FindCollisionZoneForCoors` without the level lookup: the safe zone the point is in.
pub fn find_safe_zone(c: Vec3) -> Option<usize> {
    let mut found = None;
    for (i, z) in SAFE_ZONES.iter().enumerate() {
        if c.x > z.x1 && c.x < z.x2 && c.y > z.y1 && c.y < z.y2 && c.z > z.z1 && c.z < z.z2 {
            found = Some(i);
        }
    }
    found
}

/// `IsPointInSafeZone`.
pub fn is_point_in_safe_zone(c: Vec3) -> bool {
    find_safe_zone(c).is_some()
}

/// `FindClosestZoneForCoors`.
fn find_closest_zone(c: Vec3, level1: i32, level2: i32) -> Option<usize> {
    let mut min = 10_000_000.0f32;
    let mut best = None;
    for (i, z) in SAFE_ZONES.iter().enumerate() {
        let has = |l: i32| l == z.src_level || l == z.dest_level;
        if has(level1) && has(level2) {
            let d = glam::Vec2::new(c.x - (z.x1 + z.x2) * 0.5, c.y - (z.y1 + z.y2) * 0.5).length();
            if d < min {
                min = d;
                best = Some(i);
            }
        }
    }
    best
}

/// What `GeneratePedCreationCoors` asks of the world: lines, ground and what the camera sees.
struct PopEnv<'a> {
    world: &'a mut World,
    hooks: &'a mut dyn PedHooks,
}

impl PathEnv for PopEnv<'_> {
    fn is_line_of_sight_clear(&mut self, from: Vec3, to: Vec3) -> bool {
        let kinds = crate::world::Kinds { buildings: true, ..crate::world::Kinds::NONE };
        self.world.is_line_of_sight_clear(from, to, kinds, crate::world::LineOpts::default())
    }

    fn is_sphere_visible(&mut self, pos: Vec3, radius: f32, _cam: Option<&Matrix>) -> bool {
        self.hooks.is_sphere_visible(pos, radius)
    }

    fn find_ground_z(&mut self, p: Vec3) -> (f32, bool) {
        self.world.find_ground_z_for_3d_coord(p)
    }

    fn generation_dist_multiplier(&self) -> f32 {
        1.0
    }
}

impl World {
    /// `FindPlayerCentreOfWorld`.
    pub fn find_player_centre_of_world(&mut self, px: &mut Px) -> Vec3 {
        if let Some(c) = px.hooks.camera_generation_centre() {
            return c;
        }
        if let Some(v) = self.ents.player_info.remote_vehicle {
            return self.epos(v);
        }
        if let Some(v) = self.find_player_vehicle() {
            return self.epos(v);
        }
        self.ents.player_ped.map_or(Vec3::ZERO, |p| self.epos(p))
    }

    /// `ChooseGangOccupation`.
    pub fn choose_gang_occupation(&self, rng: &mut Rng, gang: usize) -> i32 {
        let over = self.ents.gangs.0[gang].ped_model_override;
        let first = 2 * gang as i32 + mi::GANG01 as i32;
        match over {
            -1 => rng.range_i32(first, first + 2),
            0 => first,
            _ => first + 1,
        }
    }

    /// `ChoosePolicePedOccupation`: only street cops are spawned here; the others come with their cars.
    pub fn choose_police_ped_occupation(&self, rng: &mut Rng) -> u8 {
        rng.number();
        cop_type::STREET
    }

    /// `PedCreationDistMultiplier`: the faster the player drives the wider the spawn area.
    pub fn ped_creation_dist_multiplier(&self) -> f32 {
        let Some(v) = self.find_player_vehicle() else { return 1.0 };
        let Some(e) = self.get(v) else { return 1.0 };
        let speed = e.phys.as_ref().map_or(0.0, |p| p.move_speed.truncate().length());
        (speed - 0.1 + 1.0).clamp(1.0, 1.5)
    }

    /// `DealWithZoneChange`.
    pub fn deal_with_zone_change(&mut self, old_level: i32, new_level: i32, force_industrial: bool) {
        self.ents.population.zone_change_has_happened = true;
        let around = if force_industrial { Vec3::new(690.0, -920.0, 42.0) } else { self.ents.player_ped.map_or(Vec3::ZERO, |p| self.epos(p)) };
        let mut zone = find_safe_zone(around);
        if zone.is_none() {
            zone = find_closest_zone(around, old_level, new_level);
        }
        // there should always be one; the original would index -1 for a level that has no default
        let fallback = match new_level {
            level::INDUSTRIAL => Some(0),
            level::SUBURBAN => Some(4),
            _ => None,
        };
        let Some(zone) = zone.or(fallback) else { return };
        let z = &SAFE_ZONES[zone];
        let pop = &mut self.ents.population;
        if z.src_level == new_level {
            pop.regeneration_point_a = z.src_pos_a;
            pop.regeneration_point_b = z.src_pos_b;
            pop.regeneration_front = crate::math::normalise(z.dest_pos_a - z.src_pos_a);
        } else if z.dest_level == new_level {
            pop.regeneration_point_a = z.dest_pos_a;
            pop.regeneration_point_b = z.dest_pos_b;
            pop.regeneration_front = crate::math::normalise(z.src_pos_a - z.dest_pos_a);
        }
    }

    /// `CPopulation::RemovePed`.
    pub fn remove_ped(&mut self, px: &mut Px, r: EntityRef) {
        self.remove(r);
        self.destroy_ped(px, r);
    }

    /// `AddPed`: a ped of `ty` standing at `coors`; `mi_or_cop_type` is a model, or a cop type for cops.
    pub fn add_ped(&mut self, px: &mut Px, ty: usize, mi_or_cop_type: i32, coors: Vec3) -> Option<EntityRef> {
        let r = match ty {
            ped_type::CIVMALE | ped_type::CIVFEMALE | ped_type::CRIMINAL | ped_type::PROSTITUTE | ped_type::GANG1..=ped_type::GANG9 => {
                let clump = px.hooks.load_ped_clump(mi_or_cop_type as i16);
                self.create_civilian(px, ty, mi_or_cop_type as i16, clump)?
            }
            ped_type::COP => {
                let clump = px.hooks.load_ped_clump(cop_model(mi_or_cop_type as u8));
                self.create_cop(px, mi_or_cop_type as u8, clump)?
            }
            ped_type::EMERGENCY | ped_type::FIREMAN => {
                let clump = px.hooks.load_ped_clump(if ty == ped_type::EMERGENCY { mi::MEDIC } else { mi::FIREMAN });
                self.create_emergency(px, ty, clump)?
            }
            _ => return None,
        };
        let e = self.get_mut(r)?;
        e.base.set_position(coors);
        e.base.set_orientation(0.0, 0.0, 0.0);
        let _ = self.add(r);
        match ty {
            ped_type::CIVMALE | ped_type::CIVFEMALE if self.ents.population.give_peds_weapons => {
                let weapon = px.rng.range_i32(wt::UNARMED as i32, wt::DETONATOR as i32) as usize;
                if weapon != wt::UNARMED {
                    let w = self.give_weapon(r, weapon, 25001);
                    self.set_current_weapon(r, w);
                }
            }
            ped_type::GANG1..=ped_type::GANG9 => {
                let g = self.ents.gangs.0[ty - ped_type::GANG1];
                let weapon = if px.rng.range_i32(0, 100) >= 50 { g.weapon2 } else { g.weapon1 };
                let w = self.give_weapon(r, weapon as usize, 25001);
                self.set_current_weapon(r, w);
            }
            _ => {}
        }
        Some(r)
    }

    /// `CPopulation::Update`.
    pub fn update_population(&mut self, px: &mut Px) {
        self.manage_population(px);
        self.move_cars_and_peds_out_of_abandoned_zones(px);
        if self.ents.population.count_down_to_peds_at_start != 0 {
            self.ents.population.count_down_to_peds_at_start -= 1;
            if self.ents.population.count_down_to_peds_at_start == 0 {
                self.generate_peds_at_start_of_game(px);
            }
            return;
        }
        self.ents.population.update_totals();
        if !px.hooks.cutscene_running() {
            let pcdm = self.ped_creation_dist_multiplier();
            let gdm = px.hooks.generation_dist_multiplier();
            self.add_to_population(
                px,
                pcdm * (MIN_CREATION_DIST * gdm),
                pcdm * ((MIN_CREATION_DIST + CREATION_RANGE) * gdm),
                pcdm * (MIN_CREATION_DIST + CREATION_RANGE) * OFFSCREEN_CREATION_MULT - CREATION_RANGE,
                pcdm * (MIN_CREATION_DIST + CREATION_RANGE) * OFFSCREEN_CREATION_MULT,
            );
        }
    }

    /// `GeneratePedsAtStartOfGame`.
    pub fn generate_peds_at_start_of_game(&mut self, px: &mut Px) {
        for _ in 0..50 {
            self.ents.population.update_totals();
            let far = self.ped_creation_dist_multiplier() * (MIN_CREATION_DIST + CREATION_RANGE);
            self.add_to_population(px, 10.0, far, 10.0, far);
        }
    }

    /// `AddToPopulation`.
    pub fn add_to_population(&mut self, px: &mut Px, mut min_dist: f32, mut max_dist: f32, min_dist_off_screen: f32, max_dist_off_screen: f32) {
        let Some(player) = self.ents.player_ped else { return };
        let centre = self.find_player_centre_of_world(px);
        let zone = self.ents.zones.get_zone_info_for_time_of_day(centre, &self.ents.clock);
        let wanted = self.ped(player).player.as_deref().map(|p| p.wanted.clone()).unwrap_or_default();
        let cars = self.ents.car_ctrl.counts();
        let mut add_cop = false;
        if wanted.level > 2 && self.ents.population.num_cop < wanted.max_cops as u32 && !self.ped(player).in_vehicle {
            let info = &self.ents.player_info;
            let no_room = cars.num_law_enforcer_cars >= wanted.max_law_enforcer_vehicles as i32
                || cars.num_random_cars as f32 >= info.traffic_multiplier as f32 * cars.car_density_multiplier
                || cars.num_firetrucks_on_duty + cars.num_ambulances_on_duty + cars.num_parked_cars + cars.num_mission_cars + cars.num_law_enforcer_cars + cars.num_random_cars >= cars.max_cars_in_use;
            if no_room {
                add_cop = true;
                min_dist = self.ped_creation_dist_multiplier() * MIN_CREATION_DIST;
                max_dist = self.ped_creation_dist_multiplier() * (MIN_CREATION_DIST + CREATION_RANGE);
            }
        }
        let pop = &self.ents.population;
        let max_possible = ((zone.ped_density as i32 + zone.car_density as i32) as f32 * self.ents.player_info.road_density * pop.ped_density_multiplier * pop.ped_number_multiplier).min(pop.max_peds_in_use);
        if !((pop.total_peds as f32) < max_possible || add_cop) {
            return;
        }

        let decision = px.rng.range_i32(0, 1000);
        let mut model_to_add = 0i32;
        let mut ty = 0usize;
        if decision < zone.cop_density as i32 || add_cop {
            ty = ped_type::COP;
            model_to_add = self.choose_police_ped_occupation(px.rng) as i32;
        } else {
            let mut density = zone.cop_density as i32;
            for i in 0..NUM_GANGS {
                density += zone.gang_density[i] as i32;
                if decision < density {
                    ty = ped_type::GANG1 + i;
                    break;
                }
                if i == NUM_GANGS - 1 {
                    model_to_add = self.ents.population.choose_civilian_occupation(px.rng, zone.ped_group as usize);
                    if model_to_add == -1 {
                        return;
                    }
                    ty = match self.ents.models.get(model_to_add).map(|m| &m.kind) {
                        Some(crate::modelinfo::Kind::Ped(p)) => p.ped_type as usize,
                        _ => return,
                    };
                }
            }
        }
        if !add_cop && self.ents.population.all_random_peds_this_type > ped_type::PLAYER1 as i32 {
            ty = self.ents.population.all_random_peds_this_type as usize;
        }

        let is_gang = (ped_type::GANG1..=ped_type::GANG9).contains(&ty);
        let amount = if is_gang {
            let v = px.rng.number() % 100;
            match v {
                0..=49 => return,
                50..=56 => 1,
                57..=73 => 2,
                74..=84 => 3,
                _ => 4,
            }
        } else {
            1
        };

        let paths = std::mem::take(&mut self.ents.paths);
        let found = paths.generate_ped_creation_coors(px.rng, &mut PopEnv { world: self, hooks: &mut *px.hooks }, centre.x, centre.y, min_dist, max_dist, min_dist_off_screen, max_dist_off_screen, None);
        self.ents.paths = paths;
        let Some((mut coors, ..)) = found else { return };

        let mut leader: Option<EntityRef> = None;
        for i in 0..amount {
            if is_gang {
                model_to_add = self.choose_gang_occupation(px.rng, ty - ped_type::GANG1);
            }
            let needed = if ty == ped_type::COP { cop_model(model_to_add as u8) } else { model_to_add as i16 };
            if !px.hooks.model_loaded(needed) {
                return;
            }
            coors.z += 0.7;
            if let Some(l) = leader {
                // gang members stand in formation around the leader
                let lo = i as f32 * 0.75;
                let hi = (i as f32 + 1.0) * 0.75 - lo;
                let mut x = px.rng.range_f32(lo, lo + hi);
                let mut y = px.rng.range_f32(lo, lo + hi);
                if px.rng.number() & 1 != 0 {
                    x = -x;
                }
                if px.rng.number() & 1 != 0 {
                    y = -y;
                }
                let lp = self.epos(l);
                coors.x = x + lp.x;
                coors.y = y + lp.y;
            }
            if !self.is_position_clear_for_ped(coors) {
                break;
            }
            // the original skips the ground check for the last member
            if i + 1 < amount {
                let (ground, found) = self.find_ground_z_for_3d_coord(Vec3::new(coors.x, coors.y, 2.0 + coors.z));
                if !found {
                    return;
                }
                coors.z = coors.z.max(ground + 0.7);
            }
            if px.hooks.is_sphere_visible(coors, 2.0) && self.ped_creation_dist_multiplier() * MIN_CREATION_DIST > (coors - centre).truncate().length() {
                break;
            }
            let Some(new_ped) = self.add_ped(px, ty, model_to_add, coors) else { return };
            let dest = px.rng.range_i32(0, 8) as i8;
            self.set_wander_path(px, new_ped, dest);
            if i != 0 {
                self.set_leader(new_ped, leader);
            } else {
                leader = Some(new_ped);
            }
            self.ped_mut(new_ped).clump_alpha = 0;
        }
    }

    /// `AddPedInCar`: the driver or passenger a freshly made car gets.
    pub fn add_ped_in_car(&mut self, px: &mut Px, car: EntityRef) -> Option<EntityRef> {
        let mut default_model = mi::MALE01;
        let mut sure_loaded = true;
        let coors = self.ents.player_ped.map_or(Vec3::ZERO, |p| self.epos(p));
        let zone = self.ents.zones.get_zone_info_for_time_of_day(coors, &self.ents.clock);
        let car_model = self.ent(car).base.model_index;
        let taxi = matches!(car_model, mi::TAXI | mi::CABBIE | mi::BORGNINE);
        let fixed: Option<(usize, i32)> = match car_model {
            mi::FIRETRUCK => Some((ped_type::FIREMAN, 0)),
            mi::AMBULAN => Some((ped_type::EMERGENCY, 0)),
            mi::FBICAR => Some((ped_type::COP, cop_type::FBI as i32)),
            mi::POLICE => Some((ped_type::COP, cop_type::STREET as i32)),
            mi::ENFORCER => Some((ped_type::COP, cop_type::SWAT as i32)),
            mi::RHINO | mi::BARRACKS => Some((ped_type::COP, cop_type::ARMY as i32)),
            _ if taxi && px.rng.true_false() => Some((ped_type::CIVMALE, mi::TAXI_D as i32)),
            _ => None,
        };
        let model_ped_type = |w: &World, m: i32| match w.ents.models.get(m).map(|m| &m.kind) {
            Some(crate::modelinfo::Kind::Ped(p)) => p.ped_type as usize,
            _ => ped_type::CIVMALE,
        };
        let (mut ty, mut preferred) = match fixed {
            Some(f) => f,
            None => {
                if taxi {
                    default_model = mi::TAXI_D;
                }
                sure_loaded = false;
                match self.ents.gangs.0.iter().position(|g| g.vehicle_mi == car_model as i32) {
                    Some(g) => (ped_type::GANG1 + g, self.choose_gang_occupation(px.rng, g)),
                    None => {
                        let class = match self.ents.models.get(car_model as i32).map(|m| &m.kind) {
                            Some(crate::modelinfo::Kind::Vehicle(v)) => v.vehicle_class,
                            _ => 0,
                        };
                        let mut pick = default_model as i32;
                        for _ in 0..16 {
                            // each try draws another model of the zone's group
                            pick = self.ents.population.choose_civilian_occupation(px.rng, zone.ped_group as usize);
                            if pick == -1 {
                                pick = default_model as i32;
                            }
                            let drives = match self.ents.models.get(pick).map(|m| &m.kind) {
                                Some(crate::modelinfo::Kind::Ped(p)) => p.cars_can_drive & (1 << class) != 0,
                                _ => false,
                            };
                            if drives {
                                break;
                            }
                            pick = default_model as i32;
                        }
                        (model_ped_type(self, pick), pick)
                    }
                }
            }
        };
        if !sure_loaded && !px.hooks.model_loaded(preferred as i16) {
            preferred = default_model as i32;
            ty = model_ped_type(self, preferred);
        }

        let r = self.add_ped(px, ty, preferred, self.epos(car))?;
        self.get_mut(r).expect("ped").base.flags.uses_collision = false;
        if ty != ped_type::COP {
            self.set_current_weapon(r, wt::COLT45);
            self.remove_weapon_model(r);
        }
        // FIX_BUGS: the pose fits the vehicle instead of always the car seat
        let anim = self.driver_anim(car);
        self.ped_blend_anim(r, assoc_group::STD, anim, 100.0);
        self.ped_mut(r).vehicle_anim = Some(anim as i32);
        self.stop_non_partial_anims(r);
        Some(r)
    }

    /// `MoveCarsAndPedsOutOfAbandonedZones`.
    pub fn move_cars_and_peds_out_of_abandoned_zones(&mut self, px: &mut Px) {
        let frame = self.ents.timer.frame_counter & 7;
        let in_memory = self.ents.collision_in_memory;
        if frame == 1 {
            let mut moved = 0;
            let pool = &self.ents.pools.vehicles;
            let refs: Vec<EntityRef> = (0..pool.size()).rev().filter(|&i| pool.iter().any(|(s, _)| s == i)).map(|i| EntityRef { pool: PoolKind::Vehicle, handle: pool.handle_of(i) }).collect();
            for v in refs {
                let Some(e) = self.get(v) else { continue };
                let zone_level = e.phys.as_ref().map_or(0, |p| p.zone_level);
                if zone_level as i32 != level::GENERIC || self.car_ref(v).is_none() {
                    continue;
                }
                if matches!(e.base.status, status::ABANDONED | status::WRECKED | status::PLAYER | status::PLAYER_REMOTE) {
                    continue;
                }
                let pos = e.base.position();
                let lvl = if find_safe_zone(pos).is_some() { level::GENERIC } else { self.ents.zones.get_level_from_position(pos) };
                // level 0 is a transition area, which stays alone
                if lvl == level::GENERIC || lvl == in_memory || pos.z <= -4.0 {
                    continue;
                }
                let locked_or_kept = self.try_veh(v).is_some_and(|d| d.flags.is_locked || !d.can_be_deleted(|p| self.try_ped(p).is_some_and(|p| p.can_be_deleted())));
                if locked_or_kept {
                    let pop = &self.ents.population;
                    let (a, b, front) = (pop.regeneration_point_a, pop.regeneration_point_b, pop.regeneration_front);
                    let mut to = match moved & 3 {
                        0 => a,
                        1 => b,
                        2 => Vec3::new(a.x, b.y, a.z),
                        _ => Vec3::new(b.x, a.y, a.z),
                    };
                    to.z += (moved / 4) as f32 * 7.0;
                    let e = self.get_mut(v).expect("vehicle");
                    e.base.set_position(to);
                    e.base.matrix.forward = front;
                    self.place_car_on_road_properly(v);
                    px.hooks.join_car_with_road_system(v);
                    let at = self.epos(v);
                    px.hooks.clear_space_for_mission_entity(at, v);
                    moved += 1;
                } else {
                    self.remove(v);
                    self.delete(v);
                }
            }
        } else if frame == 5 {
            for r in self.ped_refs_descending() {
                let Some(pd) = self.try_ped(r) else { continue };
                let zone_level = self.ent(r).phys.as_ref().map_or(0, |p| p.zone_level);
                if zone_level as i32 != level::GENERIC || pd.in_vehicle {
                    continue;
                }
                let pos = self.epos(r);
                let lvl = if find_safe_zone(pos).is_some() { level::GENERIC } else { self.ents.zones.get_level_from_position(pos) };
                if lvl == level::GENERIC || lvl == in_memory || pos.z <= -4.0 {
                    continue;
                }
                if pd.can_be_deleted() {
                    self.remove_ped(px, r);
                } else if !matches!(pd.ped_type, ped_type::PLAYER1 | ped_type::PLAYER2) {
                    let a = self.ents.population.regeneration_point_a;
                    self.get_mut(r).expect("ped").base.set_position(a);
                    let (ground, found) = self.find_ground_z_for_3d_coord(Vec3::new(a.x, a.y, a.z + 2.0));
                    if found {
                        self.get_mut(r).expect("ped").base.matrix.pos.z = 1.0 + ground;
                        let at = self.epos(r);
                        px.hooks.clear_space_for_mission_entity(at, r);
                    }
                }
            }
        }
    }

    /// The live peds, last pool slot first.
    pub(super) fn ped_refs_descending(&self) -> Vec<EntityRef> {
        let pool = &self.ents.pools.peds;
        let mut v: Vec<EntityRef> = pool.iter().map(|(i, _)| EntityRef { pool: PoolKind::Ped, handle: pool.handle_of(i) }).collect();
        v.reverse();
        v
    }

    /// `CPopulation::ConvertToDummyObject`: a loose object far from the player goes back to being scenery.
    pub fn convert_to_dummy_object(&mut self, px: &mut Px, obj: EntityRef) {
        let Some(e) = self.get(obj) else { return };
        let (model, level) = (e.base.model_index, e.base.level);
        let Kind::Object(o) = &e.kind else { return };
        let matrix = o.object_matrix;
        let Some(dummy) = self.new_dummy(model, px.rng) else { return };
        let glass = self.ents.indices.is_glass(&self.ents.models, model);
        let d = self.get_mut(dummy).expect("dummy");
        d.base.matrix = matrix;
        d.base.level = level;
        if glass {
            d.base.flags.is_visible = false;
        }
        self.remove(obj);
        self.delete(obj);
        let _ = self.add(dummy);
    }

    /// `CPopulation::ConvertToRealObject`: scenery near the player becomes an object that can be hit and moved.
    pub fn convert_to_real_object(&mut self, px: &mut Px, dummy: EntityRef) {
        if !self.test_safe_for_real_object(dummy) {
            return;
        }
        let Some(e) = self.get(dummy) else { return };
        let (model, matrix, level) = (e.base.model_index, e.base.matrix, e.base.level);
        let Some(obj) = self.new_object(model, matrix, px.rng) else { return };
        let e = self.get_mut(obj).expect("object");
        e.base.level = level;
        if let Kind::Object(o) = &mut e.kind {
            o.object_matrix = matrix;
        }
        self.remove(dummy);
        self.delete(dummy);
        let _ = self.add(obj);
        if self.ents.indices.is_glass(&self.ents.models, model) {
            self.get_mut(obj).expect("object").base.flags.is_visible = false;
        } else if model == self.ents.indices.buoy() {
            let e = self.get_mut(obj).expect("object");
            e.base.flags.touching_water = true;
            e.phys.as_mut().expect("physical").move_speed = Vec3::new(0.0, 0.0, -0.001);
            self.make_moving(obj);
        }
    }

    /// `CPopulation::ConvertAllObjectsToDummyObjects`.
    pub fn convert_all_objects_to_dummy_objects(&mut self, px: &mut Px) {
        let pool = &self.ents.pools.objects;
        let objs: Vec<EntityRef> = pool.iter().map(|(i, _)| EntityRef { pool: PoolKind::Object, handle: pool.handle_of(i) }).collect();
        for r in objs.into_iter().rev() {
            if self.get(r).is_some_and(|e| matches!(&e.kind, Kind::Object(o) if o.can_be_deleted())) {
                self.convert_to_dummy_object(px, r);
            }
        }
    }

    /// `CPopulation::TestRoomForDummyObject`: nothing stands in the way of putting the scenery back.
    pub fn test_room_for_dummy_object(&mut self, obj: EntityRef) -> bool {
        let Some(e) = self.get(obj) else { return false };
        let Kind::Object(o) = &e.kind else { return false };
        let pos = o.object_matrix.pos;
        let radius = self.col_model_of(obj).map_or(0.0, |c| c.bounding_sphere.radius);
        let kinds = Kinds { vehicles: true, peds: true, ..Kinds::NONE };
        self.find_objects_kinda_colliding(pos, radius, false, 2, kinds).is_empty()
    }

    /// `CPopulation::TestSafeForRealObject`: no car is inside the scenery that is about to become solid.
    pub fn test_safe_for_real_object(&mut self, dummy: EntityRef) -> bool {
        let Some(dummy_col) = self.col_model_of(dummy) else { return true };
        let (pos, matrix) = (self.epos(dummy), self.ent(dummy).base.matrix);
        let radius = dummy_col.bounding_sphere.radius;
        let (col_radius, col_centre) = (self.bound_radius_of(dummy), self.bound_centre(dummy));
        let mut points = vec![crate::collision::ColPoint::default(); crate::collision::MAX_SPHERES];
        for v in self.vehicles_in_sector_box(pos.x - radius, pos.y - radius, pos.x + radius, pos.y + radius) {
            let Some(vc) = self.col_model_of(v) else { continue };
            let e = self.ent(v);
            if e.base.is_touching(&vc, col_centre, col_radius) && crate::collision::process_col_models(&matrix, &dummy_col, &e.base.matrix, &vc, &mut points, &mut [], &mut []) > 0 {
                return false;
            }
        }
        true
    }

    /// The object half of `ManagePopulation`: far temp objects are dropped and far loose ones go back to scenery; near scenery turns real.
    fn manage_object_population(&mut self, px: &mut Px, player_pos: Vec3) {
        let slice = (self.ents.timer.frame_counter & 31) as usize;
        let now = self.now();
        let pool = &self.ents.pools.objects;
        let size = pool.size();
        let candidates: Vec<EntityRef> = pool.iter().filter(|(i, _)| (size * slice / 32..size * (slice + 1) / 32).contains(i)).map(|(i, _)| EntityRef { pool: PoolKind::Object, handle: pool.handle_of(i) }).collect();
        for r in candidates {
            let Some(e) = self.get(r) else { continue };
            let Kind::Object(o) = &e.kind else { continue };
            if !o.can_be_deleted() {
                continue;
            }
            let (created_by, end_of_life) = (o.created_by, o.end_of_life_time);
            let near = (e.base.position() - player_pos).length() <= 80.0 || (o.object_matrix.pos - player_pos).length() <= 80.0;
            let temp = created_by == crate::entity::object::CreatedBy::Temp;
            if near {
                if temp && now > end_of_life {
                    self.remove(r);
                    self.delete(r);
                }
            } else if temp {
                self.remove(r);
                self.delete(r);
            } else if created_by != crate::entity::object::CreatedBy::Cutscene && self.test_room_for_dummy_object(r) {
                self.convert_to_dummy_object(px, r);
            }
        }
        let pool = &self.ents.pools.dummies;
        let size = pool.size();
        let dummies: Vec<EntityRef> = pool.iter().filter(|(i, _)| (size * slice / 32..size * (slice + 1) / 32).contains(i)).map(|(i, _)| EntityRef { pool: PoolKind::Dummy, handle: pool.handle_of(i) }).collect();
        for r in dummies {
            if self.get(r).is_some_and(|e| (e.base.position() - player_pos).length() < 80.0) {
                self.convert_to_real_object(px, r);
            }
        }
    }

    /// `ManagePopulation`, the ped half: fades out and removes the dead and the far away.
    pub fn manage_population(&mut self, px: &mut Px) {
        let player_pos = self.find_player_centre_of_world(px);
        self.manage_object_population(px, player_pos);
        let now = self.now();
        let pcdm = self.ped_creation_dist_multiplier();
        let gdm = px.hooks.generation_dist_multiplier();
        for r in self.ped_refs_descending() {
            let Some(pd) = self.try_ped(r) else { continue };
            if pd.is_player() || !pd.can_be_deleted() || pd.in_vehicle {
                continue;
            }
            if pd.ped_state == state::DEAD && now.wrapping_sub(pd.bloody_footprint_count_or_death_time) > 60000 {
                self.ped_mut(r).flags.fade_out = true;
            }
            let pd = self.ped(r);
            if pd.flags.fade_out && pd.clump_alpha == 0 {
                self.remove_ped(px, r);
                continue;
            }
            let dist = (self.epos(r) - player_pos).truncate().length();
            let on_screen = px.hooks.is_on_screen(r);
            let far = pcdm * (PED_REMOVE_DIST_SPECIAL * gdm) < dist
                || (!pd.flags.cull_extra_far_away && pcdm * PED_REMOVE_DIST * gdm < dist)
                || (pcdm * (MIN_CREATION_DIST + CREATION_RANGE) * OFFSCREEN_CREATION_MULT < dist && !on_screen && px.hooks.camera_allows_offscreen_despawn());
            if !far {
                continue;
            }
            if pd.ped_state == state::DEAD && !pd.flags.fade_out {
                // the original's pi is 3.14
                let angle = (px.rng.number() as u8) as f32 * (NOT_PI / 128.0);
                let outline = (px.rng.number() % 3) as u8;
                let pos = self.epos(r);
                px.hooks.add_dead_ped_outline(pos, outline, angle);
            }
            if on_screen {
                self.ped_mut(r).flags.fade_out = true;
            } else {
                self.remove_ped(px, r);
            }
        }
    }
}
