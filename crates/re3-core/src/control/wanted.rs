//! `Wanted.cpp` and `Crime.h`: the player's wanted level and the cops chasing them.
use crate::entity::EntityRef;
use crate::modelinfo::indices_gta3::mi;
use crate::world::World;
use glam::Vec3;

/// `eCrimeType`.
pub mod crime {
    pub const NONE: u8 = 0;
    pub const POSSESSION_GUN: u8 = 1;
    pub const HIT_PED: u8 = 2;
    pub const HIT_COP: u8 = 3;
    pub const SHOOT_PED: u8 = 4;
    pub const SHOOT_COP: u8 = 5;
    pub const STEAL_CAR: u8 = 6;
    pub const RUN_REDLIGHT: u8 = 7;
    pub const RECKLESS_DRIVING: u8 = 8;
    pub const SPEEDING: u8 = 9;
    pub const RUNOVER_PED: u8 = 10;
    pub const RUNOVER_COP: u8 = 11;
    pub const SHOOT_HELI: u8 = 12;
    pub const PED_BURNED: u8 = 13;
    pub const COP_BURNED: u8 = 14;
    pub const VEHICLE_BURNED: u8 = 15;
    pub const DESTROYED_CESSNA: u8 = 16;
}

/// `CCrimeBeingQd`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrimeQd {
    pub ty: u8,
    pub id: i32,
    pub time: u32,
    pub pos: Vec3,
    pub reported: bool,
    pub police_doesnt_care: bool,
}

impl Default for CrimeQd {
    fn default() -> Self {
        Self { ty: crime::NONE, id: 0, time: 0, pos: Vec3::ZERO, reported: false, police_doesnt_care: false }
    }
}

/// What reporting a crime asks of the rest of the game.
pub trait WantedEnv {
    /// `CDarkel::FrenzyOnGoing`.
    fn frenzy_on_going(&mut self) -> bool;
    /// `CCullZones::GetWantedLevelDrop`.
    fn wanted_level_drop(&mut self) -> i32;
    /// `DMAudio.ReportCrime`.
    fn report_crime(&mut self, ty: u8, pos: Vec3);
}

/// `CWanted`. The script's maximum level lives here too (the original keeps it in statics).
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub chaos: i32,
    pub last_update_time: u32,
    pub last_wanted_level_change: u32,
    pub crime_sensitivity: f32,
    pub current_cops: u8,
    pub max_cops: u8,
    pub max_law_enforcer_vehicles: u8,
    pub cops_beating_suspect: u8,
    pub roadblock_density: i16,
    pub ignored_by_cops: bool,
    pub ignored_by_everyone: bool,
    pub swat_required: bool,
    pub fbi_required: bool,
    pub army_required: bool,
    pub level: i32,
    pub crimes: [CrimeQd; 16],
    pub cops: [Option<EntityRef>; 10],
    /// `MaximumWantedLevel`.
    pub maximum_level: i32,
    /// `nMaximumWantedLevel`: the most chaos there can be.
    pub maximum_chaos: i32,
}

impl Default for Wanted {
    fn default() -> Self {
        Self {
            chaos: 0,
            last_update_time: 0,
            last_wanted_level_change: 0,
            crime_sensitivity: 1.0,
            current_cops: 0,
            max_cops: 0,
            max_law_enforcer_vehicles: 0,
            cops_beating_suspect: 0,
            roadblock_density: 0,
            ignored_by_cops: false,
            ignored_by_everyone: false,
            swat_required: false,
            fbi_required: false,
            army_required: false,
            level: 0,
            crimes: [CrimeQd::default(); 16],
            cops: [None; 10],
            maximum_level: 6,
            maximum_chaos: 6400,
        }
    }
}

impl Wanted {
    /// `CWanted::Initialise`.
    pub fn initialise(&mut self) {
        *self = Self { maximum_level: self.maximum_level, maximum_chaos: self.maximum_chaos, ..Self::default() };
    }

    pub fn are_swat_required(&self) -> bool {
        self.level == 4 || self.swat_required
    }

    pub fn are_fbi_required(&self) -> bool {
        self.level == 5 || self.fbi_required
    }

    pub fn are_army_required(&self) -> bool {
        self.level == 6 || self.army_required
    }

    pub fn is_ignored(&self) -> bool {
        self.ignored_by_cops || self.ignored_by_everyone
    }

    pub fn num_of_helis_required(&self) -> i32 {
        if self.is_ignored() {
            return 0;
        }
        match self.level {
            3 | 4 => 1,
            5 | 6 => 2,
            _ => 0,
        }
    }

