//! Surface types and the road adhesion table (`SurfaceTable.h/.cpp`).
use re3_formats::{datafile::Game, world::Surfaces};

pub const DEFAULT: u8 = 0;
pub const TARMAC: u8 = 1;
pub const GRASS: u8 = 2;
pub const GRAVEL: u8 = 3;
pub const MUD_DRY: u8 = 4;
pub const PAVEMENT: u8 = 5;
pub const CAR: u8 = 6;
pub const GLASS: u8 = 7;
pub const TRANSPARENT_CLOTH: u8 = 8;
pub const GARAGE_DOOR: u8 = 9;
pub const CAR_PANEL: u8 = 10;
pub const THICK_METAL_PLATE: u8 = 11;
pub const SCAFFOLD_POLE: u8 = 12;
pub const LAMP_POST: u8 = 13;
pub const FIRE_HYDRANT: u8 = 14;
pub const GIRDER: u8 = 15;
pub const METAL_CHAIN_FENCE: u8 = 16;
pub const PED: u8 = 17;
pub const SAND: u8 = 18;
pub const WATER: u8 = 19;
pub const WOOD_CRATES: u8 = 20;
pub const WOOD_BENCH: u8 = 21;
pub const WOOD_SOLID: u8 = 22;
pub const RUBBER: u8 = 23;
pub const PLASTIC: u8 = 24;
pub const HEDGE: u8 = 25;
pub const STEEP_CLIFF: u8 = 26;
pub const CONTAINER: u8 = 27;
pub const NEWS_VENDOR: u8 = 28;
pub const WHEELBASE: u8 = 29;
pub const CARDBOARDBOX: u8 = 30;
pub const TRANSPARENT_STONE: u8 = 31;
pub const METAL_GATE: u8 = 32;
/// Vice City only.
pub const SAND_BEACH: u8 = 33;
pub const CONCRETE_BEACH: u8 = 34;

pub const ADHESIVE_RUBBER: usize = 0;
pub const ADHESIVE_HARD: usize = 1;
pub const ADHESIVE_ROAD: usize = 2;
pub const ADHESIVE_LOOSE: usize = 3;
pub const ADHESIVE_WET: usize = 4;
/// Vice City only.
pub const ADHESIVE_SAND: usize = 5;

/// With the FIX_BUGS list, which both PC builds use.
pub fn is_see_through(s: u8) -> bool {
    matches!(s, GLASS | TRANSPARENT_CLOTH | METAL_CHAIN_FENCE | TRANSPARENT_STONE | SCAFFOLD_POLE)
}

/// Vice City only: the shorter list its vertical-line test uses.
pub fn is_see_through_vertical(s: u8) -> bool {
    matches!(s, GLASS | TRANSPARENT_CLOTH)
}

/// Vice City only.
pub fn is_shoot_through(s: u8) -> bool {
    matches!(s, TRANSPARENT_CLOTH | METAL_CHAIN_FENCE | TRANSPARENT_STONE | SCAFFOLD_POLE)
}

/// Vice City only.
pub fn is_soft_landing(s: u8) -> bool {
    matches!(s, GRASS | SAND | SAND_BEACH)
}

pub fn adhesion_group(game: Game, s: u8) -> usize {
    match s {
        DEFAULT | TARMAC | PAVEMENT | WOOD_CRATES | WOOD_BENCH | WOOD_SOLID => ADHESIVE_ROAD,
        GRASS | GRAVEL | HEDGE | STEEP_CLIFF | CARDBOARDBOX => ADHESIVE_LOOSE,
        SAND if game == Game::Vc => ADHESIVE_SAND,
        SAND => ADHESIVE_LOOSE,
        SAND_BEACH if game == Game::Vc => ADHESIVE_SAND,
        CONCRETE_BEACH if game == Game::Vc => ADHESIVE_ROAD,
        PED | RUBBER | WHEELBASE => ADHESIVE_RUBBER,
        WATER => ADHESIVE_WET,
        MUD_DRY | CAR | GLASS | TRANSPARENT_CLOTH | GARAGE_DOOR | CAR_PANEL | THICK_METAL_PLATE | SCAFFOLD_POLE | LAMP_POST
        | FIRE_HYDRANT | GIRDER | METAL_CHAIN_FENCE | PLASTIC | CONTAINER | NEWS_VENDOR | TRANSPARENT_STONE | METAL_GATE => ADHESIVE_HARD,
        _ => ADHESIVE_ROAD,
    }
}

