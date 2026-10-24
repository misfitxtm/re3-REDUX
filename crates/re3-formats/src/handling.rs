//! `handling.cfg`. Values are the raw file numbers; the original's unit conversion
//! (`ConvertDataToGameUnits`) belongs with the vehicle code.
use crate::{datafile::Game, scan::{nums, Tok}};

#[derive(Debug, Clone, PartialEq)]
pub struct CarHandling {
    pub name: String,
    pub mass: f32,
    pub dimension: [f32; 3],
    pub centre_of_mass: [f32; 3],
    pub percent_submerged: i32,
    pub traction_multiplier: f32,
    pub traction_loss: f32,
    pub traction_bias: f32,
    pub gears: i32,
    pub max_velocity: f32,
    /// Kept as a double: the original multiplies the parsed double by 0.4 before narrowing.
    pub engine_acceleration: f64,
    pub drive_type: char,
    pub engine_type: char,
    pub brake_deceleration: f32,
    pub brake_bias: f32,
    pub abs: bool,
    pub steering_lock: f32,
    pub suspension_force: f32,
    pub suspension_damping: f32,
    pub seat_offset: f32,
    pub collision_damage_multiplier: f32,
    pub monetary_value: i32,
    pub suspension_upper: f32,
    pub suspension_lower: f32,
    pub suspension_bias: f32,
    /// Vice City only.
    pub suspension_antidive: Option<f32>,
    pub flags: u32,
    pub front_lights: i32,
    pub rear_lights: i32,
}

/// Extra per-vehicle rows (Vice City): name plus raw numbers in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Extra {
    pub name: String,
    pub values: Vec<f32>,
}

#[derive(Debug, Default)]
pub struct Handling {
    pub cars: Vec<CarHandling>,
    /// `!` rows: lean fwd COM/force, lean back COM/force, max lean, full anim lean, des lean, speed steer,
    /// slip steer, no-player COM z, wheelie ang, stoppie ang, wheelie steer, wheelie/stoppie stab mult (15).
    pub bikes: Vec<Extra>,
    /// `$` rows: thrust, thrust falloff, yaw, yaw stab, side slip, roll, roll stab, pitch, pitch stab,
    /// form lift, attack lift, move res, turn res xyz, speed res xyz (18).
    pub flying: Vec<Extra>,
    /// `%` rows: thrust y/z/app z, aqua-plane force/limit/offset, wave audio mult, move res xyz,
    /// turn res xyz, look cam height (14).
    pub boats: Vec<Extra>,
    pub rejected: Vec<String>,
}

fn car(l: &str, game: Game) -> Option<CarHandling> {
    let mut t = Tok::new(l);
    let name = t.s()?;
    let mass = t.n()?;
    let dimension = [t.n()?, t.n()?, t.n()?];
    let centre_of_mass = [t.n()?, t.n()?, t.n()?];
    Some(CarHandling {
        name,
        mass,
        dimension,
        centre_of_mass,
        percent_submerged: t.n()?,
        traction_multiplier: t.n()?,
        traction_loss: t.n()?,
        traction_bias: t.n()?,
        gears: t.n()?,
        max_velocity: t.n()?,
        engine_acceleration: t.n()?,
        drive_type: t.s()?.chars().next()?,
        engine_type: t.s()?.chars().next()?,
        brake_deceleration: t.n()?,
        brake_bias: t.n()?,
        abs: t.n::<i32>()? != 0,
        steering_lock: t.n()?,
        suspension_force: t.n()?,
        suspension_damping: t.n()?,
        seat_offset: t.n()?,
        collision_damage_multiplier: t.n()?,
        monetary_value: t.n()?,
        suspension_upper: t.n()?,
        suspension_lower: t.n()?,
        suspension_bias: t.n()?,
        suspension_antidive: if game == Game::Vc { Some(t.n()?) } else { None },
        flags: t.hex()?,
        front_lights: t.n()?,
        rear_lights: t.n()?,
    })
}

/// Parses until the `;the end` line, as the original does.
pub fn parse_handling(text: &str, game: Game) -> Handling {
    let mut h = Handling::default();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line == ";the end" {
            break;
        }
        if line.starts_with(';') || line.trim().is_empty() {
            continue;
        }
        let extra = |n: usize| {
            let mut t = Tok::new(line);
            t.s()?; // the `!`/`$`/`%` marker
            Some(Extra { name: t.s()?, values: nums(&mut t, n)? })
        };
        let ok = match line.chars().next() {
            Some('!') if game == Game::Vc => extra(15).map(|e| h.bikes.push(e)).is_some(),
            Some('$') if game == Game::Vc => extra(18).map(|e| h.flying.push(e)).is_some(),
            Some('%') if game == Game::Vc => extra(14).map(|e| h.boats.push(e)).is_some(),
            _ => car(line, game).map(|c| h.cars.push(c)).is_some(),
        };
        if !ok {
            h.rejected.push(line.to_owned());
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const GTA3: &str = "; c\nLANDSTAL 1700.0 2.0 5.0 2.0 0.0 0.0 0.25 85 0.85 0.85 0.51 5 160.0 20.0 4 D 6.2 0.60 0 35.0 2.0 0.1 0.27 0.35 25000 0.25 -0.1 0.5 C00B 0 1\n;the end\nIGNORED\n";

    #[test]
    fn gta3_row() {
        let h = parse_handling(GTA3, Game::Gta3);
        assert_eq!(h.cars.len(), 1);
        let c = &h.cars[0];
        assert_eq!((c.name.as_str(), c.gears, c.drive_type, c.flags), ("LANDSTAL", 5, '4', 0xC00B));
        assert_eq!(c.suspension_antidive, None);
        assert!(h.rejected.is_empty());
    }

    #[test]
    fn vc_rows() {
        let h = parse_handling("%\tDINGHY 1 2 3 4 5 6 7 8 9 10 11 12 13 14\nBAD ROW\n;the end\n", Game::Vc);
        assert_eq!(h.boats[0].values.len(), 14);
        assert_eq!(h.rejected, vec!["BAD ROW".to_string()]);
    }
}
