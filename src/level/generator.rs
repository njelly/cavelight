use std::collections::{HashSet, VecDeque};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::tile::TileType;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Which axis a door passage blocks.
///
/// Determines which door sprite variant is shown (e.g. `door_northsouth_closed`).
/// Additional orientations (e.g. `EastWest`) will be added when levels with
/// east–west locked-door corridors are introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorOrientation {
    /// Corridor runs north–south; the door bar is horizontal (blocks N-S movement).
    NorthSouth,
}

/// An axis-aligned rectangular room in tile coordinates.
#[derive(Debug, Clone)]
pub struct RoomRect {
    /// Left tile column (inclusive).
    pub x: usize,
    /// Bottom tile row (inclusive).
    pub y: usize,
    /// Width in tiles.
    pub w: usize,
    /// Height in tiles.
    pub h: usize,
}

impl RoomRect {
    /// Returns the center tile of the room (rounded toward origin).
    pub fn center(&self) -> (usize, usize) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// All output produced by [`generate_level1`].
pub struct MapData {
    pub width: usize,
    pub height: usize,
    /// Row-major flat tile array: index = `y * width + x`.
    pub tiles: Vec<TileType>,
    /// Tile-space player spawn — always inside the start room.
    pub player_start: (usize, usize),
    /// Tile-space campfire spawn — always inside the start room.
    pub campfire_spawn: (usize, usize),
    /// Tile-space signpost spawn — always inside the start room.
    pub signpost_spawn: (usize, usize),
    /// Tile-space NPC spawn — always inside the start room.
    pub npc_spawn: (usize, usize),
    /// Tile-space weapon chest spawn (bow + arrows) — inside the weapon side room.
    pub weapon_chest_spawn: (usize, usize),
    /// Tile-space key chest spawn — inside the key side room.
    pub key_chest_spawn: (usize, usize),
    /// Tile-space position of the locked door entity (a 1-tile bottleneck in the corridor).
    pub locked_door_pos: (usize, usize),
    /// Whether the locked-door corridor runs N-S or E-W.
    pub locked_door_orientation: DoorOrientation,
    /// Tile-space ladder spawn — inside the end room.
    pub ladder_pos: (usize, usize),
    /// Tile-space spawner position — inside the key chest room.
    pub spawner_pos: (usize, usize),
}

impl MapData {
    /// Returns the tile type at grid position `(x, y)`.
    pub fn get(&self, x: usize, y: usize) -> TileType {
        self.tiles[y * self.width + x]
    }
}

// ---------------------------------------------------------------------------
// Level 1 entry point
// ---------------------------------------------------------------------------

/// Generates the first cave level using a fixed room graph.
///
/// ## Room graph
/// ```text
/// [WeaponChest] ──── [Start] ──── [LockedDoor] ──── [End/Ladder]
///                       │
///                  [KeyChest]
/// ```
///
/// ## Algorithm
/// 1. Place four rectangular rooms at randomised positions within fixed zones.
/// 2. Carve L-shaped corridors (3 tiles wide) between connected rooms.
/// 3. Apply 3 cellular-automata smoothing passes to roughen the cave feel.
/// 4. Re-carve room interiors (inset by 1) so they stay open after CA.
/// 5. Enforce a 1-tile bottleneck at the locked door position.
/// 6. Flood-fill from the start room to wall off any disconnected scraps.
/// 7. Extract all spawn-point coordinates from within their respective rooms.
pub fn generate_level1(width: usize, height: usize, seed: u64) -> MapData {
    let mut rng = StdRng::seed_from_u64(seed);

    let mut tiles = vec![TileType::Wall; width * height];

    // ------------------------------------------------------------------
    // 1. Place rooms
    //    Zones are chosen so rooms never overlap and always fit in 64×64.
    //    Each center is jittered ±jitter tiles for variety.
    // ------------------------------------------------------------------
    let start_room = place_room(32, 36, 18, 18, 3, &mut rng, width, height);
    let weapon_room = place_room(52, 24, 12, 12, 3, &mut rng, width, height);
    let key_room = place_room(14, 52, 12, 12, 3, &mut rng, width, height);
    let end_room = place_room(32, 11, 12, 12, 3, &mut rng, width, height);

    // ------------------------------------------------------------------
    // 2. Carve rooms and corridors
    // ------------------------------------------------------------------
    for room in [&start_room, &weapon_room, &key_room, &end_room] {
        carve_rect(&mut tiles, width, room.x, room.y, room.w, room.h);
    }

    let (start_cx, start_cy) = start_room.center();
    let (weapon_cx, weapon_cy) = weapon_room.center();
    let (key_cx, key_cy) = key_room.center();
    let (end_cx, end_cy) = end_room.center();

    // Weapon and key rooms are side branches off the start room.
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (weapon_cx, weapon_cy), 3);
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (key_cx, key_cy), 3);
    // End room is connected through the locked door corridor.
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (end_cx, end_cy), 3);

    // ------------------------------------------------------------------
    // 3. Locked door: 1-tile bottleneck on the Start→End corridor.
    //    The corridor's vertical segment runs at x = end_cx.
    //    The door sits at the midpoint between the two room centers.
    // ------------------------------------------------------------------
    let door_x = end_cx;
    let door_y = (start_cy + end_cy) / 2;

    // ------------------------------------------------------------------
    // 4. Cellular automata: 3 passes to rough up the corridors.
    //    Room interiors are re-carved after each pass so they stay open.
    // ------------------------------------------------------------------
    for _ in 0..3 {
        tiles = smooth_pass(&tiles, width, height);
        // Re-carve room interiors (inset 1 tile so edges can naturalize).
        for room in [&start_room, &weapon_room, &key_room, &end_room] {
            if room.w > 2 && room.h > 2 {
                carve_rect(&mut tiles, width, room.x + 1, room.y + 1, room.w - 2, room.h - 2);
            }
        }
    }

    // ------------------------------------------------------------------
    // 5. Safety: re-carve corridor spines (1 tile wide) so CA cannot
    //    sever a corridor entirely.
    // ------------------------------------------------------------------
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (weapon_cx, weapon_cy));
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (key_cx, key_cy));
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (end_cx, end_cy));

    // ------------------------------------------------------------------
    // 6. Enforce solid border and door bottleneck.
    // ------------------------------------------------------------------
    enforce_border(&mut tiles, width, height);

    // Wall the entire row at door_y, then restore exactly the door tile.
    // Cardinal-only movement means a complete horizontal wall is an unbreakable barrier —
    // no matter how much CA widens the corridor, there is no path around the door.
    for x in 0..width {
        tiles[door_y * width + x] = TileType::Wall;
    }
    tiles[door_y * width + door_x] = TileType::Floor;

    // ------------------------------------------------------------------
    // 7. Flood-fill from the start room center; wall off unreachable scraps.
    // ------------------------------------------------------------------
    let flood_start = start_cy * width + start_cx;
    if matches!(tiles[flood_start], TileType::Floor) {
        let mut visited = vec![false; width * height];
        let reachable = bfs_region(&tiles, width, height, flood_start, &mut visited);
        let reachable_set: HashSet<usize> = reachable.into_iter().collect();
        for (i, tile) in tiles.iter_mut().enumerate() {
            if matches!(tile, TileType::Floor) && !reachable_set.contains(&i) {
                *tile = TileType::Wall;
            }
        }
    }

    // ------------------------------------------------------------------
    // 8. Extract spawn points — all guaranteed to be floor tiles.
    // ------------------------------------------------------------------
    let player_start =
        random_floor_in_room(&tiles, width, &start_room, &[], &mut rng);
    let campfire_spawn =
        random_floor_in_room(&tiles, width, &start_room, &[player_start], &mut rng);
    let signpost_spawn =
        random_floor_in_room(&tiles, width, &start_room, &[player_start, campfire_spawn], &mut rng);
    let npc_spawn = random_floor_in_room(
        &tiles, width, &start_room,
        &[player_start, campfire_spawn, signpost_spawn],
        &mut rng,
    );
    let weapon_chest_spawn =
        random_floor_in_room(&tiles, width, &weapon_room, &[], &mut rng);
    let key_chest_spawn =
        random_floor_in_room(&tiles, width, &key_room, &[], &mut rng);
    let spawner_pos =
        random_floor_in_room(&tiles, width, &key_room, &[key_chest_spawn], &mut rng);
    let ladder_pos =
        random_floor_in_room(&tiles, width, &end_room, &[], &mut rng);

    MapData {
        width,
        height,
        tiles,
        player_start,
        campfire_spawn,
        signpost_spawn,
        npc_spawn,
        weapon_chest_spawn,
        key_chest_spawn,
        spawner_pos,
        locked_door_pos: (door_x, door_y),
        locked_door_orientation: DoorOrientation::NorthSouth,
        ladder_pos,
    }
}

