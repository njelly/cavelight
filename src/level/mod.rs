mod generator;
mod tile;

pub use generator::DoorOrientation;
pub use tile::Tile;

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_light_2d::prelude::*;
use rand::thread_rng;

use generator::generate_level1;
use tile::TileType;

/// Width of the generated level in tiles.
pub const LEVEL_WIDTH: usize = 64;
/// Height of the generated level in tiles.
pub const LEVEL_HEIGHT: usize = 64;
/// Size of one tile in world units. Must match `GRID_SIZE` in main.rs.
pub const TILE_SIZE: f32 = 8.0;

// ---------------------------------------------------------------------------
// Spawn point resources (inserted in PreStartup, read in Startup)
// ---------------------------------------------------------------------------

/// World-space position where the player should spawn for the current level.
#[derive(Resource)]
pub struct PlayerSpawnPoint(pub Vec2);

/// World-space position where the campfire should spawn for the current level.
#[derive(Resource)]
pub struct CampfireSpawnPoint(pub Vec2);

/// World-space position where the weapon chest (bow + arrows) should spawn.
#[derive(Resource)]
pub struct WeaponChestSpawnPoint(pub Vec2);

/// World-space position where the key chest should spawn.
#[derive(Resource)]
pub struct KeyChestSpawnPoint(pub Vec2);

/// World-space position and orientation of the locked door entity.
#[derive(Resource)]
pub struct LockedDoorSpawnPoint {
    pub pos: Vec2,
    pub orientation: DoorOrientation,
}

/// World-space position where the signpost should spawn for the current level.
#[derive(Resource)]
pub struct SignpostSpawnPoint(pub Vec2);

/// World-space position where the NPC should spawn for the current level.
#[derive(Resource)]
pub struct NpcSpawnPoint(pub Vec2);

/// World-space position where the exit ladder should spawn for the current level.
#[derive(Resource)]
pub struct LadderSpawnPoint(pub Vec2);

/// World-space position of the enemy spawner entity in the key chest room.
#[derive(Resource)]
pub struct SpawnerSpawnPoint(pub Vec2);

// ---------------------------------------------------------------------------
// Walkability grid
// ---------------------------------------------------------------------------

/// Read-only walkability representation of the generated level.
///
/// Inserted in [`PreStartup`] alongside the spawn point resources. Systems that need
/// to reason about static passability (e.g. the A\* pathfinder in [`crate::npc`]) read
/// this resource rather than querying individual tile entities.
#[derive(Resource)]
pub struct LevelTiles {
    pub width: usize,
    pub height: usize,
    /// Row-major walkability flags: `true` = passable floor, `false` = wall.
    walkable: Vec<bool>,
}

impl LevelTiles {
    /// Constructs a `LevelTiles` from a raw walkability vector. Available in tests only.
    #[cfg(test)]
    pub fn from_walkable(width: usize, height: usize, walkable: Vec<bool>) -> Self {
        Self { width, height, walkable }
    }

    /// Marks tile `(x, y)` as a wall. Available in tests only.
    #[cfg(test)]
    pub fn set_wall(&mut self, x: usize, y: usize) {
        self.walkable[y * self.width + x] = false;
    }

    /// Returns `true` if tile `(x, y)` is passable. Out-of-bounds always returns `false`.
    pub fn is_walkable(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.walkable[y * self.width + x]
    }

    /// Converts a world-space position to tile coordinates.
    ///
    /// Returns `None` if the position maps outside the level boundaries.
    pub fn world_to_tile(&self, pos: Vec2) -> Option<(usize, usize)> {
        let tx = (pos.x / TILE_SIZE + self.width as f32 / 2.0).round() as i32;
        let ty = (pos.y / TILE_SIZE + self.height as f32 / 2.0).round() as i32;
        if tx >= 0 && ty >= 0 && (tx as usize) < self.width && (ty as usize) < self.height {
            Some((tx as usize, ty as usize))
        } else {
            None
        }
    }

    /// Converts tile coordinates to the world-space center of that tile.
    pub fn tile_to_world(&self, x: usize, y: usize) -> Vec2 {
        Vec2::new(
            (x as f32 - self.width as f32 / 2.0) * TILE_SIZE,
            (y as f32 - self.height as f32 / 2.0) * TILE_SIZE,
        )
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Generates and spawns the level tilemap.
pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Tile>()
            .add_systems(PreStartup, spawn_level);
    }
}

// ---------------------------------------------------------------------------
// Startup system
// ---------------------------------------------------------------------------

