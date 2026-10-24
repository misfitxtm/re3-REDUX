//! Runs against real game files when GTA3_DIR / VC_DIR are set; skipped otherwise.
use re3_formats::dir::Directory;
use std::{env, fs, path::Path};

fn check(root: &str, pairs: &[&str]) {
    for p in pairs {
        let dir = Path::new(root).join(format!("{p}.dir"));
        let img = Path::new(root).join(format!("{p}.img"));
        let bytes = fs::read(&dir).unwrap();
        assert_eq!(bytes.len() % 32, 0, "{p}.dir has a partial record");
        let d = Directory::parse(&bytes);
        let img_len = fs::metadata(&img).unwrap().len();
        assert!(!d.entries().is_empty());
        for e in d.entries() {
            assert!(e.byte_range().end <= img_len, "{p}: {} overruns the img", e.name);
        }
        println!("{p}: {} entries ok", d.entries().len());
    }
}

#[test]
fn gta3() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    check(&root, &["models/gta3", "models/txd", "anim/cuts"]);
}

#[test]
fn vc() {
    let Ok(root) = env::var("VC_DIR") else { return };
    check(&root, &["models/gta3", "anim/cuts"]);
}

#[test]
fn gxt() {
    use re3_formats::gxt::Gxt;
    for (var, langs) in [
        ("GTA3_DIR", &["american", "english", "french", "german", "italian", "spanish"][..]),
        ("VC_DIR", &["american", "french", "german", "italian", "spanish"][..]),
    ] {
        let Ok(root) = env::var(var) else { continue };
        for l in langs {
            let bytes = fs::read(Path::new(&root).join(format!("TEXT/{l}.gxt"))).unwrap();
            let g = Gxt::parse(&bytes).unwrap_or_else(|e| panic!("{var} {l}: {e:?}"));
            assert!(!g.main.is_empty());
            let mut mission_keys = 0;
            for (name, _) in &g.missions {
                mission_keys += g.mission(name).unwrap().unwrap().len();
            }
            for k in g.main.keys() {
                let k = std::str::from_utf8(k).unwrap();
                assert!(g.main.get(k).is_some());
            }
            println!("{var} {l}: {} keys, {} mission tables, {mission_keys} mission keys", g.main.len(), g.missions.len());
        }
    }
}

/// Resolves a Windows-style, any-case relative path against `root` on a case-sensitive filesystem.
fn resolve(root: &Path, rel: &str) -> Option<std::path::PathBuf> {
    let mut p = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        let hit = fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        p = hit.path();
    }
    Some(p)
}

#[test]
fn data_files() {
    use re3_formats::datafile::*;
    for (var, dat) in [("GTA3_DIR", "data/gta3.dat"), ("VC_DIR", "data/gta_vc.dat")] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let Some(datp) = resolve(root, dat) else { println!("{var}: {dat} not found"); continue };
        let game = if var == "VC_DIR" { Game::Vc } else { Game::Gta3 };
        let mut dirs = vec![];
        for f in ["data/default.dat", dat] {
            let p = resolve(root, f).unwrap();
            dirs.extend(parse_gta_dat(&String::from_utf8_lossy(&fs::read(p).unwrap())));
        }
        let _ = datp;
        let (mut ides, mut ipls, mut rejected, mut missing) = (0, 0, vec![], vec![]);
        let (mut objs, mut cars, mut peds, mut inst, mut paths, mut fx) = (0, 0, 0, 0, 0, 0);
        for d in &dirs {
            match d {
                Directive::Ide(p) | Directive::Ipl(p) => {
                    let Some(f) = resolve(root, p) else { missing.push(p.clone()); continue };
                    let text = String::from_utf8_lossy(&fs::read(f).unwrap()).into_owned();
                    if matches!(d, Directive::Ide(_)) {
                        let ide = parse_ide(&text, game);
                        ides += 1;
                        objs += ide.objs.len() + ide.tobj.len();
                        cars += ide.cars.len();
                        peds += ide.peds.len();
                        paths += ide.paths.len();
                        fx += ide.fx2d.len();
                        rejected.extend(ide.rejected.into_iter().map(|(s, l)| format!("{p} [{s}] {l}")));
                    } else {
                        let ipl = parse_ipl(&text, game);
                        ipls += 1;
                        inst += ipl.inst.len();
                        paths += ipl.paths.len();
                        rejected.extend(ipl.rejected.into_iter().map(|(s, l)| format!("{p} [{s}] {l}")));
                    }
                }
                _ => {}
            }
        }
        println!("{var}: {} directives, {ides} ide ({objs} objs, {cars} cars, {peds} peds, {paths} paths, {fx} effects), {ipls} ipl ({inst} inst)", dirs.len());
        assert!(paths > 0 && fx > 0);
        assert!(missing.is_empty() && rejected.is_empty(), "{var}: missing {missing:?}, rejected {rejected:#?}");
        assert!(objs > 2000 && cars > 50 && peds > 80 && inst > 8000, "{var}: suspiciously few records");
    }
}

