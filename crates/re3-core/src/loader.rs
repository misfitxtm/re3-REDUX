//! Builds a [`World`] from a game folder: the data files `CGame::Initialise` and `CFileLoader::LoadLevel` read.
use crate::collision::ColModel;
use crate::general::Rng;
use crate::math::Matrix;
use crate::modelinfo::{ModelInfoStore, TableLookups};
use crate::temp_col::TempColModels;
use crate::world::World;
use glam::{Mat3, Quat, Vec3};
use re3_formats::datafile::{parse_gta_dat, parse_ide, parse_ipl, Directive, Game, Ide};
use re3_formats::ped::parse_ped_stats;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a data file reader reports back.
#[derive(Debug, Default)]
pub struct LoadReport {
    pub ide_files: usize,
    pub models: usize,
    pub col_models: usize,
    pub col_missing: usize,
    pub instances: usize,
    pub buildings: usize,
    pub treadables: usize,
    pub dummies: usize,
    pub zones: usize,
    pub warnings: Vec<String>,
}

/// A game folder whose paths are matched without regard to case, as on Windows.
pub struct GameFiles {
    root: PathBuf,
}

impl GameFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        let mut p = self.root.clone();
        for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
            p = std::fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
        }
        Some(p)
    }

    pub fn bytes(&self, rel: &str) -> Result<Vec<u8>, String> {
        let p = self.resolve(rel).ok_or_else(|| format!("{rel}: not found"))?;
        std::fs::read(&p).map_err(|e| format!("{rel}: {e}"))
    }

    pub fn text(&self, rel: &str) -> Result<String, String> {
        Ok(String::from_utf8_lossy(&self.bytes(rel)?).into_owned())
    }
}

/// The directives of `default.dat` and then the game's own `.dat`, in file order.
fn directives(files: &GameFiles, game: Game) -> Result<Vec<Directive>, String> {
    let dat = if game == Game::Gta3 { "data/gta3.dat" } else { "data/gta_vc.dat" };
    let mut all = parse_gta_dat(&files.text("data/default.dat")?);
    all.extend(parse_gta_dat(&files.text(dat)?));
    Ok(all)
}

/// The rotation an IPL instance's quaternion stands for (`LoadObjectInstance`).
fn instance_matrix(rot: [f32; 4], pos: [f32; 3]) -> Matrix {
    let angle = -(2.0 * rot[3].acos());
    let axis = Vec3::new(rot[0], rot[1], rot[2]).normalize_or_zero();
    let m = Mat3::from_quat(Quat::from_axis_angle(axis, angle));
    Matrix { right: m.x_axis, forward: m.y_axis, up: m.z_axis, pos: Vec3::from(pos) }
}

