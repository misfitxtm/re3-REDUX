//! `weapon.dat` and `fistfite.dat`. Frame counts are raw (the loader divides them by 30).
use crate::{datafile::Game, scan::Tok};

#[derive(Debug, PartialEq)]
pub struct Weapon {
    pub name: String,
    pub fire_type: String,
    pub range: f32,
    pub firing_rate: i32,
    pub reload: i32,
    pub ammo: i32,
    pub damage: i32,
    pub speed: f32,
    pub radius: f32,
    pub lifespan: f32,
    pub spread: f32,
    pub fire_offset: [f32; 3],
    pub anim: String,
    pub anim_loop_start: f32,
    pub anim_loop_end: f32,
    pub anim_frame_fire: f32,
    pub model_id: i32,
    pub flags: u32,
    /// GTA III only: second animation name (`null` for none).
    pub anim2: Option<String>,
    /// GTA III: delay before anim 2 fires. Vice City: same, after the anim-2 loop fields.
    pub anim2_frame_fire: f32,
    /// Vice City only: anim-2 loop start/end, anim breakout, second model id, weapon slot.
    pub vc: Option<VcWeapon>,
}

#[derive(Debug, PartialEq)]
pub struct VcWeapon {
    pub anim2_loop_start: f32,
    pub anim2_loop_end: f32,
    pub anim_breakout: f32,
    pub model_id2: i32,
    pub slot: i32,
}

fn weapon(l: &str, game: Game) -> Option<Weapon> {
    let mut t = Tok::new(l);
    let (name, fire_type) = (t.s()?, t.s()?);
    let (range, firing_rate, reload, ammo, damage) = (t.n()?, t.n()?, t.n()?, t.n()?, t.n()?);
    let (speed, radius, lifespan, spread) = (t.n()?, t.n()?, t.n()?, t.n()?);
    let fire_offset = [t.n()?, t.n()?, t.n()?];
    let anim = t.s()?;
    let mut w = Weapon {
        name, fire_type, range, firing_rate, reload, ammo, damage, speed, radius, lifespan, spread, fire_offset, anim,
        anim_loop_start: 0.0, anim_loop_end: 0.0, anim_frame_fire: 0.0, model_id: 0, flags: 0, anim2: None, anim2_frame_fire: 0.0, vc: None,
    };
    match game {
        Game::Gta3 => {
            w.anim2 = Some(t.s()?);
            (w.anim_loop_start, w.anim_loop_end, w.anim_frame_fire, w.anim2_frame_fire) = (t.n()?, t.n()?, t.n()?, t.n()?);
            w.model_id = t.n()?;
            w.flags = t.n()?;
        }
        Game::Vc => {
            (w.anim_loop_start, w.anim_loop_end, w.anim_frame_fire) = (t.n()?, t.n()?, t.n()?);
            let (anim2_loop_start, anim2_loop_end, anim2_fire) = (t.n()?, t.n()?, t.n()?);
            w.anim2_frame_fire = anim2_fire;
            let anim_breakout = t.n()?;
            w.model_id = t.n()?;
            let model_id2 = t.n()?;
            w.flags = t.hex()?;
            w.vc = Some(VcWeapon { anim2_loop_start, anim2_loop_end, anim_breakout, model_id2, slot: t.n()? });
        }
    }
    Some(w)
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Parses until an `ENDWEAPONDATA` line. Returns the weapons and any lines that did not parse.
pub fn parse_weapons(text: &str, game: Game) -> (Vec<Weapon>, Vec<String>) {
    let (mut ok, mut bad) = (vec![], vec![]);
    for l in lines(text) {
        if l.starts_with("ENDWEAPONDATA") {
            break;
        }
        match weapon(l, game) {
            Some(w) => ok.push(w),
            None => bad.push(l.to_owned()),
        }
    }
    (ok, bad)
}

#[derive(Debug, PartialEq)]
pub struct FightMove {
    pub name: String,
    pub start_fire: f32,
    pub end_fire: f32,
    pub combo_follow_on: f32,
    pub strike_radius: f32,
    /// Vice City only.
    pub extend_reach: Option<f32>,
    /// `G`round, `H`igh, `L`ow, `M`edium or `N`ull.
    pub hit_level: char,
    pub anim: String,
    pub damage: i32,
    pub flags: i32,
}

pub fn parse_fight_moves(text: &str, game: Game) -> (Vec<FightMove>, Vec<String>) {
    let (mut ok, mut bad) = (vec![], vec![]);
    for l in lines(text) {
        if l.starts_with("ENDWEAPONDATA") {
            break;
        }
        let mut t = Tok::new(l);
        let m = (|| {
            Some(FightMove {
                name: t.s()?,
                start_fire: t.n()?,
                end_fire: t.n()?,
                combo_follow_on: t.n()?,
                strike_radius: t.n()?,
                extend_reach: if game == Game::Vc { Some(t.n()?) } else { None },
                hit_level: t.s()?.chars().next()?,
                anim: t.s()?,
                damage: t.n()?,
                flags: t.n()?,
            })
        })();
        match m {
            Some(m) => ok.push(m),
            None => bad.push(l.to_owned()),
        }
    }
    (ok, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gta3_weapon_and_move() {
        let (w, bad) = parse_weapons("Colt45 INSTANT_HIT 30.0 250 350 12 25 -1.0 -1.0 -1.0 -1.0 0.30 0.0 0.09 WEAPON_hgun_body RBLOCK_Cshoot 8 15 9 9 173 192\nENDWEAPONDATA\nx\n", Game::Gta3);
        assert!(bad.is_empty());
        assert_eq!((w[0].damage, w[0].anim2.as_deref(), w[0].flags), (25, Some("RBLOCK_Cshoot"), 192));
        let (m, _) = parse_fight_moves("StdPunch 5 8 0 0.4 M punchR 1 1\n", Game::Gta3);
        assert_eq!((m[0].hit_level, m[0].extend_reach), ('M', None));
    }

    #[test]
    fn vc_weapon() {
        let (w, bad) = parse_weapons("Unarmed MELEE 2.4 250 100 1000 8 -1.0 0.6 -1.0 -1.0 0.1 0.65 0.30 unarmed 0 99 6 0 99 12 99 -1 -1 102000 0\n", Game::Vc);
        assert!(bad.is_empty());
        assert_eq!(w[0].flags, 0x102000);
        assert_eq!(w[0].vc.as_ref().unwrap().anim_breakout, 99.0);
    }
}
