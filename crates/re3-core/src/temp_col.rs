//! Hardcoded collision models for peds and car parts (`TempColModels.cpp`, FIX_BUGS behaviour:
//! every sphere gets its surface and piece set).
use crate::collision::{Box3, ColModel, Sphere};
use crate::surface;
use glam::Vec3;
use re3_formats::datafile::Game;

#[derive(Debug, Clone)]
pub struct TempColModels {
    pub ped1: ColModel,
    pub ped2: ColModel,
    pub bbox: ColModel,
    pub bumper1: ColModel,
    pub wheel1: ColModel,
    pub panel1: ColModel,
    pub body_part2: ColModel,
    pub body_part1: ColModel,
    pub cut_obj: [ColModel; 5],
    pub ped_ground_hit: ColModel,
    pub boot1: ColModel,
    pub door1: ColModel,
    pub bonnet1: ColModel,
    /// Vice City only.
    pub weapon: Option<ColModel>,
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

fn bbox(min: Vec3, max: Vec3) -> Box3 {
    Box3 { min, max, surface: 0, piece: 0 }
}

fn model(radius: f32, centre: Vec3, min: Vec3, max: Vec3, spheres: Vec<Sphere>) -> ColModel {
    ColModel { bounding_sphere: Sphere::new(radius, centre, 0, 0), bounding_box: bbox(min, max), spheres, ..Default::default() }
}

fn spheres(surf: u8, list: &[(f32, Vec3, u8)]) -> Vec<Sphere> {
    list.iter().map(|&(r, c, piece)| Sphere::new(r, c, surf, piece)).collect()
}

impl TempColModels {
    pub fn new(game: Game) -> Self {
        let cut = || model(2.0, Vec3::ZERO, v(-2., -2., -2.), v(2., 2., 2.), vec![]);
        let two = |r: f32, a: Vec3, b: Vec3| vec![(r, a, 0), (r, b, 0)];
        Self {
            bbox: cut(),
            cut_obj: std::array::from_fn(|_| cut()),
            ped1: model(
                1.25,
                Vec3::ZERO,
                v(-0.35, -0.35, -1.0),
                v(0.35, 0.35, 0.9),
                spheres(surface::PED, &[(0.35, v(0., 0., -0.25), 0), (0.35, v(0., 0., 0.15), 0), (0.35, v(0., 0., 0.55), 0)]),
            ),
            ped2: model(
                2.0,
                Vec3::ZERO,
                v(-0.7, -0.7, -1.2),
                v(0.7, 0.7, 0.0),
                spheres(surface::PED, &[(0.3, v(0., 0.35, -0.9), 0), (0.4, v(0., 0., -0.9), 0), (0.3, v(0., -0.35, -0.9), 0)]),
            ),
            ped_ground_hit: model(
                2.0,
                Vec3::ZERO,
                v(-0.4, -1.0, -1.25),
                v(0.4, 1.2, -0.5),
                spheres(surface::PED, &[(0.35, v(0., -0.4, -0.9), 4), (0.35, v(0., -0.1, -0.9), 1), (0.35, v(0., 0.25, -0.9), 0), (0.3, v(0., 0.65, -0.9), 6)]),
            ),
            door1: model(
                1.5,
                v(0., -0.6, 0.),
                v(-0.3, 0.0, -0.6),
                v(0.3, -1.2, 0.6),
                spheres(surface::CAR_PANEL, &[(0.15, v(0., -0.25, -0.35), 0), (0.15, v(0., -0.95, -0.35), 0), (0.25, v(0., -0.6, 0.25), 0)]),
            ),
            bumper1: model(
                2.2,
                v(0., -0.6, 0.),
                v(-1.2, -0.3, -0.2),
                v(1.2, 0.3, 0.2),
                spheres(surface::CAR_PANEL, &[(0.15, v(0.85, -0.05, 0.), 0), (0.15, v(0.4, 0.05, 0.), 0), (0.15, v(-0.4, 0.05, 0.), 0), (0.15, v(-0.85, -0.05, 0.), 0)]),
            ),
            panel1: model(
                1.4,
                Vec3::ZERO,
                v(-0.3, -0.6, -0.15),
                v(0.3, 0.6, 0.15),
                spheres(surface::CAR_PANEL, &[(0.15, v(0.15, 0.45, 0.), 0), (0.15, v(0.15, -0.45, 0.), 0), (0.15, v(-0.15, -0.45, 0.), 0), (0.15, v(-0.15, 0.45, 0.), 0)]),
            ),
            bonnet1: model(
                1.7,
                v(0., 0.5, 0.),
                v(-0.7, -0.2, -0.3),
                v(0.7, 1.2, 0.3),
                spheres(surface::CAR_PANEL, &[(0.2, v(-0.4, 0.1, 0.), 0), (0.2, v(-0.4, 0.9, 0.), 0), (0.2, v(0.4, 0.1, 0.), 0), (0.2, v(0.4, 0.9, 0.), 0)]),
            ),
            boot1: model(
                1.4,
                v(0., -0.4, 0.),
                v(-0.7, -0.9, -0.3),
                v(0.7, 0.2, 0.3),
                spheres(surface::CAR_PANEL, &[(0.2, v(-0.4, -0.1, 0.), 0), (0.2, v(-0.4, -0.6, 0.), 0), (0.2, v(0.4, -0.1, 0.), 0), (0.2, v(0.4, -0.6, 0.), 0)]),
            ),
            wheel1: model(1.4, Vec3::ZERO, v(-0.7, -0.4, -0.4), v(0.7, 0.4, 0.4), spheres(surface::WHEELBASE, &two(0.35, v(-0.3, 0., 0.), v(0.3, 0., 0.)))),
            body_part1: model(0.7, v(0.4, 0., 0.), v(-0.3, -0.3, -0.3), v(1.1, 0.3, 0.3), spheres(surface::PED, &two(0.2, v(0., 0., 0.), v(0.8, 0., 0.)))),
            body_part2: model(0.5, v(0.25, 0., 0.), v(-0.2, -0.2, -0.2), v(0.7, 0.2, 0.2), spheres(surface::PED, &two(0.15, v(0., 0., 0.), v(0.5, 0., 0.)))),
            // the original sets only the radius and the box here, leaving the centre at the origin
            weapon: (game == Game::Vc).then(|| model(0.25, Vec3::ZERO, v(-0.25, -0.25, -0.25), v(0.25, 0.25, 0.25), vec![])),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_match_the_original_tables() {
        let t = TempColModels::new(Game::Gta3);
        assert_eq!(t.ped1.spheres.len(), 3);
        assert_eq!(t.ped1.spheres[2].center, v(0., 0., 0.55));
        assert!(t.ped1.spheres.iter().all(|s| s.surface == surface::PED && s.radius == 0.35));
        assert_eq!(t.ped_ground_hit.spheres.iter().map(|s| s.piece).collect::<Vec<_>>(), [4, 1, 0, 6]);
        assert_eq!(t.door1.spheres.len(), 3);
        assert!(t.wheel1.spheres.iter().all(|s| s.surface == surface::WHEELBASE));
        assert_eq!(t.bbox.bounding_box.max, v(2., 2., 2.));
        assert!(t.weapon.is_none() && TempColModels::new(Game::Vc).weapon.is_some());
    }
}
