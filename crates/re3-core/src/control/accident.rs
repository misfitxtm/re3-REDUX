//! `Accident.cpp`: injured peds that medics should come and revive.
use crate::entity::EntityRef;
use crate::ped::data::MISSION_CHAR;
use crate::world::{Kinds, World};
use glam::Vec3;

/// `NUM_ACCIDENTS`.
pub const NUM_ACCIDENTS: usize = 20;
/// `CAccidentManager::MAX_MEDICS_TO_ATTEND_ACCIDENT`.
const MAX_MEDICS_TO_ATTEND_ACCIDENT: u32 = 2;

/// `CAccident`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Accident {
    pub victim: Option<EntityRef>,
    pub medics_attending: u32,
    pub medics_performing_cpr: u32,
}

/// `CAccidentManager`.
#[derive(Debug, Clone, PartialEq)]
pub struct AccidentManager {
    pub accidents: [Accident; NUM_ACCIDENTS],
}

impl Default for AccidentManager {
    fn default() -> Self {
        Self { accidents: [Accident::default(); NUM_ACCIDENTS] }
    }
}

impl AccidentManager {
    /// `CountActiveAccidents`.
    pub fn count_active_accidents(&self) -> usize {
        self.accidents.iter().filter(|a| a.victim.is_some()).count()
    }

    pub fn next_free(&self) -> Option<usize> {
        self.accidents.iter().position(|a| a.victim.is_none())
    }

    pub fn count_active(&self) -> usize {
        self.accidents.iter().filter(|a| a.victim.is_some()).count()
    }

    pub fn work_to_do_for_medics(&self) -> bool {
        self.accidents.iter().any(|a| a.victim.is_some() && a.medics_attending < MAX_MEDICS_TO_ATTEND_ACCIDENT)
    }

    pub fn unattended(&self) -> bool {
        self.accidents.iter().any(|a| a.victim.is_some() && a.medics_attending == 0)
    }
}

impl World {
    /// `CAccidentManager::ReportAccident`.
    pub fn report_accident(&mut self, ped: EntityRef) {
        let Some(pd) = self.try_ped(ped) else { return };
        let in_water = self.ent(ped).phys.as_ref().is_some_and(|p| p.flags.is_in_water);
        let scorched = self.ent(ped).base.flags.render_scorched;
        if pd.is_player() || pd.char_created_by == MISSION_CHAR || scorched || pd.flags.body_part_just_came_off || !pd.flags.allow_medics_to_revive_me || in_water {
            return;
        }
        if self.ents.accidents.accidents.iter().any(|a| a.victim == Some(ped)) || pd.current_phys_surface.is_some() {
            return;
        }
        let mut point = self.epos(ped);
        point.z -= 2.0;
        if self.process_vertical_line(point, -100.0, Kinds { buildings: true, ..Kinds::NONE }, false, None).is_some() {
            return;
        }
        if let Some(i) = self.ents.accidents.next_free() {
            self.ents.accidents.accidents[i] = Accident { victim: Some(ped), medics_attending: 0, medics_performing_cpr: 0 };
            self.ped_mut(ped).last_accident = Some(i);
        }
    }

    /// `CAccidentManager::FindNearestAccident`: the nearest accident with the fewest medics doing CPR, and its distance.
    pub fn find_nearest_accident(&self, pos: Vec3) -> (Option<usize>, f32) {
        let mut min_distance = 999999.0;
        for performing in 0..MAX_MEDICS_TO_ATTEND_ACCIDENT {
            let mut found = None;
            min_distance = 999999.0;
            for (j, a) in self.ents.accidents.accidents.iter().enumerate() {
                let Some(victim) = a.victim.and_then(|v| self.try_ped(v).map(|d| (v, d))) else { continue };
                if victim.1.char_created_by == MISSION_CHAR || victim.1.health != 0.0 || a.medics_performing_cpr != performing {
                    continue;
                }
                let vp = self.epos(victim.0);
                let distance = (vp - pos).truncate().length();
                if distance / 2.0 > vp.z - pos.z && distance < min_distance {
                    min_distance = distance;
                    found = Some(j);
                }
            }
            if found.is_some() {
                return (found, min_distance);
            }
        }
        (None, min_distance)
    }
}