// ---------------------------------------------------------------------------
// Room placement
// ---------------------------------------------------------------------------

/// Places a room centered at `(cx, cy)` with size `(w, h)`, jittered by ±`jitter` tiles.
///
/// The result is clamped so the room always fits within the map with at least a 2-tile
/// wall border on every side.
fn place_room(
    cx: usize,
    cy: usize,
    w: usize,
    h: usize,
    jitter: usize,
    rng: &mut impl Rng,
    map_w: usize,
    map_h: usize,
) -> RoomRect {
    let jx = rng.gen_range(0..=(2 * jitter)) as i32 - jitter as i32;
    let jy = rng.gen_range(0..=(2 * jitter)) as i32 - jitter as i32;

    let min_x = (w / 2 + 2) as i32;
    let max_x = (map_w as i32 - w as i32 / 2 - 2).max(min_x);
    let min_y = (h / 2 + 2) as i32;
    let max_y = (map_h as i32 - h as i32 / 2 - 2).max(min_y);

    let actual_cx = (cx as i32 + jx).clamp(min_x, max_x) as usize;
    let actual_cy = (cy as i32 + jy).clamp(min_y, max_y) as usize;

    let x = actual_cx.saturating_sub(w / 2).max(2);
    let y = actual_cy.saturating_sub(h / 2).max(2);
    let w = w.min(map_w.saturating_sub(x + 2));
    let h = h.min(map_h.saturating_sub(y + 2));

    RoomRect { x, y, w, h }
}