    /// `CWanted::SetWantedLevel`.
    pub fn set_wanted_level(&mut self, level: i32, now: u32) {
        let level = level.min(self.maximum_level);
        self.clear_qd_crimes();
        self.chaos = match level {
            0 => 0,
            1 => 60,
            2 => 220,
            3 => 420,
            4 => 820,
            5 => 1620,
            6 => 3220,
            _ => self.chaos,
        };
        self.update_wanted_level(now);
    }

    pub fn set_wanted_level_no_drop(&mut self, level: i32, now: u32) {
        if level > self.level {
            self.set_wanted_level(level, now);
        }
    }

    /// `CWanted::SetMaximumWantedLevel`.
    pub fn set_maximum_wanted_level(&mut self, level: i32) {
        let chaos = match level {
            0 => 0,
            1 => 120,
            2 => 300,
            3 => 600,
            4 => 1200,
            5 => 2400,
            6 => 4800,
            _ => return,
        };
        self.maximum_chaos = chaos;
        self.maximum_level = level;
    }

    pub fn clear_qd_crimes(&mut self) {
        for c in &mut self.crimes {
            c.ty = crime::NONE;
        }
    }

    /// `CWanted::RegisterCrime`.
    pub fn register_crime(&mut self, ty: u8, pos: Vec3, id: i32, police_doesnt_care: bool, now: u32) {
        self.add_crime_to_q(ty, id, pos, false, police_doesnt_care, now);
    }

    /// `CWanted::RegisterCrime_Immediately`.
    pub fn register_crime_immediately(&mut self, env: &mut dyn WantedEnv, ty: u8, pos: Vec3, id: i32, police_doesnt_care: bool, now: u32) {
        if !self.add_crime_to_q(ty, id, pos, false, police_doesnt_care, now) {
            self.report_crime_now(env, ty, pos, police_doesnt_care, now);
        }
    }

    /// `CWanted::AddCrimeToQ`: true if the crime had been reported already.
    pub fn add_crime_to_q(&mut self, ty: u8, id: i32, pos: Vec3, reported: bool, police_doesnt_care: bool, now: u32) -> bool {
        if let Some(c) = self.crimes.iter_mut().find(|c| c.ty == ty && c.id == id) {
            if c.reported {
                return true;
            }
            if reported {
                c.reported = true;
            }
            return false;
        }
        if let Some(c) = self.crimes.iter_mut().find(|c| c.ty == crime::NONE) {
            *c = CrimeQd { ty, id, pos, time: now, reported, police_doesnt_care };
        }
        false
    }

    /// `CWanted::ReportCrimeNow`.
    pub fn report_crime_now(&mut self, env: &mut dyn WantedEnv, ty: u8, pos: Vec3, police_doesnt_care: bool, now: u32) {
        let sensitivity = if env.frenzy_on_going() { self.crime_sensitivity * 0.3 } else { self.crime_sensitivity };
        let drop = env.wanted_level_drop().min(100);
        let mut chaos = (1.0 - drop as f32 / 100.0) * sensitivity;
        if police_doesnt_care {
            chaos *= 0.333;
        }
        let weight = match ty {
            crime::POSSESSION_GUN => 0.0,
            crime::HIT_PED | crime::RECKLESS_DRIVING | crime::SPEEDING => 5.0,
            crime::HIT_COP => 45.0,
            crime::SHOOT_PED => 30.0,
            crime::SHOOT_COP | crime::RUNOVER_COP | crime::COP_BURNED => 80.0,
            crime::STEAL_CAR => 15.0,
            crime::RUN_REDLIGHT => 10.0,
            crime::RUNOVER_PED => 18.0,
            crime::SHOOT_HELI => 400.0,
            crime::PED_BURNED | crime::VEHICLE_BURNED => 20.0,
            crime::DESTROYED_CESSNA => 500.0,
            _ => panic!("Undefined crime type, RegisterCrime, Wanted.cpp"),
        };
        // the original adds a float to an int, so the sum is truncated
        self.chaos = (self.chaos as f32 + weight * chaos) as i32;
        env.report_crime(ty, pos);
        self.update_wanted_level(now);
    }

    /// `CWanted::UpdateWantedLevel`.
    pub fn update_wanted_level(&mut self, now: u32) {
        let before = self.level;
        self.chaos = self.chaos.min(self.maximum_chaos);
        // (level, law enforcer vehicles, cops, roadblocks) by chaos
        let (level, vehicles, cops, roadblocks) = match self.chaos {
            0..=39 => (0, 0, 0, 0),
            40..=199 => (1, 1, 1, 0),
            200..=399 => (2, 2, 3, 0),
            400..=799 => (3, 2, 4, 4),
            800..=1599 => (4, 2, 6, 8),
            1600..=3199 => (5, 3, 8, 10),
            3200.. => (6, 3, 10, 12),
            _ => (self.level, self.max_law_enforcer_vehicles, self.max_cops, self.roadblock_density as u8),
        };
        self.level = level;
        self.max_law_enforcer_vehicles = vehicles;
        self.max_cops = cops;
        self.roadblock_density = roadblocks as i16;
        if before != self.level {
            self.last_wanted_level_change = now;
        }
    }

