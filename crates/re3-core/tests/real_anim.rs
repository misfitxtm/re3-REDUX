//! Animation groups built from the real tables, skeletons and animation files.
//! Runs when GTA3_DIR / VC_DIR are set; skipped otherwise.
use glam::Vec3;
use re3_core::anim::*;
use re3_formats::{datafile::Game, dff::parse_dff, dir::Archive};
use std::{env, fs, path::{Path, PathBuf}};

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p = fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
    }
    Some(p)
}

#[test]
fn groups_resolve_against_real_skeletons() {
    for (var, game) in [("GTA3_DIR", Game::Gta3), ("VC_DIR", Game::Vc)] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let mut ar = Archive::open(&resolve(root, "models/gta3.dir").unwrap(), &resolve(root, "models/gta3.img").unwrap()).unwrap();
        let cop = ar.dir.find("cop.dff").unwrap().clone();
        let dff = parse_dff(&ar.read(&cop).unwrap()).unwrap();
        let template = AnimClump::from_dff(game, &dff);
        assert!(template.frames.len() > 10, "{var}: cop skeleton has {} frames", template.frames.len());

        let mut mgr = AnimManager::new(game);
        mgr.load_anim_file(&fs::read(resolve(root, "anim/ped.ifp").unwrap()).unwrap()).unwrap();
        // Vice City streams the other blocks (vehicles, weapons) from the main archive
        for e in ar.dir.entries().to_vec() {
            if e.size > 0 && e.name.to_ascii_lowercase().ends_with(".ifp") {
                mgr.load_anim_file(&ar.read(&e).unwrap()).unwrap();
            }
        }
        let groups = if game == Game::Vc { tables_vc::GROUPS.iter().map(|g| (g.name, g.block, g.anims, g.descs)).collect::<Vec<_>>() } else { tables_gta3::GROUPS.iter().map(|g| (g.name, g.block, g.anims, g.descs)).collect::<Vec<_>>() };
        let (mut built, mut skipped, mut animations, mut matched_bones, mut total_bones) = (0, vec![], 0, 0, 0);
        for (i, &(name, block, anims, descs)) in groups.iter().enumerate() {
            if mgr.block(block).is_none() {
                skipped.push(name);
                continue;
            }
            let def = GroupDefinition { name, block, anims, descs };
            mgr.create_group(i, &def, &template).unwrap_or_else(|e| panic!("{var} group {name}: {e}"));
            built += 1;
            for a in &mgr.groups[i].associations {
                animations += 1;
                total_bones += a.hierarchy.sequences.iter().filter(|s| !s.keyframes.is_empty()).count();
                matched_bones += a.nodes.iter().filter(|n| n.sequence.is_some()).count();
            }
        }
        println!("{var}: {built} groups built ({animations} animations; {matched_bones}/{total_bones} bone tracks matched the cop skeleton), skipped {skipped:?}");
        assert!(built >= 20, "{var}: only {built} groups");
        // the standard ped animations drive the whole body
        assert!(matched_bones * 10 >= total_bones * 9, "{var}: only {matched_bones}/{total_bones} tracks matched");

        // play the standard group with a few blends; every pose must stay finite and normalised
        let mut clump = template.clone();
        clump.velocity = Some(Vec3::ZERO);
        let std_group = 0;
        let n = mgr.groups[std_group].associations.len();
        assert!(n >= 4, "{var}: standard group has {n} animations");
        let mut events = 0;
        for step in 0..600 {
            if step % 50 == 0 {
                let anim = (step / 50) % 4;
                mgr.blend_animation(&mut clump, std_group, anim, 4.0).unwrap();
            }
            let (out, ev) = clump.update_animations(1.0 / 30.0, true);
            events += ev.len();
            for f in &out {
                if let Some(r) = f.rotation {
                    assert!(r.x.is_finite() && (r.magnitude_squared() - 1.0).abs() < 1e-3, "{var}: bad rotation {r:?} at step {step}");
                }
                if let Some(t) = f.translation {
                    assert!(t.is_finite() && t.length() < 100.0, "{var}: bad translation {t:?}");
                }
            }
        }
        let v = clump.velocity.unwrap();
        assert!(v.is_finite() && v.length() < 10.0, "{var}: velocity {v:?}");
        println!("{var}: played 600 frames, {} associations alive, {events} events, final velocity {v:?}", clump.associations.len());
    }
}
