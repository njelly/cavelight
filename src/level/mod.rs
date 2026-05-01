mod generator;
mod tile;

pub use tile::Tile;

use avian2d::prelude::*;
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

            // Walls get a static rigid body so the player cannot walk through them,
            // plus a shadow caster so point lights cast shadows.
            if tile_type == TileType::Wall {
                let pos = tile_to_world(x, y, map.width, map.height);
                commands.spawn((
                    Transform::from_xyz(pos.x, pos.y, 0.0),
                    RigidBody::Static,
                    Collider::rectangle(TILE_SIZE, TILE_SIZE),
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

    // Offset the sprite by half a tile so tile centers land on multiples of TILE_SIZE,
    // keeping visual tiles aligned with the GridMover snap grid.
    commands.spawn((
        Sprite::from_image(image_handle),
        Transform::from_xyz(-TILE_SIZE / 2.0, -TILE_SIZE / 2.0, -1.0),
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
/// Tile centers are always at integer multiples of [`TILE_SIZE`], which keeps them
/// aligned with the [`GridMoverPlugin`]'s snap grid. For even-width maps the origin
/// falls on a tile center at the middle-right of the map; the tilemap sprite is
/// shifted to compensate so the visual result is still centered on screen.
fn tile_to_world(x: usize, y: usize, width: usize, height: usize) -> Vec2 {
    Vec2::new(
        (x as f32 - width as f32 / 2.0) * TILE_SIZE,
        (y as f32 - height as f32 / 2.0) * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_to_world_positions_on_grid() {
        // All tile centers must be exact multiples of TILE_SIZE so they align with
        // the GridMover snap grid.
        for x in 0..4usize {
            for y in 0..4usize {
                let pos = tile_to_world(x, y, 4, 4);
                assert_eq!(pos.x % TILE_SIZE, 0.0, "tile ({x},{y}) x not on grid");
                assert_eq!(pos.y % TILE_SIZE, 0.0, "tile ({x},{y}) y not on grid");
            }
        }
    }

    #[test]
    fn tile_to_world_spacing_equals_tile_size() {
        let a = tile_to_world(0, 0, 4, 4);
        let b = tile_to_world(1, 0, 4, 4);
        assert!((b.x - a.x - TILE_SIZE).abs() < f32::EPSILON);
    }
}
