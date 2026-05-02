use std::collections::{HashSet, VecDeque};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::tile::TileType;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Which axis a door passage runs along.
///
/// Detected from the tiles adjacent to the door after generation:
/// - Walls on the **east and west** sides → corridor runs **north–south** → `NorthSouth`
/// - Walls on the **north and south** sides → corridor runs **east–west** → `EastWest`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorOrientation {
    /// Corridor runs north–south; walls flank the door to the east and west.
    NorthSouth,
    /// Corridor runs east–west; walls flank the door to the north and south.
    EastWest,
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
    /// Tile-space ladder_down spawn — inside the end room (leads to the next level).
    pub ladder_pos: (usize, usize),
    /// Tile-space ladder_up spawn — inside the start room (leads back to the surface).
    pub ladder_up_pos: (usize, usize),
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

/// Generates the first cave level using a fixed room graph and a randomly rotated layout.
///
/// ## Room graph
/// ```text
/// [WeaponChest] ──── [Start] ──── [LockedDoor] ──── [End/Ladder]
///                       │
///                  [KeyChest]
/// ```
///
/// ## Layout randomisation
/// A canonical base layout (end room south, side rooms east/west) is rotated by a
/// random multiple of 90° each generation. This gives 4 distinct orientations of the
/// room graph (end north/south/east/west) while keeping the graph topology fixed.
/// The side rooms are additionally randomly assigned between weapon and key.
///
/// ## Algorithm
/// 1. Pick a random rotation (0°/90°/180°/270°) and apply it to all room zone centres.
/// 2. Place four rectangular rooms within their zones (±4-tile jitter each).
/// 3. Carve L-shaped corridors (3 tiles wide) between connected rooms.
/// 4. Apply 3 cellular-automata smoothing passes to roughen the cave feel.
/// 5. Re-carve room interiors (inset by 1) so they stay open after CA.
/// 6. Re-carve corridor spines so CA cannot fully sever a passage.
/// 7. Enforce a 1-tile bottleneck at the locked door position:
///    - N/S corridors: wall the entire row at `door_y`.
///    - E/W corridors: wall the entire column at `door_x`.
/// 8. Flood-fill from the start room to wall off any disconnected scraps.
/// 9. Extract all spawn-point coordinates from within their respective rooms.
pub fn generate_level1(width: usize, height: usize, seed: u64) -> MapData {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut tiles = vec![TileType::Wall; width * height];

    // ------------------------------------------------------------------
    // 1. Choose layout rotation.
    //    The base layout has end south, side-A east, side-B west.
    //    Rotation 0 = base, 1 = 90° CCW (end east), 2 = 180° (end north),
    //    3 = 270° CCW (end west).  Rotation 0/2 → N/S corridor;
    //    rotation 1/3 → E/W corridor.
    // ------------------------------------------------------------------
    let rotation = rng.gen_range(0u8..4);
    let corridor_is_ns = rotation % 2 == 0;

    // Base zone offsets from map centre. The end room is 20 tiles south in
    // the canonical layout; the two side rooms flank start east and west.
    let map_cx = (width  / 2) as i32;
    let map_cy = (height / 2) as i32;

    // Converts a base (bx,by) offset to an absolute tile centre, applying
    // the chosen rotation.
    let to_abs = |bx: i32, by: i32| -> (usize, usize) {
        let (rx, ry) = rotate_offset(bx, by, rotation);
        (((map_cx + rx).max(0)) as usize, ((map_cy + ry).max(0)) as usize)
    };

    let (scx,  scy)  = to_abs(  0,   0);   // start — map centre
    let (ecx,  ecy)  = to_abs(  0, -20);   // end — 20 south in base
    let (s1cx, s1cy) = to_abs( 20,   0);   // side-A — 20 east in base
    let (s2cx, s2cy) = to_abs(-18,   0);   // side-B — 18 west in base

    // Randomly assign which side room holds the weapon chest vs the key chest.
    let ((wcx, wcy), (kcx, kcy)) = if rng.gen_bool(0.5) {
        ((s1cx, s1cy), (s2cx, s2cy))
    } else {
        ((s2cx, s2cy), (s1cx, s1cy))
    };

    // ------------------------------------------------------------------
    // 2. Place rooms within their zones (±4-tile jitter).
    // ------------------------------------------------------------------
    let start_room  = place_room(scx, scy,  18, 18, 4, &mut rng, width, height);
    let end_room    = place_room(ecx, ecy,  12, 12, 4, &mut rng, width, height);
    let weapon_room = place_room(wcx, wcy,  12, 12, 4, &mut rng, width, height);
    let key_room    = place_room(kcx, kcy,  12, 12, 4, &mut rng, width, height);

    // ------------------------------------------------------------------
    // 3. Carve rooms and corridors.
    // ------------------------------------------------------------------
    for room in [&start_room, &weapon_room, &key_room, &end_room] {
        carve_rect(&mut tiles, width, room.x, room.y, room.w, room.h);
    }

    let (start_cx, start_cy) = start_room.center();
    let (end_cx,   end_cy)   = end_room.center();
    let (weapon_cx, weapon_cy) = weapon_room.center();
    let (key_cx,    key_cy)    = key_room.center();

    // Branch rooms connect directly to start; end room connects via locked door.
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (weapon_cx, weapon_cy), 3);
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (key_cx,    key_cy),    3);
    carve_corridor(&mut tiles, width, height, (start_cx, start_cy), (end_cx,    end_cy),    3);

    // ------------------------------------------------------------------
    // 4. Locked door bottleneck position.
    //    N/S corridors: door sits on the vertical spine at end_cx, midway
    //    between the two room centres.
    //    E/W corridors: door sits on the horizontal spine at start_cy, midway
    //    along the east-west axis.
    // ------------------------------------------------------------------
    let (door_x, door_y) = if corridor_is_ns {
        (end_cx, (start_cy + end_cy) / 2)
    } else {
        ((start_cx + end_cx) / 2, start_cy)
    };

    // ------------------------------------------------------------------
    // 5. Cellular automata: 3 passes to roughen the corridors.
    //    Room interiors are re-carved after each pass so they stay open.
    // ------------------------------------------------------------------
    for _ in 0..3 {
        tiles = smooth_pass(&tiles, width, height);
        for room in [&start_room, &weapon_room, &key_room, &end_room] {
            if room.w > 2 && room.h > 2 {
                carve_rect(&mut tiles, width, room.x + 1, room.y + 1, room.w - 2, room.h - 2);
            }
        }
    }

    // ------------------------------------------------------------------
    // 6. Re-carve corridor spines (1 tile wide) so CA cannot sever a path.
    // ------------------------------------------------------------------
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (weapon_cx, weapon_cy));
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (key_cx,    key_cy));
    carve_spine(&mut tiles, width, height, (start_cx, start_cy), (end_cx,    end_cy));

    // ------------------------------------------------------------------
    // 7. Enforce solid border and 1-tile door bottleneck.
    //    For N/S corridors the entire row at door_y becomes wall.
    //    For E/W corridors the entire column at door_x becomes wall.
    //    Cardinal-only movement cannot bypass a complete row/column barrier.
    // ------------------------------------------------------------------
    enforce_border(&mut tiles, width, height);

    if corridor_is_ns {
        for x in 0..width {
            tiles[door_y * width + x] = TileType::Wall;
        }
    } else {
        for y in 0..height {
            tiles[y * width + door_x] = TileType::Wall;
        }
    }
    // Restore the single door tile so the room graph remains connected.
    tiles[door_y * width + door_x] = TileType::Floor;

    // ------------------------------------------------------------------
    // 8. Flood-fill from the start room center; wall off unreachable scraps.
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
    // 9. Extract spawn points — all guaranteed to be floor tiles.
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
    let ladder_up_pos = random_floor_in_room(
        &tiles, width, &start_room,
        &[player_start, campfire_spawn, signpost_spawn, npc_spawn],
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

    // Detect door orientation before tiles is moved into MapData.
    let locked_door_orientation = detect_door_orientation(&tiles, width, height, door_x, door_y);

    MapData {
        width,
        height,
        tiles,
        player_start,
        campfire_spawn,
        signpost_spawn,
        npc_spawn,
        ladder_up_pos,
        weapon_chest_spawn,
        key_chest_spawn,
        spawner_pos,
        locked_door_pos: (door_x, door_y),
        locked_door_orientation,
        ladder_pos,
    }
}

