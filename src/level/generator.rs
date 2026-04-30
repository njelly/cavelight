use std::collections::{HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::tile::TileType;

/// The generated map data returned by [`generate_cave`].
pub struct MapData {
    pub width: usize,
    pub height: usize,
    /// Row-major flat tile array: index = y * width + x.
    pub tiles: Vec<TileType>,
    /// Grid-space coordinate of the recommended player spawn, always a floor tile.
    pub player_start: (usize, usize),
}

impl MapData {
    /// Returns the tile at grid position `(x, y)`.
    pub fn get(&self, x: usize, y: usize) -> TileType {
        self.tiles[y * self.width + x]
    }
}

/// Generates a cave map using cellular automata.
///
/// Algorithm:
/// 1. Randomly fill the map (~45% walls).
/// 2. Run 5 smoothing passes: a tile becomes wall if ≥5 of its 8 neighbors are walls.
/// 3. Flood-fill to find the largest connected floor region; wall off all other regions.
/// 4. Return the floor tile nearest the map center as the player start position.
pub fn generate_cave(width: usize, height: usize) -> MapData {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut rng = StdRng::seed_from_u64(seed);

    let mut tiles = random_fill(width, height, &mut rng);

    for _ in 0..5 {
        tiles = smooth_pass(&tiles, width, height);
    }

    let player_start = isolate_largest_region(&mut tiles, width, height);

    MapData { width, height, tiles, player_start }
}

/// Fills the map randomly with ~45% walls. Border tiles are always walls.
fn random_fill(width: usize, height: usize, rng: &mut impl Rng) -> Vec<TileType> {
    let mut tiles: Vec<TileType> = (0..width * height)
        .map(|_| if rng.gen_bool(0.45) { TileType::Wall } else { TileType::Floor })
        .collect();

    // Force solid border
    for x in 0..width {
        tiles[x] = TileType::Wall;
        tiles[(height - 1) * width + x] = TileType::Wall;
    }
    for y in 0..height {
        tiles[y * width] = TileType::Wall;
        tiles[y * width + (width - 1)] = TileType::Wall;
    }

    tiles
}

/// Runs one cellular automata smoothing pass.
///
/// A tile becomes wall if 5 or more of its 8 neighbors (including out-of-bounds) are walls.
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

/// Returns how many of the 8 neighbors of `(x, y)` are walls.
/// Out-of-bounds neighbors count as walls.
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
                count += 1; // out of bounds = wall
            } else if matches!(tiles[ny as usize * width + nx as usize], TileType::Wall) {
                count += 1;
            }
        }
    }
    count
}

/// Finds all connected floor regions via BFS, walls off all but the largest, and returns
/// the floor tile in the surviving region closest to the map center.
fn isolate_largest_region(tiles: &mut Vec<TileType>, width: usize, height: usize) -> (usize, usize) {
    let mut visited = vec![false; width * height];
    let mut regions: Vec<Vec<usize>> = Vec::new();

    for start in 0..tiles.len() {
        if matches!(tiles[start], TileType::Floor) && !visited[start] {
            regions.push(bfs_region(tiles, width, height, start, &mut visited));
        }
    }

    let largest = regions.into_iter().max_by_key(|r| r.len()).unwrap_or_default();
    let largest_set: HashSet<usize> = largest.iter().copied().collect();

    // Wall off every floor tile not in the largest region
    for (idx, tile) in tiles.iter_mut().enumerate() {
        if matches!(tile, TileType::Floor) && !largest_set.contains(&idx) {
            *tile = TileType::Wall;
        }
    }

    // Player starts at the floor tile nearest the map center
    let cx = width / 2;
    let cy = height / 2;
    let start_idx = largest_set
        .iter()
        .copied()
        .min_by_key(|&idx| {
            let dx = (idx % width) as i32 - cx as i32;
            let dy = (idx / width) as i32 - cy as i32;
            dx * dx + dy * dy
        })
        .unwrap_or(cy * width + cx);

    (start_idx % width, start_idx / width)
}

/// BFS flood fill from `start`, returning all reachable floor tile indices.
fn bfs_region(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_cave_correct_dimensions() {
        let map = generate_cave(64, 64);
        assert_eq!(map.tiles.len(), 64 * 64);
        assert_eq!(map.width, 64);
        assert_eq!(map.height, 64);
    }

    #[test]
    fn player_start_is_floor_tile() {
        let map = generate_cave(64, 64);
        let (sx, sy) = map.player_start;
        assert!(matches!(map.get(sx, sy), TileType::Floor), "player start must be a floor tile");
    }

    #[test]
    fn borders_are_always_walls() {
        let map = generate_cave(64, 64);
        for x in 0..map.width {
            assert!(matches!(map.get(x, 0), TileType::Wall), "top border must be wall");
            assert!(matches!(map.get(x, map.height - 1), TileType::Wall), "bottom border must be wall");
        }
        for y in 0..map.height {
            assert!(matches!(map.get(0, y), TileType::Wall), "left border must be wall");
            assert!(matches!(map.get(map.width - 1, y), TileType::Wall), "right border must be wall");
        }
    }

    #[test]
    fn all_floor_tiles_are_connected() {
        let map = generate_cave(64, 64);
        let floor_tiles: Vec<usize> = map
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, TileType::Floor))
            .map(|(i, _)| i)
            .collect();

        if floor_tiles.is_empty() {
            return; // Degenerate map; generation guarantees at least one floor tile in practice
        }

        // BFS from the first floor tile — should reach all floor tiles
        let mut visited = vec![false; map.width * map.height];
        let reachable = bfs_region(&map.tiles, map.width, map.height, floor_tiles[0], &mut visited);

        assert_eq!(
            reachable.len(),
            floor_tiles.len(),
            "all floor tiles must be connected after generation"
        );
    }

    #[test]
    fn count_wall_neighbors_all_walls() {
        let tiles = vec![TileType::Wall; 9];
        assert_eq!(count_wall_neighbors(&tiles, 3, 3, 1, 1), 8);
    }

    #[test]
    fn count_wall_neighbors_all_floor() {
        let tiles = vec![TileType::Floor; 9];
        assert_eq!(count_wall_neighbors(&tiles, 3, 3, 1, 1), 0);
    }

    #[test]
    fn count_wall_neighbors_corner_counts_oob_as_walls() {
        let tiles = vec![TileType::Floor; 9];
        // Corner (0,0): 5 of its 8 neighbors are out-of-bounds
        assert_eq!(count_wall_neighbors(&tiles, 3, 3, 0, 0), 5);
    }

    #[test]
    fn smooth_pass_fills_isolated_floor() {
        // A floor tile completely surrounded by walls should become a wall
        let mut tiles = vec![TileType::Wall; 9];
        tiles[4] = TileType::Floor; // center
        let result = smooth_pass(&tiles, 3, 3);
        assert!(matches!(result[4], TileType::Wall));
    }
}