/// Generates the cave map, builds a single tilemap texture, and inserts all level resources.
///
/// The entire floor/wall visual is written into one [`Image`] and rendered as a single sprite,
/// avoiding per-tile draw calls. Wall tiles also get invisible [`LightOccluder2d`] entities so
/// they cast shadows when `bevy_light_2d` point lights are present.
fn spawn_level(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let map = generate_level1(LEVEL_WIDTH, LEVEL_HEIGHT);
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

            // Walls get a static rigid body so the player cannot walk through them.
            // Only walls adjacent to at least one floor tile get a LightOccluder2d —
            // deep interior walls (surrounded entirely by other walls) are never
            // reachable by a light ray from the cave floor, so they don't need one.
            // This keeps the occluder count proportional to cave perimeter rather than
            // total wall area, which is critical for shadow-rendering performance.
            if tile_type == TileType::Wall {
                let pos = tile_to_world(x, y, map.width, map.height);
                let is_boundary = borders_floor(&map.tiles, map.width, map.height, x, y);

                let mut entity = commands.spawn((
                    Transform::from_xyz(pos.x, pos.y, 0.0),
                    RigidBody::Static,
                    Collider::rectangle(TILE_SIZE, TILE_SIZE),
                    Tile,
                ));
                if is_boundary {
                    entity.insert(LightOccluder2d {
                        shape: LightOccluder2dShape::Rectangle {
                            half_size: Vec2::splat(TILE_SIZE / 2.0),
                        },
                    });
                }
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

    // Insert all spawn point resources.
    let conv = |pos: (usize, usize)| tile_to_world(pos.0, pos.1, map.width, map.height);

    commands.insert_resource(PlayerSpawnPoint(conv(map.player_start)));
    commands.insert_resource(CampfireSpawnPoint(conv(map.campfire_spawn)));
    commands.insert_resource(SignpostSpawnPoint(conv(map.signpost_spawn)));
    commands.insert_resource(NpcSpawnPoint(conv(map.npc_spawn)));
    commands.insert_resource(WeaponChestSpawnPoint(conv(map.weapon_chest_spawn)));
    commands.insert_resource(KeyChestSpawnPoint(conv(map.key_chest_spawn)));
    commands.insert_resource(LockedDoorSpawnPoint {
        pos: conv(map.locked_door_pos),
        orientation: map.locked_door_orientation,
    });
    commands.insert_resource(LadderSpawnPoint(conv(map.ladder_pos)));
    commands.insert_resource(SpawnerSpawnPoint(conv(map.spawner_pos)));

    let walkable = map.tiles.iter().map(|t| matches!(t, TileType::Floor)).collect();
    commands.insert_resource(LevelTiles { width: map.width, height: map.height, walkable });

    // Suppress unused variable warning — rng is used only for tile colors.
    let _ = rng;
}

/// Returns `true` if the wall tile at `(x, y)` shares an edge or corner with at least one floor tile.
///
/// Used to skip spawning [`LightOccluder2d`] on deep interior walls that are
/// completely surrounded by other walls — no light ray from the cave floor can
/// ever reach those tiles, so an occluder there would cost GPU time for nothing.
fn borders_floor(tiles: &[TileType], width: usize, height: usize, x: usize, y: usize) -> bool {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < height {
                if matches!(tiles[ny as usize * width + nx as usize], TileType::Floor) {
                    return true;
                }
            }
        }
    }
    false
}

/// Converts a grid-space tile coordinate to a world-space position (tile center).
fn tile_to_world(x: usize, y: usize, width: usize, height: usize) -> Vec2 {
    Vec2::new(
        (x as f32 - width as f32 / 2.0) * TILE_SIZE,
        (y as f32 - height as f32 / 2.0) * TILE_SIZE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tiles(width: usize, height: usize, all_walkable: bool) -> LevelTiles {
        LevelTiles { width, height, walkable: vec![all_walkable; width * height] }
    }

    #[test]
    fn tile_to_world_positions_on_grid() {
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

    #[test]
    fn level_tiles_world_to_tile_round_trips() {
        let tiles = make_tiles(64, 64, true);
        for x in 0..64usize {
            for y in 0..64usize {
                let world = tiles.tile_to_world(x, y);
                let back = tiles.world_to_tile(world).expect("in-bounds tile round-trip failed");
                assert_eq!(back, (x, y), "round-trip mismatch at tile ({x}, {y})");
            }
        }
    }

    #[test]
    fn level_tiles_world_to_tile_out_of_bounds_returns_none() {
        let tiles = make_tiles(4, 4, true);
        assert!(tiles.world_to_tile(Vec2::new(9999.0, 9999.0)).is_none());
        assert!(tiles.world_to_tile(Vec2::new(-9999.0, -9999.0)).is_none());
    }

    #[test]
    fn level_tiles_is_walkable_respects_flags() {
        let mut tiles = make_tiles(3, 3, false);
        tiles.walkable[4] = true; // center tile (1, 1)
        assert!(tiles.is_walkable(1, 1));
        assert!(!tiles.is_walkable(0, 0));
        assert!(!tiles.is_walkable(99, 99)); // out of bounds
    }
}