// ---------------------------------------------------------------------------
// Tile carving
// ---------------------------------------------------------------------------

/// Sets every tile in an axis-aligned rectangle to [`TileType::Floor`].
fn carve_rect(tiles: &mut [TileType], width: usize, x: usize, y: usize, w: usize, h: usize) {
    for row in y..y + h {
        for col in x..x + w {
            let idx = row * width + col;
            if idx < tiles.len() {
                tiles[idx] = TileType::Floor;
            }
        }
    }
}

/// Carves an L-shaped corridor of `thickness` tiles wide between two points.
///
/// The path goes **horizontal first** (from `from` to `(to.x, from.y)`) and
/// then **vertical** (from `(to.x, from.y)` to `to`). The corridor is centred on
/// the spine so `thickness = 3` produces one tile either side of the spine.
fn carve_corridor(
    tiles: &mut Vec<TileType>,
    width: usize,
    height: usize,
    from: (usize, usize),
    to: (usize, usize),
    thickness: usize,
) {
    let half = thickness / 2;

    // Horizontal segment at from.y
    let x0 = from.0.min(to.0);
    let x1 = from.0.max(to.0);
    for x in x0..=x1 {
        let y_lo = from.1.saturating_sub(half);
        let y_hi = (from.1 + half + 1).min(height);
        for y in y_lo..y_hi {
            let idx = y * width + x;
            if idx < tiles.len() {
                tiles[idx] = TileType::Floor;
            }
        }
    }

    // Vertical segment at to.x
    let y0 = from.1.min(to.1);
    let y1 = from.1.max(to.1);
    for y in y0..=y1 {
        let x_lo = to.0.saturating_sub(half);
        let x_hi = (to.0 + half + 1).min(width);
        for x in x_lo..x_hi {
            let idx = y * width + x;
            if idx < tiles.len() {
                tiles[idx] = TileType::Floor;
            }
        }
    }
}

/// Carves a guaranteed 1-tile-wide spine through an L-shaped path.
///
/// Called after CA smoothing to ensure corridors are never completely blocked.
fn carve_spine(
    tiles: &mut Vec<TileType>,
    width: usize,
    height: usize,
    from: (usize, usize),
    to: (usize, usize),
) {
    // Horizontal segment
    let x0 = from.0.min(to.0);
    let x1 = from.0.max(to.0);
    for x in x0..=x1 {
        let idx = from.1 * width + x;
        if from.1 < height && idx < tiles.len() {
            tiles[idx] = TileType::Floor;
        }
    }
    // Vertical segment
    let y0 = from.1.min(to.1);
    let y1 = from.1.max(to.1);
    for y in y0..=y1 {
        let idx = y * width + to.0;
        if to.0 < width && idx < tiles.len() {
            tiles[idx] = TileType::Floor;
        }
    }
}

