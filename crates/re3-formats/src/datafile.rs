//! Text data files: `gta.dat` directives, IDE (object types) and IPL (placements).
//! Parsing follows `CFileLoader`; lines that the original would only half-read are rejected whole.
use crate::scan::Tok;

/// `CFileLoader::LoadLine`: control chars and commas become spaces, leading blanks are dropped.
pub fn clean_lines(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('\n').map(|l| {
        let l: String = l.chars().take(255).map(|c| if c < ' ' || c == ',' { ' ' } else { c }).collect();
        l.trim_start().to_owned()
    })
}

fn skip(l: &str) -> bool {
    l.is_empty() || l.starts_with('#')
}

#[derive(Debug, PartialEq)]
pub enum Directive {
    ImagePath(String),
    TexDiction(String),
    ColFile { level: i32, path: String },
    ModelFile(String),
    HierFile(String),
    Ide(String),
    Ipl(String),
    MapZone(String),
    CdImage(String),
}

/// Parses `gta.dat`; stops at `EXIT`, ignores unknown lines (as the original does).
pub fn parse_gta_dat(text: &str) -> Vec<Directive> {
    let mut out = vec![];
    for l in clean_lines(text).filter(|l| !skip(l)) {
        if l.starts_with("EXIT") {
            break;
        }
        // arg starts after "<KEYWORD> ", matching the original's fixed offsets
        let arg = |n: usize| l.get(n..).unwrap_or("").trim_end().to_owned();
        out.push(if l.starts_with("IMAGEPATH") {
            Directive::ImagePath(arg(10))
        } else if l.starts_with("TEXDICTION") {
            Directive::TexDiction(arg(11))
        } else if let Some(rest) = l.strip_prefix("COLFILE") {
            let mut t = rest.split_whitespace();
            let level = t.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            Directive::ColFile { level, path: t.collect::<Vec<_>>().join(" ") }
        } else if l.starts_with("MODELFILE") {
            Directive::ModelFile(arg(10))
        } else if l.starts_with("HIERFILE") {
            Directive::HierFile(arg(9))
        } else if l.starts_with("IDE") {
            Directive::Ide(arg(4))
        } else if l.starts_with("IPL") {
            Directive::Ipl(arg(4))
        } else if l.starts_with("MAPZONE") {
            Directive::MapZone(arg(8))
        } else if l.starts_with("CDIMAGE") {
            Directive::CdImage(arg(8))
        } else {
            continue;
        });
    }
    out
}

#[derive(Debug, PartialEq)]
pub struct Obj {
    pub id: i32,
    pub model: String,
    pub txd: String,
    pub dists: Vec<f32>,
    pub flags: u32,
    /// Index of the first damaged-model atomic (0 = none), derived from the distances.
    pub first_damaged: u8,
    pub time: Option<(i32, i32)>,
}

fn parse_obj(l: &str, timed: bool) -> Option<Obj> {
    let mut t = Tok::new(l);
    let (id, model, txd, num) = (t.n()?, t.s()?, t.s()?, t.n::<usize>()?);
    if !(1..=3).contains(&num) {
        return None;
    }
    let dists: Vec<f32> = (0..num).map(|_| t.n()).collect::<Option<_>>()?;
    let flags = t.n()?;
    let time = if timed { Some((t.n()?, t.n()?)) } else { None };
    let first_damaged = match dists[..] {
        [a, b] if a >= b => 1,
        [a, b, _] if a >= b => 1,
        [_, b, c] if b >= c => 2,
        _ => 0,
    };
    Some(Obj { id, model, txd, dists, flags, first_damaged, time })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Gta3,
    Vc,
}

#[derive(Debug, PartialEq)]
pub struct Weap {
    pub id: i32,
    pub model: String,
    pub txd: String,
    pub anim: String,
    pub dist: f32,
}

#[derive(Debug, PartialEq)]
pub struct Hier {
    pub id: i32,
    pub model: String,
    pub txd: String,
}

#[derive(Debug, PartialEq)]
pub struct Car {
    pub id: i32,
    pub model: String,
    pub txd: String,
    pub kind: String,
    pub handling: String,
    pub game_name: String,
    /// Vice City only.
    pub anim: Option<String>,
    pub class: String,
    pub frequency: u32,
    pub level: i32,
    pub comp_rules: u32,
    /// Wheel model / plane LOD id / bike steer angle; absent on boats, trains and helis.
    pub misc: Option<i32>,
    pub wheel_scale: Option<f32>,
}