/// Loads the models, collision, tables and map of a game into a new world.
pub fn load_world(root: &Path, game: Game) -> Result<(World, LoadReport), String> {
    let files = GameFiles::new(root);
    let mut report = LoadReport::default();
    let dirs = directives(&files, game)?;

    // models: every IDE, then the collision files that give them their shapes
    let stat_names: Vec<String> = parse_ped_stats(&files.text("data/pedstats.dat")?).into_iter().map(|s| s.name).collect();
    let lookups = TableLookups { game, ped_stat_names: &stat_names };
    let temp_col = TempColModels::new(game);
    let mut models = ModelInfoStore::new(game);
    models.initialise(&temp_col);
    let mut indices = crate::modelinfo::Indices::new(game);
    let mut ides: Vec<Ide> = vec![];
    for d in &dirs {
        if let Directive::Ide(p) = d {
            let ide = parse_ide(&files.text(p)?, game);
            models.load_ide(&ide, &lookups, 1.0).map_err(|e| format!("{p}: {e:?}"))?;
            report.ide_files += 1;
            ides.push(ide);
        }
    }
    // the shapes the IDE loader of the original gives peds and clump models itself
    for ide in &ides {
        for p in &ide.peds {
            models.get_mut(p.id).expect("defined by the IDE").col_model = Some(Arc::new(temp_col.ped1.clone()));
        }
        for h in &ide.hier {
            models.get_mut(h.id).expect("defined by the IDE").col_model = Some(Arc::new(temp_col.bbox.clone()));
        }
    }
    if game == Game::Gta3
        && let Some(m) = models.get_mut(i32::from(crate::modelinfo::indices_gta3::mi::LOPOLYGUY))
    {
        m.col_model = Some(Arc::new(temp_col.ped1.clone()));
    }
    for id in 0..models.limits.models as i32 {
        if let Some(m) = models.get(id) {
            let name = m.name.clone();
            indices.match_model_string(&name, id as i16);
            report.models += 1;
        }
    }
    for d in &dirs {
        if let Directive::ColFile { level, path } = d {
            for col in re3_formats::col::parse_col(&files.bytes(path)?).map_err(|e| format!("{path}: {e:?}"))? {
                let Some((id, _)) = models.find_by_name(&col.name) else {
                    report.col_missing += 1;
                    continue;
                };
                let mut shape = ColModel::from_file(&col).map_err(|e| format!("{path}: {} {e}", col.name))?;
                shape.level = *level;
                models.get_mut(id).expect("just found").col_model = Some(Arc::new(shape));
                report.col_models += 1;
            }
        }
    }

    let mut w = World::new(models, temp_col);
    w.ents.indices = indices;

    // tables
    w.ents.handling = crate::vehicle::handling::HandlingMgr::new(&re3_formats::handling::parse_handling(&files.text("data/handling.cfg")?, game));
    w.ents.ped_types = crate::ped::types::PedTypes::from_dat(&files.text("data/ped.dat")?);
    w.ents.ped_stats = crate::ped::types::PedStatTable::from_dat(&files.text("data/pedstats.dat")?);
    w.ents.weapon_infos = crate::weapons::info::WeaponInfos::from_dat(&files.text("data/weapon.dat")?);
    w.ents.adhesive_limits = re3_formats::world::parse_surface_dat(&files.text("data/surface.dat")?).limits;
    // the colour pairs each car model may be painted with (`LoadVehicleColours`)
    for (name, pairs) in re3_formats::carcols::parse_carcols(&files.text("data/carcols.dat")?).cars {
        let Some((id, _)) = w.ents.models.find_by_name(&name) else { continue };
        if let Some(v) = w.ents.models.get_mut(id).and_then(|m| match &mut m.kind {
            crate::modelinfo::Kind::Vehicle(v) => Some(v),
            _ => None,
        }) {
            for (n, &(a, b)) in pairs.iter().take(crate::modelinfo::NUM_VEHICLE_COLOURS).enumerate() {
                (v.colours1[n], v.colours2[n]) = (a, b);
                v.num_colours = n as u8 + 1;
            }
        }
    }
    if game == Game::Gta3 {
        w.ents.fight_moves = fight_moves(&files.text("data/fistfite.dat")?);
    }
    let rows = re3_formats::world::parse_object_dat(&files.text("data/object.dat")?);
    let mut table = crate::entity::object::ObjectTable::new(game);
    table.load(&rows, &mut w.ents.models);
    w.ents.objects = table;
    {
        let models = &w.ents.models;
        let lookup = |name: &str| models.find_by_name(name).map(|(id, _)| id);
        w.ents.population.load_ped_groups(&files.text("data/pedgrp.dat")?, lookup);
    }

    // road nodes of every IDE
    for ide in &ides {
        for np in ide.paths.iter().filter(|np| np.id >= 0) {
            for (n, node) in np.nodes.iter().enumerate() {
                if np.kind == re3_formats::datafile::PathKind::Car {
                    w.ents.paths.store_node_info_car(np.id as usize, n, node.kind as i8, node.next as i8, node.pos[0], node.pos[1], node.pos[2], node.num_left.unwrap_or(0) as i8, node.num_right.unwrap_or(0) as i8);
                } else {
                    w.ents.paths.store_node_info_ped(np.id as usize, n, node.kind as i8, node.next as i8, node.pos[0], node.pos[1], node.pos[2], node.cross != 0);
                }
            }
        }
    }

    // zones, then the map
    for d in &dirs {
        match d {
            Directive::MapZone(p) => {
                for z in parse_ipl(&files.text(p)?, game).zone {
                    w.ents.zones.create_map_zone(&z.name, zone_type(z.kind), Vec3::from(z.min), Vec3::from(z.max), z.level);
                    report.zones += 1;
                }
            }
            Directive::Ipl(p) => {
                let ipl = parse_ipl(&files.text(p)?, game);
                report.warnings.extend(ipl.rejected.iter().map(|(what, line)| format!("{p}: {what}: {line}")));
                for z in &ipl.zone {
                    w.ents.zones.create_zone(&z.name, zone_type(z.kind), Vec3::from(z.min), Vec3::from(z.max), z.level);
                    report.zones += 1;
                }
                for inst in &ipl.inst {
                    place_instance(&mut w, inst, &mut report);
                }
            }
            _ => {}
        }
    }
    w.ents.paths.prepare_path_data();
    Ok((w, report))
}