/// Forces all border tiles (row 0, row height-1, col 0, col width-1) to wall.
fn enforce_border(tiles: &mut Vec<TileType>, width: usize, height: usize) {
    for x in 0..width {
        tiles[x] = TileType::Wall;
        tiles[(height - 1) * width + x] = TileType::Wall;
    }
    for y in 0..height {
        tiles[y * width] = TileType::Wall;
        tiles[y * width + width - 1] = TileType::Wall;
    }
}

// ---------------------------------------------------------------------------
// Cellular automata
// ---------------------------------------------------------------------------

/// One CA smoothing pass: a tile becomes wall when 5 or more of its 8 neighbours are walls.
///
/// Out-of-bounds neighbours count as walls.
fn smooth_pass(tiles: &[TileType], width: usize, height: usize) -> Vec<TileType> {
    let mut next = vec![TileType::Wall; width * height];
    for y in 0..height {
        for x in 0..width {
            let walls = count_wall_neighbors(tiles, width, height, x, y);
            next[y * width + x] = if walls >= 5 { TileType::Wall } else { TileType::Floor };
        }
    }
    next
}

/// Counts how many of the 8 neighbours of `(x, y)` are walls.
/// Out-of-bounds positions are treated as walls.
fn count_wall_neighbors(tiles: &[TileType], width: usize, height: usize, x: usize, y: usize) -> u32 {
    let mut count = 0;
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                count += 1;
            } else if matches!(tiles[ny as usize * width + nx as usize], TileType::Wall) {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Flood fill
// ---------------------------------------------------------------------------

/// BFS flood-fill from `start_idx`, returning all reachable floor tile indices.
pub fn bfs_region(
    tiles: &[TileType],
    width: usize,
    height: usize,
    start: usize,
    visited: &mut Vec<bool>,
) -> Vec<usize> {
    let mut region = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);
    visited[start] = true;

    while let Some(idx) = queue.pop_front() {
        region.push(idx);
        let x = idx % width;
        let y = idx / width;
        for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                let nidx = ny as usize * width + nx as usize;
                if matches!(tiles[nidx], TileType::Floor) && !visited[nidx] {
                    visited[nidx] = true;
                    queue.push_back(nidx);
                }
            }
        }
    }

    region
}

// ---------------------------------------------------------------------------
// Spawn-point helpers
// ---------------------------------------------------------------------------