#[derive(Debug, PartialEq)]
pub struct Ped {
    pub id: i32,
    pub model: String,
    pub txd: String,
    pub ped_type: String,
    pub stats: String,
    pub anim_group: String,
    pub cars_can_drive: u32,
    /// Vice City only: anim file and the two radio station ids.
    pub vc: Option<(String, i32, i32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Ped,
    Car,
    /// Vice City only.
    Water,
}

#[derive(Debug, PartialEq)]
pub struct PathNode {
    pub kind: i32,
    pub next: i32,
    pub cross: i32,
    pub pos: [f32; 3],
    pub width: f32,
    /// Car lanes (GTA III cars, and all Vice City nodes).
    pub num_left: Option<i32>,
    pub num_right: Option<i32>,
    /// Vice City only.
    pub speed: Option<i32>,
    pub flags: Option<i32>,
    pub spawn_rate: Option<f32>,
}

/// A path header followed by its 12 node lines.
#[derive(Debug, PartialEq)]
pub struct NavPath {
    pub kind: PathKind,
    pub id: i32,
    pub model: String,
    pub nodes: Vec<PathNode>,
}

pub const PATH_NODES: usize = 12;

fn path_header(l: &str, game: Game) -> Option<(PathKind, i32, String)> {
    let mut t = Tok::new(l);
    let kind = if game == Game::Vc {
        match t.n::<i32>()? {
            0 => PathKind::Ped,
            1 => PathKind::Car,
            2 => PathKind::Water,
            _ => return None,
        }
    } else {
        match t.s()?.as_str() {
            "ped" => PathKind::Ped,
            "car" => PathKind::Car,
            _ => return None,
        }
    };
    Some((kind, t.n()?, t.s().unwrap_or_default())) // detached paths (id -1) have no model name
}

fn path_node(l: &str, kind: PathKind, game: Game) -> Option<PathNode> {
    let mut t = Tok::new(l);
    let mut n = PathNode {
        kind: t.n()?, next: t.n()?, cross: t.n()?, pos: v3(&mut t)?, width: t.n()?,
        num_left: None, num_right: None, speed: None, flags: None, spawn_rate: None,
    };
    if game == Game::Vc {
        (n.num_left, n.num_right, n.speed, n.flags) = (Some(t.n()?), Some(t.n()?), Some(t.n()?), Some(t.n()?));
        n.spawn_rate = Some(t.n().unwrap_or(1.0));
    } else if kind == PathKind::Car {
        (n.num_left, n.num_right) = (Some(t.n()?), Some(t.n()?));
    }
    Some(n)
}

/// Feeds `path` section lines: a header, then 12 node lines. `Err` carries an unparsable line.
#[derive(Default)]
struct PathReader {
    cur: Option<NavPath>,
}

impl PathReader {
    fn line(&mut self, l: &str, game: Game, out: &mut Vec<NavPath>) -> bool {
        match &mut self.cur {
            None => match path_header(l, game) {
                Some((kind, id, model)) => {
                    self.cur = Some(NavPath { kind, id, model, nodes: vec![] });
                    true
                }
                None => false,
            },
            Some(p) => {
                let ok = path_node(l, p.kind, game).map(|n| p.nodes.push(n)).is_some();
                if p.nodes.len() == PATH_NODES {
                    out.extend(self.cur.take());
                }
                ok
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectKind {
    Light {
        corona: String,
        shadow: String,
        dist: f32,
        range: f32,
        size: f32,
        shadow_size: f32,
        shadow_intensity: i32,
        light_type: i32,
        road_reflection: i32,
        flare_type: i32,
        flags: i32,
    },
    Particle { particle_type: i32, dir: [f32; 3], scale: f32 },
    Attractor { kind: i32, dir: [f32; 3], probability: i32 },
    /// Vice City only.
    PedAttractor { kind: i32, queue_dir: [f32; 3], use_dir: [f32; 3] },
    /// Vice City only; the original reads no extra fields.
    SunGlare,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub model_id: i32,
    pub pos: [f32; 3],
    pub rgba: [i32; 4],
    pub kind: EffectKind,
}

fn parse_effect(l: &str, game: Game) -> Option<Effect> {
    let mut t = Tok::new(l);
    let (model_id, pos) = (t.n()?, v3(&mut t)?);
    let rgba = [t.n()?, t.n()?, t.n()?, t.n()?];
    let kind = match (t.n::<i32>()?, game) {
        (0, _) => {
            // the two texture names are quoted
            let q: Vec<&str> = l.splitn(5, '"').collect();
            if q.len() < 5 {
                return None;
            }
            let mut t = Tok::new(q[4]);
            EffectKind::Light {
                corona: q[1].to_owned(),
                shadow: q[3].to_owned(),
                dist: t.n()?, range: t.n()?, size: t.n()?, shadow_size: t.n()?,
                shadow_intensity: t.n()?, light_type: t.n()?, road_reflection: t.n()?, flare_type: t.n()?, flags: t.n()?,
            }
        }
        (1, _) => EffectKind::Particle { particle_type: t.n()?, dir: v3(&mut t)?, scale: t.n()? },
        (2, _) => EffectKind::Attractor { kind: t.n()?, dir: v3(&mut t)?, probability: t.n::<i32>()?.clamp(0, 255) },
        (3, Game::Vc) => EffectKind::PedAttractor { kind: t.n()?, queue_dir: v3(&mut t)?, use_dir: v3(&mut t)? },
        (4, Game::Vc) => EffectKind::SunGlare,
        _ => return None,
    };
    Some(Effect { model_id, pos, rgba, kind })
}

#[derive(Debug, Default, PartialEq)]
pub struct Ide {
    pub objs: Vec<Obj>,
    pub tobj: Vec<Obj>,
    pub hier: Vec<Hier>,
    pub cars: Vec<Car>,
    pub peds: Vec<Ped>,
    pub weap: Vec<Weap>,
    pub paths: Vec<NavPath>,
    pub fx2d: Vec<Effect>,
    /// Lines that did not parse, with the section they were in.
    pub rejected: Vec<(&'static str, String)>,
}

pub fn parse_ide(text: &str, game: Game) -> Ide {
    let mut ide = Ide::default();
    let mut paths = PathReader::default();
    let mut section = "";
    for l in clean_lines(text).filter(|l| !skip(l)) {
        if section.is_empty() {
            section = ["objs", "tobj", "weap", "hier", "cars", "peds", "path", "2dfx"]
                .into_iter()
                .find(|s| l.starts_with(s))
                .unwrap_or("");
        } else if l.starts_with("end") {
            section = "";
            if let Some(p) = paths.cur.take() {
                ide.rejected.push(("path", format!("incomplete path {}", p.id)));
            }
        } else {
            let ok = match section {
                "objs" => parse_obj(&l, false).map(|o| ide.objs.push(o)).is_some(),
                "tobj" => parse_obj(&l, true).map(|o| ide.tobj.push(o)).is_some(),
                "weap" if game == Game::Vc => {
                    let mut t = Tok::new(&l);
                    (|| {
                        let w = Weap { id: t.n()?, model: t.s()?, txd: t.s()?, anim: t.s()?, dist: 0.0 };
                        let _num: u32 = t.n()?;
                        Some(Weap { dist: t.n()?, ..w })
                    })()
                    .map(|w| ide.weap.push(w))
                    .is_some()
                }
                "hier" => {
                    let mut t = Tok::new(&l);
                    (|| Some(Hier { id: t.n()?, model: t.s()?, txd: t.s()? }))().map(|h| ide.hier.push(h)).is_some()
                }
                "cars" => {
                    let mut t = Tok::new(&l);
                    (|| {
                        Some(Car {
                            id: t.n()?,
                            model: t.s()?,
                            txd: t.s()?,
                            kind: t.s()?,
                            handling: t.s()?,
                            game_name: t.s()?.replace('_', " "),
                            anim: if game == Game::Vc { Some(t.s()?) } else { None },
                            class: t.s()?,
                            frequency: t.n()?,
                            level: t.n()?,
                            comp_rules: t.hex()?,
                            misc: t.n(),
                            wheel_scale: t.n(),
                        })
                    })()
                    .map(|c| ide.cars.push(c))
                    .is_some()
                }
                "peds" => {
                    let mut t = Tok::new(&l);
                    (|| {
                        Some(Ped {
                            id: t.n()?,
                            model: t.s()?,
                            txd: t.s()?,
                            ped_type: t.s()?,
                            stats: t.s()?,
                            anim_group: t.s()?,
                            cars_can_drive: t.hex()?,
                            vc: if game == Game::Vc { Some((t.s()?, t.n()?, t.n()?)) } else { None },
                        })
                    })()
                    .map(|p| ide.peds.push(p))
                    .is_some()
                }
                "path" => paths.line(&l, game, &mut ide.paths),
                "2dfx" => parse_effect(&l, game).map(|e| ide.fx2d.push(e)).is_some(),
                _ => false, // e.g. `weap` in a GTA3 file
            };
            if !ok {
                ide.rejected.push((section, l));
            }
        }
    }
    ide
}

#[derive(Debug, PartialEq)]
pub struct Inst {
    pub id: i32,
    pub name: String,
    pub pos: [f32; 3],
    pub scale: [f32; 3],
    /// Interior/area id; 0 when absent (always for GTA III).
    pub area: f32,
    /// Quaternion as stored in the file (x, y, z, w).
    pub rot: [f32; 4],
}

#[derive(Debug, PartialEq)]
pub struct Zone {
    pub name: String,
    pub kind: i32,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub level: i32,
}

#[derive(Debug, PartialEq)]
pub struct Cull {
    pub pos: [f32; 3],
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub flags: i32,
    pub wanted_level_drop: i32,
}

#[derive(Debug, Default, PartialEq)]
pub struct Ipl {
    pub inst: Vec<Inst>,
    pub zone: Vec<Zone>,
    pub cull: Vec<Cull>,
    pub pick: Vec<(i32, [f32; 3])>,
    /// Vice City occlusion volumes: x, y, z, width, length, height, angle.
    pub occl: Vec<[f32; 7]>,
    /// Vice City paths; GTA III's IPL `path` sections are unused by the game.
    pub paths: Vec<NavPath>,
    pub rejected: Vec<(&'static str, String)>,
}

fn v3(t: &mut Tok) -> Option<[f32; 3]> {
    Some([t.n()?, t.n()?, t.n()?])
}

fn parse_inst(l: &str, game: Game) -> Option<Inst> {
    let with = |area: bool| {
        let mut t = Tok::new(l);
        let (id, name) = (t.n()?, t.s()?);
        let area = if area { t.n()? } else { 0.0 };
        Some(Inst { id, name, area, pos: v3(&mut t)?, scale: v3(&mut t)?, rot: [t.n()?, t.n()?, t.n()?, t.n()?] })
    };
    // like the original: try the 13-field VC form first, then the 12-field form
    if game == Game::Vc { with(true).or_else(|| with(false)) } else { with(false) }
}

pub fn parse_ipl(text: &str, game: Game) -> Ipl {
    let mut ipl = Ipl::default();
    let mut paths = PathReader::default();
    let mut section = "";
    for l in clean_lines(text).filter(|l| !skip(l)) {
        if section.is_empty() {
            section = ["inst", "zone", "cull", "pick", "path", "occl"].into_iter().find(|s| l.starts_with(s)).unwrap_or("");
        } else if l.starts_with("end") {
            section = "";
            if let Some(p) = paths.cur.take() {
                ipl.rejected.push(("path", format!("incomplete path {}", p.id)));
            }
        } else {
            let mut t = Tok::new(&l);
            let ok = match section {
                "inst" => parse_inst(&l, game).map(|i| ipl.inst.push(i)).is_some(),
                "zone" => (|| Some(Zone { name: t.s()?, kind: t.n()?, min: v3(&mut t)?, max: v3(&mut t)?, level: t.n()? }))()
                    .map(|z| ipl.zone.push(z))
                    .is_some(),
                "cull" => (|| {
                    Some(Cull { pos: v3(&mut t)?, min: v3(&mut t)?, max: v3(&mut t)?, flags: t.n()?, wanted_level_drop: t.n().unwrap_or(0) })
                })()
                .map(|c| ipl.cull.push(c))
                .is_some(),
                "pick" => (|| Some((t.n()?, v3(&mut t)?)))().map(|p| ipl.pick.push(p)).is_some(),
                "occl" if game == Game::Vc => (|| Some([t.n()?, t.n()?, t.n()?, t.n()?, t.n()?, t.n()?, t.n()?]))()
                    .map(|o| ipl.occl.push(o))
                    .is_some(),
                "path" => paths.line(&l, game, &mut ipl.paths),
                _ => false,
            };
            if !ok {
                ipl.rejected.push((section, l));
            }
        }
    }
    ipl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gta_dat() {
        let d = parse_gta_dat("# c\nIDE DATA\\MAPS\\a.ide\nCOLFILE 2 X\\b.col\nbogus\nEXIT\nIDE nope\n");
        assert_eq!(d, vec![Directive::Ide("DATA\\MAPS\\a.ide".into()), Directive::ColFile { level: 2, path: "X\\b.col".into() }]);
    }

    #[test]
    fn ide_sections() {
        let ide = parse_ide(
            "objs\n1, a, t, 2, 100, 50, 4\n2 b t 1 80 0\nend\n\
             tobj\n3 c t 1 90 0 6 20\nend\n\
             cars\n9 car car car H Cool_Car richfamily 5 1 ff 2 0.5\nend\n\
             peds\n7 p p COP STAT_COP cop 3\nend\nobjs\nbad\nend\n",
            Game::Gta3,
        );
        assert_eq!(ide.objs[0].first_damaged, 1);
        assert_eq!(ide.objs[1].first_damaged, 0);
        assert_eq!(ide.tobj[0].time, Some((6, 20)));
        assert_eq!(ide.cars[0].game_name, "Cool Car");
        assert_eq!(ide.cars[0].comp_rules, 0xff);
        assert_eq!(ide.cars[0].misc, Some(2));
        assert_eq!(ide.peds[0].cars_can_drive, 3);
        assert_eq!(ide.rejected.len(), 1);
    }

    #[test]
    fn ipl_sections() {
        let ipl = parse_ipl("inst\n5 foo 1 2 3 1 1 1 0 0 0 1\nend\nzone\nZ 0 0 0 0 1 1 1 2\nend\ncull\n0 0 0 1 1 1 2 2 2 8\nend\n", Game::Gta3);
        assert_eq!(ipl.inst[0].pos, [1.0, 2.0, 3.0]);
        assert_eq!(ipl.zone[0].level, 2);
        assert_eq!(ipl.cull[0].wanted_level_drop, 0);
    }

    #[test]
    fn paths_and_effects() {
        let node = "0 1 0 1.0 2.0 3.0 4.0 1 2";
        let text = format!("path\nped 7 mdl\n{}end\n2dfx\n5 1 2 3 255 0 0 255 0 \"cor\" \"sh\" 1 2 3 4 5 6 7 8 9\n5 1 2 3 0 0 0 0 2 1 0 0 1 200\nend\n", format!("{node}\n").repeat(12));
        let ide = parse_ide(&text, Game::Gta3);
        assert!(ide.rejected.is_empty(), "{:?}", ide.rejected);
        assert_eq!((ide.paths[0].kind, ide.paths[0].nodes.len(), ide.paths[0].nodes[0].pos), (PathKind::Ped, 12, [1.0, 2.0, 3.0]));
        assert!(matches!(&ide.fx2d[0].kind, EffectKind::Light { corona, flags: 9, .. } if corona == "cor"));
        assert_eq!(ide.fx2d[1].kind, EffectKind::Attractor { kind: 1, dir: [0.0, 0.0, 1.0], probability: 200 });
        let short = parse_ide("path\ncar 1 m\n0 1 0 1 2 3 4 1 1\nend\n", Game::Gta3);
        assert_eq!(short.rejected.len(), 1);
    }

    #[test]
    fn vc_variants() {
        let ide = parse_ide("cars\n9 c c car H Name_X anim richfamily 5 1 ff 2 0.5\nend\npeds\n7 p p COP S g 3 anim 1 2\nend\nweap\n4 w w a 1 90\nend\n", Game::Vc);
        assert_eq!(ide.cars[0].anim.as_deref(), Some("anim"));
        assert_eq!(ide.peds[0].vc, Some(("anim".into(), 1, 2)));
        assert_eq!(ide.weap[0].dist, 90.0);
        let ipl = parse_ipl("inst\n1 a 0 1 2 3 1 1 1 0 0 0 1\n2 b 1 2 3 1 1 1 0 0 0 1\nend\noccl\n1 2 3 4 5 6 7\nend\n", Game::Vc);
        assert_eq!(ipl.occl, vec![[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]]);
        assert_eq!((ipl.inst[0].area, ipl.inst[0].pos), (0.0, [1.0, 2.0, 3.0]));
        assert_eq!(ipl.inst[1].pos, [1.0, 2.0, 3.0]);
        assert!(parse_ide("weap\n4 w w a 1 90\nend\n", Game::Gta3).weap.is_empty());
    }
}
