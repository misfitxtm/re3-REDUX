//! The player's wanted level as the ped code sees it (`FindPlayerPed()->m_pWanted`).
use super::ctx::{PedHooks, Px, WantedInfo};
use crate::control::wanted::{Wanted, WantedEnv};
use crate::entity::EntityRef;
use crate::world::World;
use glam::Vec3;

/// Answers a [`Wanted`]'s questions from the ped hooks.
struct HookEnv<'a>(&'a mut dyn PedHooks);

impl WantedEnv for HookEnv<'_> {
    fn frenzy_on_going(&mut self) -> bool {
        self.0.frenzy_on_going()
    }

    fn wanted_level_drop(&mut self) -> i32 {
        self.0.wanted_level_drop()
    }

    fn report_crime(&mut self, ty: u8, pos: Vec3) {
        self.0.report_crime(ty, pos);
    }
}

impl World {
    /// `FindPlayerPed()->m_pWanted`, if there is a player.
    pub fn player_wanted(&self) -> Option<&Wanted> {
        let p = self.try_ped(self.ents.player_ped?)?;
        p.player.as_ref().map(|d| &d.wanted)
    }

    pub fn player_wanted_mut(&mut self) -> Option<&mut Wanted> {
        let r = self.ents.player_ped?;
        match &mut self.get_mut(r)?.kind {
            crate::entity::Kind::Ped(p) => p.player.as_mut().map(|d| &mut d.wanted),
            _ => None,
        }
    }

    /// The facts about the wanted level the ped code asks for.
    pub fn wanted_info(&self) -> WantedInfo {
        self.player_wanted().map_or(WantedInfo::default(), |w| WantedInfo {
            current_cops: w.current_cops as i32,
            level: w.level,
            ignored_by_cops: w.ignored_by_cops,
            ignored_by_everyone: w.ignored_by_everyone,
        })
    }

    /// `FindPlayerPed()->SetWantedLevelNoDrop`.
    pub fn set_wanted_level_no_drop(&mut self, level: i32) {
        let now = self.now();
        if let Some(w) = self.player_wanted_mut() {
            w.set_wanted_level_no_drop(level, now);
        }
    }

    /// `FindPlayerPed()->m_pWanted->RegisterCrime_Immediately(type, pos, (uintptr)ped, false)`.
    pub fn register_crime_immediately(&mut self, px: &mut Px, ty: u8, pos: Vec3, by: EntityRef) {
        let now = self.now();
        if let Some(w) = self.player_wanted_mut() {
            w.register_crime_immediately(&mut HookEnv(px.hooks), ty, pos, by.handle, false, now);
        }
    }

    /// `CWanted::Update` for the player's wanted level.
    pub fn update_player_wanted(&mut self, px: &mut Px) {
        let now = self.now();
        let Some(player) = self.ents.player_ped else { return };
        let Some(last) = self.player_wanted().map(|w| (w.last_update_time, w.level)) else { return };
        if now.wrapping_sub(last.0) <= 1000 {
            return;
        }
        let presence = if last.1 > 1 {
            None
        } else {
            let at = self.player_vehicle().map_or_else(|| self.epos(player), |v| self.epos(v));
            Some(self.work_out_police_presence(at, 18.0, self.player_vehicle()))
        };
        let Some(w) = self.player_wanted_mut() else { return };
        match presence {
            None => w.last_update_time = now,
            Some(0) => {
                w.last_update_time = now;
                w.chaos = (w.chaos - 1).max(0);
                w.update_wanted_level(now);
            }
            Some(_) => {}
        }
        w.update_crimes_q(&mut HookEnv(px.hooks), now);
        w.tidy_cops();
    }
}
