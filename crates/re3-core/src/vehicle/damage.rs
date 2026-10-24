//! `CDamageManager`: door, panel, wheel, light and engine damage of a car.
use crate::general::Rng;

pub mod engine {
    pub const STEAM1: u8 = 100;
    pub const STEAM2: u8 = 150;
    pub const SMOKE: u8 = 200;
    pub const ON_FIRE: u8 = 225;
}

pub mod door_status {
    pub const OK: u8 = 0;
    pub const SMASHED: u8 = 1;
    pub const SWINGING: u8 = 2;
    pub const MISSING: u8 = 3;
}

pub mod wheel_status {
    pub const OK: u8 = 0;
    pub const BURST: u8 = 1;
    pub const MISSING: u8 = 2;
}

/// `tComponent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Component {
    Default,
    WheelFrontLeft,
    WheelFrontRight,
    WheelRearLeft,
    WheelRearRight,
    DoorBonnet,
    DoorBoot,
    DoorFrontLeft,
    DoorFrontRight,
    DoorRearLeft,
    DoorRearRight,
    PanelFrontLeft,
    PanelFrontRight,
    PanelRearLeft,
    PanelRearRight,
    PanelWindscreen,
    BumperFront,
    BumperRear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Bumper,
    Wheel,
    Door,
    Bonnet,
    Boot,
    Panel,
    Default,
}

/// Panel indices (`ePanels`).
pub const BUMPER_FRONT: u8 = 5;
pub const BUMPER_REAR: u8 = 6;

/// Damage multiplier per component group, in `Group` order.
const GROUP_DAMAGE: [f32; 7] = [2.5, 1.25, 3.2, 1.4, 2.5, 2.8, 0.5];

fn ldb(pos: u32, size: u32, w: u32) -> u32 {
    w >> pos & ((1 << size) - 1)
}