fn text(root: &Path, rel: &str) -> String {
    String::from_utf8_lossy(&fs::read(resolve(root, rel).unwrap_or_else(|| panic!("{rel} missing"))).unwrap()).into_owned()
}

#[test]
fn config_files() {
    use re3_formats::{carcols::*, combat::*, datafile::Game, handling::*, ped::*, world::*};
    for (var, game) in [("GTA3_DIR", Game::Gta3), ("VC_DIR", Game::Vc)] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let vc = game == Game::Vc;

        let h = parse_handling(&text(root, "data/handling.cfg"), game);
        println!("{var} handling: {} cars, {} bikes, {} flying, {} boats, rejected {:?}", h.cars.len(), h.bikes.len(), h.flying.len(), h.boats.len(), h.rejected);
        assert!(h.rejected.is_empty() && h.cars.len() > 50);

        let types = parse_ped_dat(&text(root, "data/ped.dat"));
        let stats = parse_ped_stats(&text(root, "data/pedstats.dat"));
        let groups = parse_ped_groups(&text(root, "data/pedgrp.dat"), if vc { 16 } else { 8 });
        println!("{var} ped: {} types, {} stats, {} groups", types.len(), stats.len(), groups.len());
        assert!(types.len() >= 20 && stats.len() > 20 && groups.len() > 10 && types.iter().any(|t| !t.threats.is_empty()));

        let cc = parse_carcols(&text(root, "data/carcols.dat"));
        println!("{var} carcols: {} colours, {} cars", cc.colours.len(), cc.cars.len());
        assert!(cc.colours.len() > 60 && cc.cars.len() > 40);

        let (w, bad) = parse_weapons(&text(root, "data/weapon.dat"), game);
        println!("{var} weapons: {}, bad {bad:?}", w.len());
        assert!(bad.is_empty() && w.len() >= 13);
        let (f, bad) = parse_fight_moves(&text(root, "data/fistfite.dat"), game);
        println!("{var} fight moves: {}, bad {bad:?}", f.len());
        assert!(bad.is_empty() && f.len() > 20);

        let o = parse_object_dat(&text(root, "data/object.dat"));
        let s = parse_surface_dat(&text(root, "data/surface.dat"));
        let wa = parse_water_dat(&text(root, "data/water.dat"));
        println!("{var} world: {} objects, {} surfaces, {} water rects", o.len(), s.names.len(), wa.len());
        assert!(o.len() > 100 && s.names.len() == if vc { 6 } else { 5 } && !wa.is_empty());

        let wp = parse_waterpro(&fs::read(resolve(root, "data/waterpro.dat").unwrap()).unwrap());
        println!("{var} waterpro: {:?}", wp.as_ref().map(|w| w.count));
        let wp = wp.expect("waterpro.dat size");
        // the original game also stored the `* ;end of file` line as a bogus extra level
        assert!((wa.len()..=wa.len() + 1).contains(&(wp.count as usize)), "water.dat has {} rects, waterpro.dat {}", wa.len(), wp.count);
        for (i, r) in wa.iter().enumerate() {
            // CRect order is left, y-max, right, y-min
            assert_eq!((wp.zs[i], wp.rects[i]), (r.z, [r.rect[0], r.rect[3], r.rect[2], r.rect[1]]), "water rect {i}");
        }

        let t = parse_timecyc(&text(root, "data/timecyc.dat"), game).unwrap();
        println!("{var} timecyc rows: {}", t.len());
        assert_eq!(t.len() % HOURS, 0);

        let p = parse_particle_cfg(&text(root, "data/particle.cfg"));
        println!("{var} particles: {}", p.len());
        assert!(p.len() > 30);

        let tracks: &[&str] = if vc { &["data/paths/flight.dat", "data/paths/flight2.dat", "data/paths/flight3.dat"] } else { &["data/paths/tracks.dat", "data/paths/tracks2.dat", "data/paths/flight.dat"] };
        for f in tracks {
            let n = parse_track(&text(root, f)).unwrap_or_else(|| panic!("{f} bad")).len();
            println!("{var} {f}: {n} nodes");
        }
    }
}