/// Picks a random floor tile inside `room` that is not in `reserved`.
///
/// Falls back to the room center in the degenerate case where every floor tile is taken.
pub fn random_floor_in_room(
    tiles: &[TileType],
    width: usize,
    room: &RoomRect,
    reserved: &[(usize, usize)],
    rng: &mut impl Rng,
) -> (usize, usize) {
    let candidates: Vec<(usize, usize)> = (room.y..room.y + room.h)
        .flat_map(|y| (room.x..room.x + room.w).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            matches!(tiles[y * width + x], TileType::Floor) && !reserved.contains(&(x, y))
        })
        .collect();

    if candidates.is_empty() {
        room.center()
    } else {
        candidates[rng.gen_range(0..candidates.len())]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_level1_correct_dimensions() {
        let map = generate_level1(64, 64, 0);
        assert_eq!(map.tiles.len(), 64 * 64);
        assert_eq!(map.width, 64);
        assert_eq!(map.height, 64);
    }

    #[test]
    fn player_start_is_floor_tile() {
        let map = generate_level1(64, 64, 0);
        let (sx, sy) = map.player_start;
        assert!(matches!(map.get(sx, sy), TileType::Floor), "player start must be floor");
    }

    #[test]
    fn all_spawn_points_are_floor_tiles() {
        let map = generate_level1(64, 64, 0);
        let spawns = [
            ("campfire", map.campfire_spawn),
            ("signpost", map.signpost_spawn),
            ("npc", map.npc_spawn),
            ("weapon_chest", map.weapon_chest_spawn),
            ("key_chest", map.key_chest_spawn),
            ("spawner", map.spawner_pos),
            ("ladder", map.ladder_pos),
        ];
        for (name, (x, y)) in spawns {
            assert!(matches!(map.get(x, y), TileType::Floor), "{name} spawn must be floor");
        }
    }

    #[test]
    fn locked_door_tile_is_floor() {
        let map = generate_level1(64, 64, 0);
        let (dx, dy) = map.locked_door_pos;
        assert!(matches!(map.get(dx, dy), TileType::Floor), "door tile must be floor");
    }

    #[test]
    fn locked_door_neighbours_are_walls() {
        let map = generate_level1(64, 64, 0);
        let (dx, dy) = map.locked_door_pos;
        if dx > 0 {
            assert!(
                matches!(map.get(dx - 1, dy), TileType::Wall),
                "tile left of door must be wall"
            );
        }
        if dx + 1 < map.width {
            assert!(
                matches!(map.get(dx + 1, dy), TileType::Wall),
                "tile right of door must be wall"
            );
        }
    }

    #[test]
    fn locked_door_row_is_solid_wall_barrier() {
        // Every tile in the door row must be a wall except the single door tile.
        // A complete horizontal barrier guarantees cardinal-only movement cannot
        // bypass the locked door regardless of how CA shaped the corridor.
        for seed in [0u64, 1, 42, 999, 12345] {
            let map = generate_level1(64, 64, seed);
            let (dx, dy) = map.locked_door_pos;
            for x in 0..map.width {
                if x == dx {
                    assert!(
                        matches!(map.get(x, dy), TileType::Floor),
                        "seed {seed}: door tile ({dx},{dy}) must be floor"
                    );
                } else {
                    assert!(
                        matches!(map.get(x, dy), TileType::Wall),
                        "seed {seed}: row {dy} tile x={x} must be wall (bypass route)"
                    );
                }
            }
        }
    }

    #[test]
    fn borders_are_always_walls() {
        let map = generate_level1(64, 64, 0);
        for x in 0..map.width {
            assert!(matches!(map.get(x, 0), TileType::Wall));
            assert!(matches!(map.get(x, map.height - 1), TileType::Wall));
        }
        for y in 0..map.height {
            assert!(matches!(map.get(0, y), TileType::Wall));
            assert!(matches!(map.get(map.width - 1, y), TileType::Wall));
        }
    }

    #[test]
    fn all_floor_tiles_are_connected() {
        let map = generate_level1(64, 64, 0);
        let floor_tiles: Vec<usize> = map
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, TileType::Floor))
            .map(|(i, _)| i)
            .collect();

        if floor_tiles.is_empty() {
            return;
        }

        let mut visited = vec![false; map.width * map.height];
        let reachable = bfs_region(&map.tiles, map.width, map.height, floor_tiles[0], &mut visited);
        assert_eq!(
            reachable.len(),
            floor_tiles.len(),
            "all floor tiles must be connected"
        );
    }

    #[test]
    fn smooth_pass_fills_isolated_floor() {
        let mut tiles = vec![TileType::Wall; 9];
        tiles[4] = TileType::Floor; // isolated center in 3×3 all-wall grid
        let result = smooth_pass(&tiles, 3, 3);
        assert!(matches!(result[4], TileType::Wall));
    }

    #[test]
    fn count_wall_neighbors_corner_counts_oob_as_walls() {
        let tiles = vec![TileType::Floor; 9];
        assert_eq!(count_wall_neighbors(&tiles, 3, 3, 0, 0), 5);
    }

    #[test]
    fn place_room_stays_in_bounds() {
        let mut rng = StdRng::seed_from_u64(0);
        for _ in 0..20 {
            let room = place_room(32, 32, 14, 14, 4, &mut rng, 64, 64);
            assert!(room.x >= 2, "room left edge must clear border");
            assert!(room.y >= 2, "room top edge must clear border");
            assert!(room.x + room.w <= 62, "room right edge must clear border");
            assert!(room.y + room.h <= 62, "room bottom edge must clear border");
        }
    }

    #[test]
    fn random_floor_in_room_avoids_reserved() {
        // 5×5 all-floor room
        let width = 10usize;
        let tiles = vec![TileType::Floor; width * width];
        let room = RoomRect { x: 2, y: 2, w: 5, h: 5 };
        let reserved = vec![(4, 4)];
        let mut rng = StdRng::seed_from_u64(42);

        for _ in 0..50 {
            let pos = random_floor_in_room(&tiles, width, &room, &reserved, &mut rng);
            assert_ne!(pos, (4, 4), "must avoid reserved tile");
            assert!(pos.0 >= 2 && pos.0 < 7, "must be inside room x range");
            assert!(pos.1 >= 2 && pos.1 < 7, "must be inside room y range");
        }
    }
}
