//! World data files: `object.dat`, `surface.dat`, `water.dat`/`waterpro.dat`, `timecyc.dat`,
//! train/flight track files and `particle.cfg`.
use crate::{datafile::Game, scan::{nums, Tok}};

#[derive(Debug, PartialEq)]
pub struct ObjectInfo {
    pub name: String,
    pub mass: f32,
    pub turn_mass: f32,
    pub air_resistance: f32,
    pub elasticity: f32,
    pub percent_submerged: f32,
    pub uproot_limit: f32,
    pub collision_damage_multiplier: f32,
    pub damage_effect: i32,
    pub response_case: i32,
    pub camera_avoid: i32,
}

/// `object.dat`: `;` comment lines, commas are blanks, and a `*` ends the file.
pub fn parse_object_dat(text: &str) -> Vec<ObjectInfo> {
    let mut out = vec![];
    for l in text.lines() {
        let l = l.trim_start();
        if l.starts_with(';') {
            continue;
        }
        let end = l.find('*');
        let l = l[..end.unwrap_or(l.len())].replace(',', " ");
        let mut t = Tok::new(&l);
        let o = (|| {
            Some(ObjectInfo {
                name: t.s()?,
                mass: t.n()?,
                turn_mass: t.n()?,
                air_resistance: t.n()?,
                elasticity: t.n()?,
                percent_submerged: t.n()?,
                uproot_limit: t.n()?,
                collision_damage_multiplier: t.n()?,
                damage_effect: t.n()?,
                response_case: t.n()?,
                camera_avoid: t.n()?,
            })
        })();
        out.extend(o);
        if end.is_some() {
            break;
        }
    }
    out
}

#[derive(Debug, PartialEq)]
pub struct Surfaces {
    pub names: Vec<String>,
    /// Symmetric adhesion-limit matrix; a `-` in the file means 0.
    pub limits: Vec<Vec<f32>>,
}

pub fn parse_surface_dat(text: &str) -> Surfaces {
    let rows: Vec<(String, Vec<f32>)> = text
        .lines()
        .map(|l| l.replace(',', " "))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with(';'))
        .map(|l| {
            let mut t = Tok::new(&l);
            let name = t.s().unwrap();
            (name, t.rest().iter().map(|w| if w.starts_with('-') { 0.0 } else { w.parse().unwrap_or(0.0) }).collect())
        })
        .collect();
    let n = rows.len();
    let mut limits = vec![vec![0.0; n]; n];
    for (i, (_, vals)) in rows.iter().enumerate() {
        for (j, &v) in vals.iter().take(i + 1).enumerate() {
            limits[i][j] = v;
            limits[j][i] = v;
        }
    }
    Surfaces { names: rows.into_iter().map(|r| r.0).collect(), limits }
}

#[derive(Debug, PartialEq)]
pub struct WaterRect {
    pub z: f32,
    /// left, bottom, right, top
    pub rect: [f32; 4],
}

/// `water.dat`: `z, l, b, r, t` rows; `;` comments and the `* ;end of file` marker are skipped.
pub fn parse_water_dat(text: &str) -> Vec<WaterRect> {
    text.lines()
        .map(|l| l.replace(',', " "))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with(';') && !l.contains("* ;end of file"))
        .filter_map(|l| {
            let v = nums::<f32>(&mut Tok::new(&l), 5)?;
            Some(WaterRect { z: v[0], rect: [v[1], v[2], v[3], v[4]] })
        })
        .collect()
}

pub const WATER_LEVELS: usize = 48;
pub const WATER_LARGE: usize = 64;
pub const WATER_SMALL: usize = 128;

/// `waterpro.dat`: the pre-rasterised water table the game writes after reading `water.dat`.
#[derive(Debug, PartialEq)]
pub struct WaterPro {
    pub count: i32,
    pub zs: [f32; WATER_LEVELS],
    /// `CRect` order: left, y-max, right, y-min (so the 2nd/4th numbers are swapped vs `water.dat`).
    pub rects: [[f32; 4]; WATER_LEVELS],
    /// -128 means no water.
    pub large: Vec<i8>,
    pub small: Vec<i8>,
}

pub const WATERPRO_SIZE: usize = 4 + WATER_LEVELS * 4 + WATER_LEVELS * 16 + WATER_LARGE * WATER_LARGE + WATER_SMALL * WATER_SMALL;