/// Every model, texture dictionary and collision entry in the main archives must parse.
#[test]
fn binary_archives() {
    use re3_formats::{col::parse_col, dff::parse_dff, dir::Archive, txd::parse_txd};
    for (var, archives) in [
        ("GTA3_DIR", &["models/gta3", "models/txd"][..]),
        ("VC_DIR", &["models/gta3"][..]),
    ] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        for a in archives {
            let mut ar = Archive::open(&resolve(root, &format!("{a}.dir")).unwrap(), &resolve(root, &format!("{a}.img")).unwrap()).unwrap();
            let entries: Vec<_> = ar.dir.entries().to_vec();
            let (mut dff, mut txd, mut col, mut other, mut tex, mut verts) = (0, 0, 0, 0, 0, 0);
            let mut failures = vec![];
            let (mut empty, mut other_platform) = (0, vec![]);
            for e in &entries {
                if e.size == 0 {
                    empty += 1; // placeholder entries
                    continue;
                }
                let bytes = ar.read(e).unwrap();
                let ext = e.name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                // entries are padded to whole 2048-byte sectors; the block itself says where it ends
                match ext.as_str() {
                    "dff" => match parse_dff(&bytes) {
                        Ok(d) => {
                            dff += 1;
                            verts += d.geometries.iter().map(|g| g.vertices.len()).sum::<usize>();
                        }
                        Err(err) => failures.push(format!("{}: {err:?}", e.name)),
                    },
                    "txd" => match parse_txd(&bytes) {
                        Ok(t) => {
                            txd += 1;
                            tex += t.textures.len();
                            for x in &t.textures {
                                if x.expected_top_mip().is_some_and(|n| x.mips[0].len() != n) {
                                    failures.push(format!("{}/{}: top mip {} bytes, expected {:?}", e.name, x.name, x.mips[0].len(), x.expected_top_mip()));
                                }
                            }
                        }
                        Err(re3_formats::rw::Error::Platform(p)) => other_platform.push((e.name.clone(), p)),
                        Err(err) => failures.push(format!("{}: {err:?}", e.name)),
                    },
                    "col" => match parse_col(&bytes) {
                        Ok(_) => col += 1,
                        Err(err) => failures.push(format!("{}: {err:?}", e.name)),
                    },
                    _ => other += 1,
                }
            }
            println!("{var} {a}: {dff} dff ({verts} vertices), {txd} txd ({tex} textures), {col} col, {other} other, {empty} empty, non-PC textures {other_platform:?}; {} failures", failures.len());
            for f in failures.iter().take(10) {
                println!("  {f}");
            }
            assert!(failures.is_empty(), "{var} {a}: {} failures", failures.len());
        }
    }
}

/// Standalone files named by `default.dat` / `gta3.dat`: collision, models, hierarchies, textures.
#[test]
fn directive_files() {
    use re3_formats::{col::parse_col, datafile::*, dff::parse_dff, txd::parse_txd};
    for (var, dat) in [("GTA3_DIR", "data/gta3.dat"), ("VC_DIR", "data/gta_vc.dat")] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let (mut col, mut models, mut txds, mut skipped) = (0, 0, 0, vec![]);
        for f in ["data/default.dat", dat] {
            for d in parse_gta_dat(&text(root, f)) {
                let (path, kind) = match &d {
                    Directive::ColFile { path, .. } => (path, 'c'),
                    Directive::ModelFile(p) | Directive::HierFile(p) => (p, 'm'),
                    Directive::TexDiction(p) => (p, 't'),
                    _ => continue,
                };
                let Some(file) = resolve(root, path) else { skipped.push(path.clone()); continue };
                let bytes = fs::read(file).unwrap();
                match kind {
                    'c' => col += parse_col(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}")).len(),
                    'm' => {
                        parse_dff(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}"));
                        models += 1;
                    }
                    _ => {
                        parse_txd(&bytes).unwrap_or_else(|e| panic!("{path}: {e:?}"));
                        txds += 1;
                    }
                }
            }
        }
        println!("{var}: {col} collision models, {models} model files, {txds} texture dictionaries, not found: {skipped:?}");
        assert!(col > 0 && models > 0 && txds > 0);
    }
}

#[test]
fn animations() {
    use re3_formats::{dir::Archive, ifp::parse_ifp};
    for (var, archives) in [("GTA3_DIR", &["models/gta3", "anim/cuts"][..]), ("VC_DIR", &["models/gta3", "anim/cuts"][..])] {
        let Ok(root) = env::var(var) else { continue };
        let root = Path::new(&root);
        let ped = parse_ifp(&fs::read(resolve(root, "anim/ped.ifp").unwrap()).unwrap()).unwrap();
        let count = |b: &[re3_formats::ifp::Block]| (b.len(), b.iter().map(|b| b.animations.len()).sum::<usize>(), b.iter().flat_map(|b| &b.animations).map(|a| a.sequences.len()).sum::<usize>());
        println!("{var} ped.ifp: {:?} (blocks, animations, sequences)", count(&ped));
        assert!(ped.iter().map(|b| b.animations.len()).sum::<usize>() > 100);
        for a in archives {
            let mut ar = Archive::open(&resolve(root, &format!("{a}.dir")).unwrap(), &resolve(root, &format!("{a}.img")).unwrap()).unwrap();
            let (mut files, mut anims, mut frames) = (0, 0, 0);
            for e in ar.dir.entries().to_vec() {
                if e.size > 0 && e.name.to_ascii_lowercase().ends_with(".ifp") {
                    let blocks = parse_ifp(&ar.read(&e).unwrap()).unwrap_or_else(|err| panic!("{}: {err:?}", e.name));
                    files += 1;
                    anims += blocks.iter().map(|b| b.animations.len()).sum::<usize>();
                    frames += blocks.iter().flat_map(|b| &b.animations).flat_map(|a| &a.sequences).map(|s| s.keyframes.len()).sum::<usize>();
                }
            }
            println!("{var} {a}: {files} ifp files, {anims} animations, {frames} keyframes");
        }
    }
}
