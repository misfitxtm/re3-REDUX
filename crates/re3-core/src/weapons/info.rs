//! `weapon.dat` table (`CWeaponInfo`). GTA III layout; VC adds fields later.
use crate::anim::tables_gta3;
use glam::Vec3;
use re3_formats::{combat::parse_weapons, datafile::Game};

pub mod weapon_type {
    pub const UNARMED: usize = 0;
    pub const BASEBALLBAT: usize = 1;
    pub const COLT45: usize = 2;
    pub const UZI: usize = 3;
    pub const SHOTGUN: usize = 4;
    pub const AK47: usize = 5;
    pub const M16: usize = 6;
    pub const SNIPERRIFLE: usize = 7;
    pub const ROCKETLAUNCHER: usize = 8;
    pub const FLAMETHROWER: usize = 9;
    pub const MOLOTOV: usize = 10;
    pub const GRENADE: usize = 11;
    pub const DETONATOR: usize = 12;
    pub const HELICANNON: usize = 13;
    pub const LAST: usize = 14;
    pub const ARMOUR: usize = 15;
    pub const RAMMEDBYCAR: usize = 16;
    pub const RUNOVERBYCAR: usize = 17;
    pub const EXPLOSION: usize = 18;
    pub const UZI_DRIVEBY: usize = 19;
    pub const DROWNING: usize = 20;
    pub const FALL: usize = 21;
    pub const UNIDENTIFIED: usize = 22;
    pub const TOTAL: usize = LAST;
    pub const TOTAL_INVENTORY: usize = 13;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponFire {
    Melee,
    InstantHit,
    Projectile,
    AreaEffect,
    Use,
}

pub mod flag {
    pub const USE_GRAVITY: u32 = 1;
    pub const SLOWS_DOWN: u32 = 1 << 1;
    pub const DISSIPATES: u32 = 1 << 2;
    pub const RAND_SPEED: u32 = 1 << 3;
    pub const EXPANDS: u32 = 1 << 4;
    pub const EXPLODES: u32 = 1 << 5;
    pub const CANAIM: u32 = 1 << 6;
    pub const CANAIM_WITHARM: u32 = 1 << 7;
    pub const FIRST_PERSON: u32 = 1 << 8;
    pub const HEAVY: u32 = 1 << 9;
    pub const THROW: u32 = 1 << 10;
}

const NAMES: [&str; weapon_type::TOTAL] = [
    "Unarmed", "BaseballBat", "Colt45", "Uzi", "Shotgun", "AK47", "M16", "SniperRifle", "RocketLauncher", "FlameThrower", "Molotov", "Grenade",
    "Detonator", "HeliCannon",
];

#[derive(Clone, Debug)]
pub struct WeaponInfo {
    pub fire: WeaponFire,
    pub range: f32,
    pub firing_rate: u32,
    pub reload: u32,
    pub ammo: i32,
    pub damage: u32,
    pub speed: f32,
    pub radius: f32,
    pub lifespan: f32,
    pub spread: f32,
    pub fire_offset: Vec3,
    pub anim: u32,
    pub anim2: u32,
    pub anim_loop_start: f32,
    pub anim_loop_end: f32,
    pub anim_frame_fire: f32,
    pub anim2_frame_fire: f32,
    pub model_id: i32,
    pub flags: u32,
}

impl WeaponInfo {
    pub fn is_flag_set(&self, f: u32) -> bool {
        self.flags & f != 0
    }
}

impl Default for WeaponInfo {
    fn default() -> Self {
        Self {
            fire: WeaponFire::InstantHit,
            range: 0.0,
            firing_rate: 0,
            reload: 0,
            ammo: 0,
            damage: 0,
            speed: 0.0,
            radius: 0.0,
            lifespan: 0.0,
            spread: 0.0,
            fire_offset: Vec3::default(),
            anim: tables_gta3::anim::STD_PUNCH,
            anim2: tables_gta3::anim::STD_NUM,
            anim_loop_start: 0.0,
            anim_loop_end: 0.0,
            anim_frame_fire: 0.0,
            anim2_frame_fire: 0.0,
            model_id: 0,
            flags: flag::USE_GRAVITY | flag::SLOWS_DOWN | flag::RAND_SPEED | flag::EXPANDS | flag::EXPLODES,
        }
    }
}

/// Id of a standard-group animation by name.
fn std_anim_id(name: &str) -> Option<u32> {
    let g = tables_gta3::GROUPS.iter().find(|g| g.name == "man")?;
    let i = g.anims.iter().position(|a| a.eq_ignore_ascii_case(name))?;
    Some(g.descs[i].0)
}

#[derive(Clone, Debug)]
pub struct WeaponInfos(pub [WeaponInfo; weapon_type::TOTAL]);

impl Default for WeaponInfos {
    fn default() -> Self {
        Self(std::array::from_fn(|_| WeaponInfo::default()))
    }
}

impl WeaponInfos {
    pub fn get(&self, t: usize) -> &WeaponInfo {
        &self.0[t]
    }

    /// Loads GTA III `weapon.dat`. Unknown names fall back to unarmed, like the original.
    pub fn from_dat(text: &str) -> Self {
        let mut out = Self::default();
        for w in parse_weapons(text, Game::Gta3).0 {
            let t = NAMES.iter().position(|n| *n == w.name).unwrap_or(weapon_type::UNARMED);
            let i = &mut out.0[t];
            i.fire = match w.fire_type.as_str() {
                "MELEE" => WeaponFire::Melee,
                "PROJECTILE" => WeaponFire::Projectile,
                "AREA_EFFECT" => WeaponFire::AreaEffect,
                _ => WeaponFire::InstantHit,
            };
            i.range = w.range;
            i.firing_rate = w.firing_rate as u32;
            i.reload = w.reload as u32;
            i.ammo = w.ammo;
            i.damage = w.damage as u32;
            i.speed = w.speed;
            i.radius = w.radius;
            i.lifespan = w.lifespan;
            i.spread = w.spread;
            i.fire_offset = Vec3::new(w.fire_offset[0], w.fire_offset[1], w.fire_offset[2]);
            i.anim = std_anim_id(&w.anim).unwrap_or(0);
            if let Some(a2) = w.anim2.as_deref().filter(|a| *a != "null") {
                i.anim2 = std_anim_id(a2).unwrap_or(i.anim2);
            }
            i.anim_loop_start = w.anim_loop_start / 30.0;
            i.anim_loop_end = w.anim_loop_end / 30.0;
            i.anim_frame_fire = w.anim_frame_fire / 30.0;
            i.anim2_frame_fire = w.anim2_frame_fire / 30.0;
            i.model_id = w.model_id;
            i.flags = w.flags;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_weapon_dat() {
        let Ok(dir) = std::env::var("GTA3_DIR") else { return };
        let text = std::fs::read_to_string(format!("{dir}/data/weapon.dat")).or_else(|_| std::fs::read_to_string(format!("{dir}/data/Weapon.dat"))).unwrap();
        let w = WeaponInfos::from_dat(&text);
        let colt = w.get(weapon_type::COLT45);
        assert_eq!(colt.fire, WeaponFire::InstantHit);
        assert!(colt.range > 0.0 && colt.damage > 0);
        assert_ne!(colt.anim, tables_gta3::anim::STD_PUNCH);
        assert_eq!(w.get(weapon_type::ROCKETLAUNCHER).fire, WeaponFire::Projectile);
        assert_eq!(w.get(weapon_type::FLAMETHROWER).fire, WeaponFire::AreaEffect);
    }
}
