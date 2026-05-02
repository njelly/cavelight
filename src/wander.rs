use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use rand::Rng;

use crate::level::LevelTiles;

// ---------------------------------------------------------------------------
// A* pathfinder
// ---------------------------------------------------------------------------

/// Runs A* from `start` to `goal` on the walkability grid in `level`.
///
/// `extra_blocked` lists tile coordinates to treat as impassable in addition to
/// walls — used to route around dynamic obstacles such as the player's current tile.
///
/// Returns the path as tile coordinates, excluding `start` and including `goal`,
/// or `None` if no path exists.
pub fn astar(
    level: &LevelTiles,
    start: (usize, usize),
    goal: (usize, usize),
    extra_blocked: &[(usize, usize)],
) -> Option<Vec<(usize, usize)>> {
    if start == goal {
        return Some(vec![]);
    }

    // Min-heap: (f_cost, g_cost, tile)
    let mut open: BinaryHeap<(Reverse<u32>, u32, (usize, usize))> = BinaryHeap::new();
    let mut g_cost: HashMap<(usize, usize), u32> = HashMap::new();
    let mut came_from: HashMap<(usize, usize), (usize, usize)> = HashMap::new();

    g_cost.insert(start, 0);
    open.push((Reverse(heuristic(start, goal)), 0, start));

    while let Some((_, g, current)) = open.pop() {
        if current == goal {
            return Some(reconstruct_path(&came_from, current, start));
        }

        // Skip stale heap entries.
        if g > *g_cost.get(&current).unwrap_or(&u32::MAX) {
            continue;
        }

        for neighbor in cardinal_neighbors(current, level.width, level.height) {
            if !level.is_walkable(neighbor.0, neighbor.1) {
                continue;
            }
            if extra_blocked.contains(&neighbor) {
                continue;
            }

            let tentative_g = g + 1;
            if tentative_g < *g_cost.get(&neighbor).unwrap_or(&u32::MAX) {
                g_cost.insert(neighbor, tentative_g);
                came_from.insert(neighbor, current);
                let f = tentative_g + heuristic(neighbor, goal);
                open.push((Reverse(f), tentative_g, neighbor));
            }
        }
    }

    None
}

/// Manhattan distance heuristic for A*.
fn heuristic(a: (usize, usize), b: (usize, usize)) -> u32 {
    a.0.abs_diff(b.0) as u32 + a.1.abs_diff(b.1) as u32
}

/// Reconstructs the path from `came_from` map, returning tiles from after `start` to `goal`.
fn reconstruct_path(
    came_from: &HashMap<(usize, usize), (usize, usize)>,
    mut current: (usize, usize),
    start: (usize, usize),
) -> Vec<(usize, usize)> {
    let mut path = vec![current];
    while let Some(&prev) = came_from.get(&current) {
        if prev == start {
            break;
        }
        current = prev;
        path.push(current);
    }
    path.reverse();
    path
}

/// Returns the up-to-four in-bounds cardinal neighbours of `tile`.
pub fn cardinal_neighbors(tile: (usize, usize), width: usize, height: usize) -> Vec<(usize, usize)> {
    let (x, y) = tile;
    let mut out = Vec::with_capacity(4);
    if x > 0 { out.push((x - 1, y)); }
    if x + 1 < width { out.push((x + 1, y)); }
    if y > 0 { out.push((x, y - 1)); }
    if y + 1 < height { out.push((x, y + 1)); }
    out
}

// ---------------------------------------------------------------------------
// Destination picker
// ---------------------------------------------------------------------------

/// Picks a random walkable tile within `radius` tiles (Euclidean, in tile-space) of `origin`.
///
/// Returns `None` only in the degenerate case where no walkable tiles exist in the radius.
pub fn pick_random_walkable_in_radius(
    level: &LevelTiles,
    origin: (usize, usize),
    radius: usize,
    rng: &mut impl Rng,
) -> Option<(usize, usize)> {
    let r = radius as i32;
    let (ox, oy) = (origin.0 as i32, origin.1 as i32);
    let r_sq = (radius * radius) as i32;

    let candidates: Vec<(usize, usize)> = (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| dx * dx + dy * dy <= r_sq)
        .map(|(dx, dy)| (ox + dx, oy + dy))
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .map(|(x, y)| (x as usize, y as usize))
        .filter(|&(x, y)| level.is_walkable(x, y) && (x, y) != origin)
        .collect();

    if candidates.is_empty() {
        None
    } else {
        Some(candidates[rng.gen_range(0..candidates.len())])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_level(width: usize, height: usize, all_walkable: bool) -> LevelTiles {
        LevelTiles::from_walkable(width, height, vec![all_walkable; width * height])
    }

    fn open_level() -> LevelTiles {
        make_level(10, 10, true)
    }

    #[test]
    fn cardinal_neighbors_center_returns_four() {
        let neighbors = cardinal_neighbors((5, 5), 10, 10);
        assert_eq!(neighbors.len(), 4);
        assert!(neighbors.contains(&(4, 5)));
        assert!(neighbors.contains(&(6, 5)));
        assert!(neighbors.contains(&(5, 4)));
        assert!(neighbors.contains(&(5, 6)));
    }

    #[test]
    fn cardinal_neighbors_corner_returns_two() {
        let neighbors = cardinal_neighbors((0, 0), 10, 10);
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn astar_straight_line() {
        let level = open_level();
        let path = astar(&level, (0, 0), (3, 0), &[]).expect("expected a path");
        assert_eq!(path, vec![(1, 0), (2, 0), (3, 0)]);
    }

    #[test]
    fn astar_same_tile_returns_empty_path() {
        let level = open_level();
        let path = astar(&level, (3, 3), (3, 3), &[]).expect("expected empty path");
        assert!(path.is_empty());
    }

    #[test]
    fn astar_unreachable_returns_none() {
        // 3x1 level where the middle tile is a wall: [F][W][F]
        let mut level = make_level(3, 1, true);
        level.set_wall(1, 0);
        assert!(astar(&level, (0, 0), (2, 0), &[]).is_none());
    }

    #[test]
    fn astar_routes_around_extra_blocked() {
        // 3x3 open level. Block (1,0) dynamically — the path from (0,0) to (2,0)
        // must detour via row 1 rather than go straight.
        let level = open_level();
        let path = astar(&level, (0, 0), (2, 0), &[(1, 0)]).expect("path must exist via detour");
        assert!(!path.contains(&(1, 0)), "path must not include the blocked tile");
        assert_eq!(*path.last().unwrap(), (2, 0));
    }

    #[test]
    fn pick_random_walkable_in_radius_stays_in_radius() {
        let level = open_level();
        let origin = (5, 5);
        let radius = 3;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        for _ in 0..50 {
            let dest = pick_random_walkable_in_radius(&level, origin, radius, &mut rng)
                .expect("open level should always have candidates");
            let dx = dest.0 as i32 - origin.0 as i32;
            let dy = dest.1 as i32 - origin.1 as i32;
            assert!(
                dx * dx + dy * dy <= (radius * radius) as i32,
                "destination {:?} is outside radius {} from {:?}", dest, radius, origin
            );
        }
    }
}
