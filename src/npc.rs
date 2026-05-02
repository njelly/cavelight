use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use avian2d::prelude::*;
use bevy::prelude::*;
use rand::{Rng, thread_rng};

use crate::grid_mover::{GridMover, GridMoverSet, snap_to_grid};
use crate::level::{LevelTiles, NpcSpawnPoint};
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Tile radius within which the NPC considers candidate wander destinations.
///
/// Destinations are filtered by **Euclidean** distance (in tiles) so only nearby
/// tiles are candidates, even if A* would route far around a wall to reach them.
const WANDER_RADIUS: usize = 6;

/// Maximum number of A* steps the NPC will follow toward a destination.
///
/// Paths longer than this are truncated, preventing the NPC from routing all
/// the way around a long thin wall just because the destination is within radius.
const WANDER_MAX_PATH_STEPS: usize = 10;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks a non-player character entity.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Npc;

/// Drives A*-planned wander behaviour for an entity with a [`GridMover`].
///
/// On each timer tick the NPC picks a random walkable tile within [`WANDER_RADIUS`]
/// tiles (Euclidean distance), runs A* to that tile, truncates the resulting path to
/// at most [`WANDER_MAX_PATH_STEPS`] steps, and follows it one grid step per frame.
///
/// Truncating by path length — rather than destination distance — ensures the NPC
/// will not route a long way around a wall simply because the goal tile is
/// geographically close.
///
/// Attach alongside [`GridMover`] on any entity that should roam autonomously.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct Wander {
    /// Controls how often a new destination is chosen.
    pub timer: Timer,
    /// World-space waypoints for the current path (front = next step).
    #[reflect(ignore)]
    pub path: VecDeque<Vec2>,
    /// Tile-space goal of the current path, if any.
    pub destination: Option<(usize, usize)>,
}

impl Wander {
    /// Creates a new `Wander` with the given re-path interval and an empty path.
    pub fn new(interval_secs: f32) -> Self {
        Self {
            timer: Timer::from_seconds(interval_secs, TimerMode::Repeating),
            path: VecDeque::new(),
            destination: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Spawns the NPC and registers the wander system.
pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Npc>()
            .register_type::<Wander>()
            .add_systems(Startup, spawn_npc)
            .add_systems(Update, update_wander.before(GridMoverSet));
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the female NPC at [`NpcSpawnPoint`] with a wander controller and a grid mover.
///
/// A kinematic rigid body and collider make the NPC solid so the player and other
/// entities cannot walk through her, and so she registers in [`GridMover`]'s spatial
/// query when other entities try to enter her tile.
fn spawn_npc(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<NpcSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Npc,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 64,
            },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("npc_female", true),
        GridMover::new(GRID_SIZE),
        RigidBody::Kinematic,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
        Wander::new(1.2),
    ));
}

// ---------------------------------------------------------------------------
// A* pathfinder
// ---------------------------------------------------------------------------

/// Runs A* from `start` to `goal` on the walkability grid in `level`.
///
/// `extra_blocked` lists tile coordinates to treat as impassable in addition to
/// walls — used to route around the player's current tile when the next planned
/// step would collide with them.
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
// Wander update
// ---------------------------------------------------------------------------