fn dpb(b: u32, pos: u32, size: u32, w: u32) -> u32 {
    let m = ((1 << size) - 1) << pos;
    (w & !m) | ((b << pos) & m)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageManager {
    pub wheel_damage_effect: f32,
    pub engine_status: u8,
    pub wheel_status: [u8; 4],
    pub door_status: [u8; 6],
    /// Two bits per light.
    pub light_status: u32,
    /// Four bits per panel.
    pub panel_status: u32,
    pub field_18: u8,
}

impl Default for DamageManager {
    fn default() -> Self {
        Self { wheel_damage_effect: 0.75, engine_status: 0, wheel_status: [0; 4], door_status: [0; 6], light_status: 0, panel_status: 0, field_18: 1 }
    }
}

impl DamageManager {
    /// `ResetDamageStatus`: the original zeroes everything, including the wheel effect.
    pub fn reset(&mut self) {
        *self = Self { wheel_damage_effect: 0.0, field_18: 0, ..Default::default() };
    }

    /// `FuckCarCompletely` (`FIX_BUGS`: the bumpers are damaged, lights and panels keep their state).
    pub fn wreck(&mut self, rng: &mut Rng) {
        self.wheel_status[0] = wheel_status::MISSING;
        self.door_status = [door_status::MISSING; 6];
        for _ in 0..3 {
            self.progress_panel_damage(BUMPER_FRONT);
            self.progress_panel_damage(BUMPER_REAR);
        }
        let _ = rng;
        self.set_engine_status(250);
    }

    /// `GetComponentGroup`: the group and the index within it.
    pub fn component_group(c: Component) -> (Group, u8) {
        let n = c as u8;
        let p = Component::PanelFrontLeft as u8;
        let d = Component::DoorBonnet as u8;
        let w = Component::WheelFrontLeft as u8;
        if n >= p {
            (if c >= Component::BumperFront { Group::Bumper } else { Group::Panel }, n - p)
        } else if n >= d {
            (match c { Component::DoorBonnet => Group::Bonnet, Component::DoorBoot => Group::Boot, _ => Group::Door }, n - d)
        } else if n >= w {
            (Group::Wheel, n - w)
        } else {
            (Group::Default, 0)
        }
    }

    /// `ApplyDamage`: true if the hit was strong enough to matter.
    pub fn apply_damage(&mut self, component: Component, damage: f32, rng: &mut Rng) -> bool {
        let (group, sub) = Self::component_group(component);
        let damage = damage * GROUP_DAMAGE[group as usize];
        if damage <= 150.0 {
            return false;
        }
        match group {
            Group::Wheel => {
                self.progress_wheel_damage(sub);
            }
            Group::Door | Group::Boot => {
                self.progress_door_damage(sub);
            }
            Group::Bonnet => {
                if damage > 220.0 {
                    self.progress_engine_damage(rng);
                }
                self.progress_door_damage(sub);
            }
            Group::Panel | Group::Bumper => {
                // the windscreen counts as a light
                if group == Group::Panel {
                    self.set_light_status(sub, 1);
                }
                if damage > 220.0 && matches!(component, Component::PanelFrontLeft | Component::PanelFrontRight | Component::PanelWindscreen) {
                    self.progress_engine_damage(rng);
                }
                self.progress_panel_damage(sub);
            }
            Group::Default => {}
        }
        true
    }

    pub fn door_status(&self, door: usize) -> u8 {
        self.door_status[door]
    }

    pub fn set_door_status(&mut self, door: usize, status: u8) {
        self.door_status[door] = status;
    }

    pub fn progress_door_damage(&mut self, door: u8) -> bool {
        let s = self.door_status[door as usize];
        // the original compares with the panel constant, which has the same value
        if s == door_status::MISSING {
            return false;
        }
        self.door_status[door as usize] = s + 1;
        true
    }

    pub fn panel_status(&self, panel: u8) -> u8 {
        ldb(panel as u32 * 4, 4, self.panel_status) as u8
    }

    pub fn set_panel_status(&mut self, panel: u8, status: u8) {
        self.panel_status = dpb(status as u32, panel as u32 * 4, 4, self.panel_status);
    }

    pub fn progress_panel_damage(&mut self, panel: u8) -> bool {
        let s = self.panel_status(panel);
        if s == door_status::MISSING {
            return false;
        }
        self.set_panel_status(panel, s + 1);
        true
    }

    pub fn light_status(&self, light: u8) -> u8 {
        ldb(light as u32 * 2, 2, self.light_status) as u8
    }

    pub fn set_light_status(&mut self, light: u8, status: u8) {
        self.light_status = dpb(status as u32, light as u32 * 2, 2, self.light_status);
    }

    pub fn wheel_status(&self, wheel: usize) -> u8 {
        self.wheel_status[wheel]
    }

    pub fn set_wheel_status(&mut self, wheel: usize, status: u8) {
        self.wheel_status[wheel] = status;
    }

    pub fn progress_wheel_damage(&mut self, wheel: u8) -> bool {
        let s = self.wheel_status[wheel as usize];
        if s == wheel_status::MISSING {
            return false;
        }
        self.wheel_status[wheel as usize] = s + 1;
        true
    }

    /// Clamped at 250.
    pub fn set_engine_status(&mut self, status: u32) {
        self.engine_status = status.min(250) as u8;
    }

    pub fn progress_engine_damage(&mut self, rng: &mut Rng) -> bool {
        let status = self.engine_status as u32;
        let mut new = status + 32 + (rng.number() & 0x1F) as u32;
        if status < engine::ON_FIRE as u32 && new > engine::ON_FIRE as u32 - 1 {
            new = engine::ON_FIRE as u32 - 1;
        }
        self.set_engine_status(new);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_map_to_groups_and_indices() {
        use Component::*;
        assert_eq!(DamageManager::component_group(Default), (Group::Default, 0));
        assert_eq!(DamageManager::component_group(WheelRearRight), (Group::Wheel, 3));
        assert_eq!(DamageManager::component_group(DoorBonnet), (Group::Bonnet, 0));
        assert_eq!(DamageManager::component_group(DoorBoot), (Group::Boot, 1));
        assert_eq!(DamageManager::component_group(DoorRearRight), (Group::Door, 5));
        assert_eq!(DamageManager::component_group(PanelWindscreen), (Group::Panel, 4));
        assert_eq!(DamageManager::component_group(BumperRear), (Group::Bumper, 6));
    }

    #[test]
    fn status_bitfields_are_independent() {
        let mut d = DamageManager::default();
        d.set_panel_status(2, 3);
        d.set_panel_status(3, 1);
        assert_eq!((d.panel_status(2), d.panel_status(3), d.panel_status(1)), (3, 1, 0));
        d.set_light_status(1, 1);
        d.set_light_status(3, 3);
        assert_eq!((d.light_status(0), d.light_status(1), d.light_status(3)), (0, 1, 3));
        // damage stops at "missing"
        assert!(d.progress_panel_damage(0) && d.progress_panel_damage(0) && d.progress_panel_damage(0));
        assert!(!d.progress_panel_damage(0));
        assert_eq!(d.panel_status(0), 3);
    }

    #[test]
    fn weak_hits_do_nothing_and_strong_ones_progress_the_part() {
        let mut d = DamageManager::default();
        let mut rng = Rng::default();
        // doors take 3.2x: 40 -> 128 (no), 50 -> 160 (yes)
        assert!(!d.apply_damage(Component::DoorFrontLeft, 40.0, &mut rng));
        assert!(d.apply_damage(Component::DoorFrontLeft, 50.0, &mut rng));
        assert_eq!(d.door_status(2), door_status::SMASHED);
        // a hard hit to a front panel breaks its light and hurts the engine
        assert!(d.apply_damage(Component::PanelFrontLeft, 100.0, &mut rng));
        assert_eq!((d.light_status(0), d.panel_status(0)), (1, 1));
        assert!(d.engine_status >= 32);
    }

    #[test]
    fn engine_damage_stops_short_of_fire_and_caps_at_250() {
        let mut d = DamageManager::default();
        let mut rng = Rng::default();
        for _ in 0..20 {
            d.progress_engine_damage(&mut rng);
        }
        assert_eq!(d.engine_status, engine::ON_FIRE - 1);
        d.set_engine_status(999);
        assert_eq!(d.engine_status, 250);
        d.wreck(&mut rng);
        assert_eq!((d.wheel_status[0], d.door_status, d.panel_status(BUMPER_FRONT)), (2, [3; 6], 3));
    }
}
