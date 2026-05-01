use bevy::prelude::*;
use rand::Rng;

/// The logical type of a map tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Wall,
    Floor,
}

impl TileType {
    /// Returns a render color for this tile with slight random brightness variation,
    /// giving cave walls natural erosion detail and floors visual texture.
    pub fn color(&self, rng: &mut impl Rng) -> Color {
        let v = rng.gen_range(-0.03..0.03f32);
        match self {
            // Dark brownish-gray stone
            TileType::Wall => {
                let b = 0.20 + v;
                Color::srgb(b * 0.90, b * 0.85, b * 0.80)
            }
            // Lighter earthy cave floor
            TileType::Floor => {
                let b = 0.35 + v;
                Color::srgb(b * 0.85, b * 0.80, b * 0.75)
            }
        }
    }

}

/// Marks a spawned tile entity.
///
/// Currently a marker; `tile_type: TileType` will be added back once a system (collision,
/// pathfinding, etc.) needs to query it.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Tile;