/// Ticks the wander timer, advances path waypoints, and steers the NPC via [`GridMover`].
///
/// Each frame:
/// 1. Pop waypoints that the NPC has already reached (within 0.5 units).
/// 2. On timer fire (or empty path), pick a new destination tile within
///    [`WANDER_RADIUS`] tiles and plan an A* path. The path is truncated to
///    [`WANDER_MAX_PATH_STEPS`] steps so the NPC will not circuit far around walls.
/// 3. If the next planned step is dynamically blocked (e.g. by the player), replan
///    with that tile marked as an extra obstacle.
/// 4. Set `GridMover::direction` toward the next waypoint.
///
/// Runs before [`GridMoverSet`] so the direction is consumed in the same frame.
fn update_wander(
    time: Res<Time>,
    level: Option<Res<LevelTiles>>,
    spatial_query: SpatialQuery,
    mut query: Query<(&mut Wander, &mut GridMover, &mut Sprite, &Transform), With<Npc>>,
) {
    let Some(level) = level else { return };
    let mut rng = thread_rng();

    for (mut wander, mut mover, mut sprite, transform) in &mut query {
        wander.timer.tick(time.delta());

        let world_pos = transform.translation.truncate();
        let snapped = snap_to_grid(world_pos, mover.grid_size);

        // Pop waypoints that have been reached.
        while let Some(&next) = wander.path.front() {
            if (snapped - next).length_squared() < 0.5 * 0.5 {
                wander.path.pop_front();
            } else {
                break;
            }
        }

        let needs_new_path = wander.path.is_empty() || wander.timer.just_finished();

        if needs_new_path {
            wander.path.clear();
            wander.destination = None;

            if let Some(start_tile) = level.world_to_tile(world_pos) {
                if let Some(dest) = pick_wander_destination(&level, start_tile, WANDER_RADIUS, &mut rng) {
                    if let Some(tile_path) = astar(&level, start_tile, dest, &[]) {
                        let truncated = tile_path.into_iter().take(WANDER_MAX_PATH_STEPS);
                        for tile in truncated {
                            wander.path.push_back(level.tile_to_world(tile.0, tile.1));
                        }
                        wander.destination = Some(dest);
                    }
                }
            }
        }

        // Check if the next planned step is dynamically blocked (e.g. player is there).
        if let Some(&next_wp) = wander.path.front() {
            let filter = SpatialQueryFilter::default();
            let occupied = !spatial_query.point_intersections(next_wp, &filter).is_empty();

            if occupied {
                // Replan, treating the blocked tile as an extra obstacle.
                if let (Some(start_tile), Some(blocked_tile)) = (
                    level.world_to_tile(world_pos),
                    level.world_to_tile(next_wp),
                ) {
                    wander.path.clear();
                    if let Some(dest) = wander.destination {
                        if let Some(tile_path) = astar(&level, start_tile, dest, &[blocked_tile]) {
                            let truncated = tile_path.into_iter().take(WANDER_MAX_PATH_STEPS);
                            for tile in truncated {
                                wander.path.push_back(level.tile_to_world(tile.0, tile.1));
                            }
                        }
                    }
                }
            }
        }

        // Drive GridMover toward the next waypoint.
        let direction = wander.path.front().map(|&next| {
            let delta = next - snapped;
            if delta.x.abs() >= delta.y.abs() {
                if delta.x > 0.0 { IVec2::X } else { IVec2::NEG_X }
            } else if delta.y > 0.0 {
                IVec2::Y
            } else {
                IVec2::NEG_Y
            }
        });

        // Flip sprite to match actual horizontal movement direction.
        match direction {
            Some(d) if d == IVec2::NEG_X => sprite.flip_x = true,
            Some(d) if d == IVec2::X => sprite.flip_x = false,
            _ => {}
        }

        mover.direction = direction;
    }
}

// ---------------------------------------------------------------------------
// Destination picker
// ---------------------------------------------------------------------------

/// Picks a random walkable tile within `radius` tiles (Euclidean, in tile-space) of `origin`.
///
/// Returns `None` only in the degenerate case where no walkable tiles exist in the radius.
fn pick_wander_destination(
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
    fn wander_new_starts_idle() {
        let w = Wander::new(1.0);
        assert!(w.path.is_empty());
        assert!(w.destination.is_none());
        assert_eq!(w.timer.duration().as_secs_f32(), 1.0);
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
    fn pick_wander_destination_stays_in_radius() {
        let level = open_level();
        let origin = (5, 5);
        let radius = 3;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);

        for _ in 0..50 {
            let dest = pick_wander_destination(&level, origin, radius, &mut rng)
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