/// `CPed::LoadFightData` (GTA III): the rows of fistfite.dat with their animation found by name.
fn fight_moves(text: &str) -> Vec<crate::ped::data::FightMove> {
    use crate::anim::tables_gta3::{anim, GROUPS};
    use crate::ped::data::{fight_move, hit_level, FightMove};
    let group = &GROUPS[0];
    let mut moves = vec![FightMove::default(); fight_move::NUM];
    for (i, m) in re3_formats::combat::parse_fight_moves(text, Game::Gta3).0.iter().enumerate().take(moves.len()) {
        let anim_id = if m.anim == "null" { anim::STD_WALK } else { group.anims.iter().position(|a| a.eq_ignore_ascii_case(&m.anim)).map_or(anim::STD_WALK, |k| group.descs[k].0) };
        moves[i] = FightMove {
            anim_id,
            start_fire_time: m.start_fire / 30.0,
            end_fire_time: m.end_fire / 30.0,
            combo_follow_on_time: m.combo_follow_on / 30.0,
            strike_radius: m.strike_radius,
            hit_level: match m.hit_level {
                'G' => hit_level::GROUND,
                'H' => hit_level::HIGH,
                'L' => hit_level::LOW,
                'M' => hit_level::MEDIUM,
                _ => hit_level::NULL,
            },
            damage: m.damage as u8,
            flags: m.flags as u8,
        };
    }
    moves
}

fn zone_type(kind: i32) -> crate::zones::ZoneType {
    use crate::zones::ZoneType as Z;
    match kind {
        1 => Z::Navig,
        2 => Z::Info,
        3 => Z::MapZone,
        _ => Z::Default,
    }
}

/// `CFileLoader::LoadObjectInstance`.
fn place_instance(w: &mut World, inst: &re3_formats::datafile::Inst, report: &mut LoadReport) {
    report.instances += 1;
    let Ok(id) = i16::try_from(inst.id) else { return };
    let Some(info) = w.ents.models.get(i32::from(id)) else { return };
    let (Some(simple), object_id) = (info.simple(), info.object_id) else { return };
    let (big, subway, largest) = (simple.is_big_building, simple.flags.is_subway, info.largest_lod_distance(1.0));
    let matrix = instance_matrix(inst.rot, inst.pos);
    let mut rng = Rng::default();
    let level = w.ents.zones.get_level_from_position(matrix.pos);
    if object_id == -1 {
        let treadable = w.ents.paths.is_path_object(id as usize);
        let created = if treadable { w.new_treadable(id, &mut rng) } else { w.new_building(id, &mut rng) };
        let Some(e) = created else { return };
        if treadable {
            w.ents.paths.register_map_object(id, matrix);
            report.treadables += 1;
        } else {
            report.buildings += 1;
        }
        let ent = w.ent_mut(e);
        ent.base.matrix = matrix;
        ent.base.level = level as u16;
        if big {
            ent.base.flags.is_big_building = true;
            ent.base.flags.uses_collision = false;
        }
        if subway {
            ent.base.flags.is_subway = true;
        }
        if largest < 2.0 {
            ent.base.flags.is_visible = false;
        }
        let _ = w.add(e);
    } else {
        let Some(e) = w.new_dummy(id, &mut rng) else { return };
        let ent = w.ent_mut(e);
        ent.base.matrix = matrix;
        ent.base.level = level as u16;
        let glass = w.ents.indices.is_glass(&w.ents.models, id);
        if glass {
            w.ent_mut(e).base.flags.is_visible = false;
        }
        let _ = w.add(e);
        report.dummies += 1;
    }
}