pub fn parse_waterpro(b: &[u8]) -> Option<WaterPro> {
    if b.len() != WATERPRO_SIZE {
        return None;
    }
    let f = |at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let mut zs = [0.0; WATER_LEVELS];
    let mut rects = [[0.0; 4]; WATER_LEVELS];
    for (i, (z, rect)) in zs.iter_mut().zip(rects.iter_mut()).enumerate() {
        *z = f(4 + i * 4);
        for (j, v) in rect.iter_mut().enumerate() {
            *v = f(4 + WATER_LEVELS * 4 + i * 16 + j * 4);
        }
    }
    let big = 4 + WATER_LEVELS * 20;
    let small = big + WATER_LARGE * WATER_LARGE;
    Some(WaterPro {
        count: i32::from_le_bytes(b[..4].try_into().unwrap()),
        zs,
        rects,
        large: b[big..small].iter().map(|&x| x as i8).collect(),
        small: b[small..].iter().map(|&x| x as i8).collect(),
    })
}

/// Train / flight track files: a node count, then one `x y z` line per node.
pub fn parse_track(text: &str) -> Option<Vec<[f32; 3]>> {
    let mut lines = text.lines();
    let n: usize = lines.next()?.split_whitespace().next()?.parse().ok()?;
    let nodes: Vec<[f32; 3]> = lines.take(n).map(|l| nums(&mut Tok::new(l), 3).map(|v: Vec<f32>| [v[0], v[1], v[2]])).collect::<Option<_>>()?;
    (nodes.len() == n).then_some(nodes)
}

pub const HOURS: usize = 24;

/// `timecyc.dat` rows in file order: weather-major, 24 hours each. A row holds 40 numbers
/// (GTA III) or 52 (Vice City: four ambient colours and a pole-shadow strength).
pub fn parse_timecyc(text: &str, game: Game) -> Result<Vec<Vec<f32>>, String> {
    let want = if game == Game::Vc { 52 } else { 40 };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('/'))
        .map(|l| nums(&mut Tok::new(l), want).ok_or_else(|| format!("timecyc row without {want} numbers: {l}")))
        .collect()
}

/// `particle.cfg` rows: name plus the remaining columns as numbers. Ends at `;the end`.
pub fn parse_particle_cfg(text: &str) -> Vec<(String, Vec<f32>)> {
    let mut out = vec![];
    for l in text.lines() {
        if l.trim_end() == ";the end" {
            break;
        }
        if l.starts_with(';') || l.trim().is_empty() {
            continue;
        }
        let mut t = Tok::new(l);
        let name = t.s().unwrap();
        out.push((name, t.rest().iter().filter_map(|w| w.parse().ok()).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_dat() {
        let o = parse_object_dat("; c\nmagnet 99999.0, 99999.0 0.99, 0.1, 50.0, 125.0, 1.0, 0, 0, 0\n* end\nignored 1 2\n");
        assert_eq!(o.len(), 1);
        assert_eq!((o[0].name.as_str(), o[0].mass, o[0].camera_avoid), ("magnet", 99999.0, 0));
    }

    #[test]
    fn surfaces() {
        let s = parse_surface_dat("; c\nRubber 6.0\nHard 3.6 2.0\nRoad 4.5 - 6.0\n");
        assert_eq!(s.names, ["Rubber", "Hard", "Road"]);
        assert_eq!(s.limits[0][1], 3.6);
        assert_eq!(s.limits[2][1], 0.0);
        assert_eq!(s.limits[1][2], 0.0);
    }

    #[test]
    fn water_and_tracks() {
        let w = parse_water_dat("6.0, -3000.0, -3000.0, 3000.0, 3000.0,\n* ;end of file\n");
        assert_eq!(w, vec![WaterRect { z: 6.0, rect: [-3000.0, -3000.0, 3000.0, 3000.0] }]);
        assert_eq!(parse_track("2\n1 2 3\n4 5 6\n").unwrap()[1], [4.0, 5.0, 6.0]);
        assert!(parse_track("3\n1 2 3\n").is_none());
        assert!(parse_waterpro(&[0; 10]).is_none());
    }

    #[test]
    fn timecyc_rows() {
        let row = vec!["1"; 40].join(" ");
        assert_eq!(parse_timecyc(&format!("// c\n{row}\n"), Game::Gta3).unwrap().len(), 1);
        assert!(parse_timecyc(&row, Game::Vc).is_err());
    }
}