    /// `CWanted::UpdateCrimesQ`.
    pub fn update_crimes_q(&mut self, env: &mut dyn WantedEnv, now: u32) {
        for i in 0..self.crimes.len() {
            let c = self.crimes[i];
            if c.ty == crime::NONE {
                continue;
            }
            if now > c.time + 500 && !c.reported {
                self.report_crime_now(env, c.ty, c.pos, c.police_doesnt_care, now);
                self.crimes[i].reported = true;
            }
            if now > c.time + 10000 {
                self.crimes[i].ty = crime::NONE;
            }
        }
    }

    /// The cop bookkeeping half of `CWanted::Update`: fixes a wrong count and gaps in the list.
    pub fn tidy_cops(&mut self) {
        let count = self.cops.iter().flatten().count() as u8;
        if count != self.current_cops {
            self.current_cops = count;
        }
        let mut packed: Vec<Option<EntityRef>> = self.cops.iter().copied().filter(Option::is_some).collect();
        packed.resize(self.cops.len(), None);
        self.cops.copy_from_slice(&packed);
    }
}

impl World {
    /// `CWanted::WorkOutPolicePresence`.
    pub fn work_out_police_presence(&self, pos: Vec3, radius: f32, player_vehicle: Option<EntityRef>) -> i32 {
        use crate::entity::{status, Kind};
        let mut count = 0;
        for r in self.ped_refs() {
            let e = self.ent(r);
            if matches!(e.base.model_index, mi::COP | mi::SWAT | mi::FBI | mi::ARMY) && (pos - e.base.position()).length() < radius {
                count += 1;
            }
        }
        for r in self.vehicle_refs() {
            let e = self.ent(r);
            let Kind::Vehicle(v) = &e.kind else { continue };
            if v.flags.is_law_enforcer
                && matches!(e.base.model_index, mi::CHOPPER | mi::PREDATOR | mi::POLICE | mi::ENFORCER)
                && Some(r) != player_vehicle
                && e.base.status != status::ABANDONED
                && e.base.status != status::WRECKED
                && (pos - e.base.position()).length() < radius
            {
                count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Env(Vec<u8>);
    impl WantedEnv for Env {
        fn frenzy_on_going(&mut self) -> bool {
            false
        }
        fn wanted_level_drop(&mut self) -> i32 {
            0
        }
        fn report_crime(&mut self, ty: u8, _pos: Vec3) {
            self.0.push(ty);
        }
    }

    #[test]
    fn chaos_sets_the_level_and_the_cop_budget() {
        let mut w = Wanted::default();
        w.set_wanted_level(3, 100);
        assert_eq!((w.chaos, w.level, w.max_cops, w.roadblock_density, w.last_wanted_level_change), (420, 3, 4, 4, 100));
        w.set_wanted_level_no_drop(2, 200);
        assert_eq!(w.level, 3, "no drop");
        w.set_maximum_wanted_level(1);
        w.set_wanted_level(6, 300);
        assert_eq!((w.level, w.chaos), (1, 60), "capped by the script");
    }

    #[test]
    fn a_crime_is_reported_once_and_adds_chaos() {
        let (mut w, mut env) = (Wanted::default(), Env(vec![]));
        w.register_crime_immediately(&mut env, crime::HIT_COP, Vec3::ZERO, 7, false, 1000);
        assert_eq!((w.chaos, w.level, env.0.len()), (45, 1, 1));
        w.register_crime_immediately(&mut env, crime::HIT_COP, Vec3::ZERO, 7, false, 1100);
        // the original queues an immediate crime as not reported, so asking again reports it again
        assert_eq!(env.0.len(), 2);
        w.register_crime(crime::HIT_PED, Vec3::ZERO, 8, true, 1200);
        w.update_crimes_q(&mut env, 1800);
        assert_eq!(env.0.len(), 4, "queued crimes go off after half a second");
        assert!(w.crimes.iter().any(|c| c.ty == crime::HIT_PED && c.reported));
        w.update_crimes_q(&mut env, 12000);
        assert!(w.crimes.iter().all(|c| c.ty == crime::NONE));
    }

    #[test]
    fn the_cop_list_is_repacked() {
        let mut w = Wanted::default();
        let cop = |h| Some(EntityRef { pool: crate::entity::PoolKind::Ped, handle: h });
        w.cops[2] = cop(1);
        w.cops[5] = cop(2);
        w.current_cops = 9;
        w.tidy_cops();
        assert_eq!((w.cops[0], w.cops[1], w.cops[2], w.current_cops), (cop(1), cop(2), None, 2));
    }
}
