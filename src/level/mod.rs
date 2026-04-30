mod generator;
mod tile;

pub use tile::Tile;

use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_light_2d::prelude::*;
use rand::thread_rng;

use generator::generate_cave;
use tile::TileType;

/// Width of a generated level in tiles.
pub const LEVEL_WIDTH: usize = 32;
/// Height of a generated level in tiles.
pub const LEVEL_HEIGHT: usize = 32;
/// Size of one tile in world units. Must match `GRID_SIZE` in main.rs.
const TILE_SIZE: f32 = 8.0;

/// World-space position where the player should spawn for the current level.
///
/// Inserted as a resource by [`spawn_level`] during [`PreStartup`] so that
/// player spawning systems (which run in [`Startup`]) can read it.
#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Vec2);

/// World-space position where the campfire should spawn for the current level.
///
/// Always a floor tile at the far end of the cave from the player start,
/// making it a natural exploration goal. Inserted in [`PreStartup`].
#[derive(Resource)]
pub struct CampfireSpawnPoint(pub Vec2);

/// Generates and spawns the level tilemap.
pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, spawn_level);
    }
}

/// Generates the cave map, builds a single tilemap texture, and inserts [`PlayerSpawnPoint`].
///
/// The entire floor/wall visual is written into one [`Image`] and rendered as a single sprite,
/// avoiding per-tile draw calls. Wall tiles also get invisible [`LightOccluder2d`] entities so
/// they cast shadows when `bevy_light_2d` point lights are present.
fn spawn_level(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let map = generate_cave(LEVEL_WIDTH, LEVEL_HEIGHT);
    let mut rng = thread_rng();

    let img_w = LEVEL_WIDTH * TILE_SIZE as usize;
    let img_h = LEVEL_HEIGHT * TILE_SIZE as usize;
    let tile_px = TILE_SIZE as usize;
    // Four bytes per pixel (RGBA).
    let mut data = vec![0u8; img_w * img_h * 4];

    for y in 0..map.height {
        for x in 0..map.width {
            let tile_type = map.get(x, y);
            let color = tile_type.color(&mut rng);
            let srgba = color.to_srgba();
            let r = (srgba.red * 255.0).round() as u8;
            let g = (srgba.green * 255.0).round() as u8;
            let b = (srgba.blue * 255.0).round() as u8;

            // Image row 0 is the top; world y increases upward, so flip vertically.
            let img_row = (map.height - 1 - y) * tile_px;
            let img_col = x * tile_px;

            for py in 0..tile_px {
                for px in 0..tile_px {
                    let idx = ((img_row + py) * img_w + (img_col + px)) * 4;
                    data[idx] = r;
                    data[idx + 1] = g;
                    data[idx + 2] = b;
                    data[idx + 3] = 255;
                }
            }

            // Walls get an invisible occluder entity so point lights cast shadows through them.
            if tile_type == TileType::Wall {
                let pos = tile_to_world(x, y, map.width, map.height);
                commands.spawn((
                    Transform::from_xyz(pos.x, pos.y, 0.0),
                    LightOccluder2d {
                        shape: LightOccluder2dShape::Rectangle {
                            half_size: Vec2::splat(TILE_SIZE / 2.0),
                        },
                    },
                    Tile,
                ));
            }
        }
    }

    let image = Image::new(
        Extent3d {
            width: img_w as u32,
            height: img_h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let image_handle = images.add(image);

    commands.spawn((
        Sprite::from_image(image_handle),
        Transform::from_xyz(0.0, 0.0, -1.0),
    ));

    let (sx, sy) = map.player_start;
    let spawn_pos = tile_to_world(sx, sy, map.width, map.height);
    commands.insert_resource(PlayerSpawnPoint(spawn_pos));

    let (cx, cy) = map.campfire_spawn;
    let campfire_pos = tile_to_world(cx, cy, map.width, map.height);
    commands.insert_resource(CampfireSpawnPoint(campfire_pos));
}

/// Converts a grid-space tile coordinate to a world-space position (tile center).
///
/// The map is centered at the world origin.
fn tile_to_world(x: usize, y: usize, width: usize, height: usize) -> Vec2 {
    Vec2::new(
        (x as f32 - (width as f32 - 1.0) / 2.0) * TILE_SIZE,
        (y as f32 - (height as f32 - 1.0) / 2.0) * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_to_world_centers_map_at_origin() {
        // For a 4x4 map, tile (1,1) and (2,2) should be symmetric around origin
        let a = tile_to_world(1, 1, 4, 4);
        let b = tile_to_world(2, 2, 4, 4);
        assert_eq!(a, -b);
    }

    #[test]
    fn tile_to_world_spacing_equals_tile_size() {
        let a = tile_to_world(0, 0, 4, 4);
        let b = tile_to_world(1, 0, 4, 4);
        assert!((b.x - a.x - TILE_SIZE).abs() < f32::EPSILON);
    }
}
