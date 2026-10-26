//! The path graph built from the real GTA III IDE `path` sections and IPL placements. Runs when GTA3_DIR is set.
use glam::{Mat3, Quat, Vec3};
use re3_core::control::pathfind::*;
use re3_core::math::Matrix;
use re3_formats::datafile::*;
use std::{env, fs, path::{Path, PathBuf}};

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|s| !s.is_empty()) {
        p = fs::read_dir(&p).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path();
    }
    Some(p)
}

fn text(root: &Path, rel: &str) -> String {
    String::from_utf8_lossy(&fs::read(resolve(root, rel).unwrap_or_else(|| panic!("{rel}"))).unwrap()).into_owned()
}

#[test]
fn gta3_path_graph() {
    let Ok(root) = env::var("GTA3_DIR") else { return };
    let root = Path::new(&root);
    let mut pf = PathFind::default();
    let mut ids = vec![];
    for f in ["data/default.dat", "data/gta3.dat"] {
        for d in parse_gta_dat(&text(root, f)) {
            if let Directive::Ide(p) = d {
                let ide = parse_ide(&text(root, &p), Game::Gta3);
                for np in &ide.paths {
                    if np.id < 0 {
                        continue;
                    }
                    for (n, node) in np.nodes.iter().enumerate() {
                        if np.kind == PathKind::Car {
                            pf.store_node_info_car(np.id as usize, n, node.kind as i8, node.next as i8, node.pos[0], node.pos[1], node.pos[2], node.num_left.unwrap_or(0) as i8, node.num_right.unwrap_or(0) as i8);
                        } else {
                            pf.store_node_info_ped(np.id as usize, n, node.kind as i8, node.next as i8, node.pos[0], node.pos[1], node.pos[2], node.cross != 0);
                        }
                    }
                }
                ids.extend(ide.objs.iter().map(|o| o.id));
            }
        }
    }
    let mut placed = 0;
    for d in parse_gta_dat(&text(root, "data/gta3.dat")) {
        if let Directive::Ipl(p) = d {
            let ipl = parse_ipl(&text(root, &p), Game::Gta3);
            for i in &ipl.inst {
                if i.id >= 0 && pf.is_path_object(i.id as usize) {
                    let angle = -(2.0 * i.rot[3].acos());
                    let axis = Vec3::new(i.rot[0], i.rot[1], i.rot[2]).normalize_or_zero();
                    let m = Mat3::from_quat(Quat::from_axis_angle(axis, angle));
                    pf.register_map_object(i.id as i16, Matrix { right: m.x_axis, forward: m.y_axis, up: m.z_axis, pos: Vec3::new(i.pos[0], i.pos[1], i.pos[2]) });
                    placed += 1;
                }
            }
        }
    }
    pf.prepare_path_data();
    println!("{placed} road pieces: {} car nodes, {} ped nodes, {} links, {} car links, groups {:?}", pf.num_car_path_nodes, pf.num_ped_path_nodes, pf.num_connections, pf.num_car_path_links, pf.num_groups);
    assert!(placed > 100 && pf.num_car_path_nodes > 500 && pf.num_ped_path_nodes > 500);
    assert!(pf.num_path_nodes <= NUM_PATHNODES && pf.num_car_path_links <= NUM_CARPATHLINKS && pf.num_connections <= NUM_PATHCONNECTIONS);
    // a search from one car node to a neighbour
    let n0 = (0..pf.num_car_path_nodes).find(|&i| pf.nodes[i].num_links > 0).unwrap();
    let n1 = pf.connected_node(pf.nodes[n0].first_link as usize);
    let (route, dist) = pf.do_path_search(PATH_CAR, pf.nodes[n0].pos, n0 as i32, pf.nodes[n1].pos, 8, None, 100.0, n1 as i32);
    assert_eq!(route.last(), Some(&n1));
    assert!(dist > 0.0 && dist < 1000.0);
}