/// Applies a 90°-increment counter-clockwise rotation to a 2D tile offset.
///
/// Rotation 0 = unchanged, 1 = 90° CCW, 2 = 180°, 3 = 270° CCW.
fn rotate_offset(ox: i32, oy: i32, rotation: u8) -> (i32, i32) {
    match rotation % 4 {
        1 => (-oy,  ox),
        2 => (-ox, -oy),
        3 => ( oy, -ox),
        _ => ( ox,  oy),
    }
}

// ---------------------------------------------------------------------------
// Door orientation detection
// ---------------------------------------------------------------------------

/// Infers the [`DoorOrientation`] from the tiles immediately adjacent to the door.
///
/// Checks whether walls flank the door to the east+west (→ `NorthSouth` corridor) or
/// to the north+south (→ `EastWest` corridor). Falls back to `NorthSouth` if neither
/// pair is conclusive.
fn detect_door_orientation(
    tiles: &[TileType],
    width: usize,
    height: usize,
    door_x: usize,
    door_y: usize,
) -> DoorOrientation {
    let is_wall = |x: usize, y: usize| matches!(tiles[y * width + x], TileType::Wall);

    let east_wall  = door_x + 1 < width  && is_wall(door_x + 1, door_y);
    let west_wall  = door_x > 0          && is_wall(door_x - 1, door_y);
    let north_wall = door_y + 1 < height && is_wall(door_x, door_y + 1);
    let south_wall = door_y > 0          && is_wall(door_x, door_y - 1);

    if north_wall && south_wall {
        DoorOrientation::EastWest
    } else if east_wall && west_wall {
        DoorOrientation::NorthSouth
    } else {
        // Corridor is not cleanly axis-aligned at the door tile; default to north-south.
        DoorOrientation::NorthSouth
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
        // The two tiles that flank the door perpendicular to the corridor must be walls.
        // For NorthSouth (N/S corridor): walls are east and west of the door.
        // For EastWest (E/W corridor): walls are north and south of the door.
        let map = generate_level1(64, 64, 0);
        let (dx, dy) = map.locked_door_pos;
        match map.locked_door_orientation {
            DoorOrientation::NorthSouth => {
                if dx > 0 {
                    assert!(matches!(map.get(dx - 1, dy), TileType::Wall), "west of N/S door must be wall");
                }
                if dx + 1 < map.width {
                    assert!(matches!(map.get(dx + 1, dy), TileType::Wall), "east of N/S door must be wall");
                }
            }
            DoorOrientation::EastWest => {
                if dy > 0 {
                    assert!(matches!(map.get(dx, dy - 1), TileType::Wall), "south of E/W door must be wall");
                }
                if dy + 1 < map.height {
                    assert!(matches!(map.get(dx, dy + 1), TileType::Wall), "north of E/W door must be wall");
                }
            }
        }
    }

    #[test]
    fn locked_door_is_solid_barrier() {
        // For N/S corridors the entire row at door_y must be wall except the door tile.
        // For E/W corridors the entire column at door_x must be wall except the door tile.
        // Either way, cardinal-only movement cannot bypass the locked door.
        for seed in [0u64, 1, 42, 999, 12345] {
            let map = generate_level1(64, 64, seed);
            let (dx, dy) = map.locked_door_pos;
            match map.locked_door_orientation {
                DoorOrientation::NorthSouth => {
                    for x in 0..map.width {
                        if x == dx {
                            assert!(matches!(map.get(x, dy), TileType::Floor),
                                "seed {seed}: door tile ({dx},{dy}) must be floor");
                        } else {
                            assert!(matches!(map.get(x, dy), TileType::Wall),
                                "seed {seed}: N/S barrier row {dy} tile x={x} must be wall");
                        }
                    }
                }
                DoorOrientation::EastWest => {
                    for y in 0..map.height {
                        if y == dy {
                            assert!(matches!(map.get(dx, y), TileType::Floor),
                                "seed {seed}: door tile ({dx},{dy}) must be floor");
                        } else {
                            assert!(matches!(map.get(dx, y), TileType::Wall),
                                "seed {seed}: E/W barrier col {dx} tile y={y} must be wall");
                        }
                    }
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

    // Helpers for detect_door_orientation tests: build a small 5×5 tile grid.
    fn wall_grid() -> (Vec<TileType>, usize, usize) {
        (vec![TileType::Wall; 5 * 5], 5, 5)
    }

    fn set_floor(tiles: &mut Vec<TileType>, width: usize, x: usize, y: usize) {
        tiles[y * width + x] = TileType::Floor;
    }

    #[test]
    fn detect_orientation_east_west_walls_gives_northsouth() {
        // Door at (2,2); walls to east (3,2) and west (1,2) → NorthSouth corridor.
        let (mut tiles, w, h) = wall_grid();
        set_floor(&mut tiles, w, 2, 2); // door tile
        set_floor(&mut tiles, w, 2, 1); // floor to south
        set_floor(&mut tiles, w, 2, 3); // floor to north
        // (1,2) and (3,2) remain walls
        assert_eq!(detect_door_orientation(&tiles, w, h, 2, 2), DoorOrientation::NorthSouth);
    }

    #[test]
    fn detect_orientation_north_south_walls_gives_eastwest() {
        // Door at (2,2); walls to north (2,3) and south (2,1) → EastWest corridor.
        let (mut tiles, w, h) = wall_grid();
        set_floor(&mut tiles, w, 2, 2); // door tile
        set_floor(&mut tiles, w, 1, 2); // floor to west
        set_floor(&mut tiles, w, 3, 2); // floor to east
        // (2,1) and (2,3) remain walls
        assert_eq!(detect_door_orientation(&tiles, w, h, 2, 2), DoorOrientation::EastWest);
    }

    #[test]
    fn locked_door_orientation_matches_tile_neighbors() {
        // Verify that across multiple seeds, the reported orientation matches
        // what detect_door_orientation would conclude from the actual tiles.
        for seed in [0u64, 1, 42, 999, 12345] {
            let map = generate_level1(64, 64, seed);
            let (dx, dy) = map.locked_door_pos;
            let detected = detect_door_orientation(&map.tiles, map.width, map.height, dx, dy);
            assert_eq!(
                map.locked_door_orientation, detected,
                "seed {seed}: stored orientation doesn't match tile neighbors"
            );
        }
    }
}