/// Grip multiplier for the current road wetness (`wet_roads` is 0..1).
pub fn wet_multiplier(game: Game, s: u8, wet_roads: f32) -> f32 {
    match s {
        DEFAULT | TARMAC | MUD_DRY | PAVEMENT | TRANSPARENT_CLOTH | WOOD_CRATES | WOOD_BENCH | WOOD_SOLID | HEDGE | CARDBOARDBOX
        | TRANSPARENT_STONE => 1.0 - wet_roads * 0.25,
        CONCRETE_BEACH if game == Game::Vc => 1.0 - wet_roads * 0.25,
        GRASS | CAR | GLASS | GARAGE_DOOR | CAR_PANEL | THICK_METAL_PLATE | SCAFFOLD_POLE | LAMP_POST | FIRE_HYDRANT | GIRDER
        | METAL_CHAIN_FENCE | PED | RUBBER | PLASTIC | STEEP_CLIFF | CONTAINER | NEWS_VENDOR | WHEELBASE | METAL_GATE => {
            1.0 - wet_roads * 0.4
        }
        SAND | SAND_BEACH if game == Game::Vc => 1.0 + wet_roads * 0.5,
        _ => 1.0,
    }
}

/// Adhesion limits between pairs of adhesion groups, from `surface.dat`.
#[derive(Debug, Clone)]
pub struct SurfaceTable {
    game: Game,
    limits: Vec<Vec<f32>>,
}

impl SurfaceTable {
    pub fn new(game: Game, parsed: &Surfaces) -> Self {
        Self { game, limits: parsed.limits.clone() }
    }

    /// `GetAdhesiveLimit`: indexed [group of B][group of A].
    pub fn adhesive_limit(&self, surface_a: u8, surface_b: u8) -> f32 {
        self.limits[adhesion_group(self.game, surface_b)][adhesion_group(self.game, surface_a)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use re3_formats::world::parse_surface_dat;

    #[test]
    fn groups_and_lists() {
        assert_eq!(adhesion_group(Game::Gta3, SAND), ADHESIVE_LOOSE);
        assert_eq!(adhesion_group(Game::Vc, SAND), ADHESIVE_SAND);
        assert_eq!(adhesion_group(Game::Gta3, 200), ADHESIVE_ROAD);
        assert!(is_see_through(SCAFFOLD_POLE) && !is_see_through_vertical(SCAFFOLD_POLE) && is_shoot_through(SCAFFOLD_POLE));
        assert!(is_soft_landing(SAND_BEACH) && !is_soft_landing(TARMAC));
    }

    #[test]
    fn wetness() {
        assert_eq!(wet_multiplier(Game::Gta3, TARMAC, 1.0), 0.75);
        assert_eq!(wet_multiplier(Game::Gta3, CAR, 1.0), 0.6);
        assert_eq!(wet_multiplier(Game::Vc, SAND, 1.0), 1.5);
        assert_eq!(wet_multiplier(Game::Gta3, WATER, 1.0), 1.0);
    }

    #[test]
    fn limit_lookup_is_symmetric_table() {
        let t = SurfaceTable::new(Game::Gta3, &parse_surface_dat("Rubber 6.0\nHard 3.6 2.0\nRoad 4.5 3.0 6.0\nLoose 3.2 3.5 2.0 1.0\nWet 1 2 3 4 5\n"));
        assert_eq!(t.adhesive_limit(RUBBER, GLASS), 3.6); // [hard][rubber]
        assert_eq!(t.adhesive_limit(GLASS, RUBBER), 3.6);
    }
}
