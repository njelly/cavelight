use avian2d::prelude::*;
use bevy::prelude::*;

use crate::level::LevelTiles;

/// Default walk speed for AI entities in pixels per second.
const DEFAULT_WALK_SPEED: f32 = 16.0;

/// Default speed for player-controlled entities in pixels per second.
///
/// Player entities use [`GridMover::new`] without calling [`GridMover::walk`] or
/// `run`, so they always move at this speed regardless of the walk/run distinction.
const DEFAULT_PLAYER_SPEED: f32 = 32.0;

/// System set for grid movement simulation.
///
/// Use this to order other systems relative to grid movement. For example, input
/// systems should run `.before(GridMoverSet)` so direction is set before the mover
/// consumes it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GridMoverSet;

/// Drives smooth, grid-locked movement for an entity.
///
/// Each movement step translates the entity exactly one grid cell using linear interpolation
/// for smooth visual motion — matching the feel of classic Pokémon (Game Boy). A controller
/// (player input or AI) sets `direction` each frame. The mover consumes the direction when
/// beginning each step, so holding a direction produces continuous tile-by-tile movement.
///
/// AI controllers call [`GridMover::walk`] to select walk speed while wandering, and will
/// call `run()` (added with the aggro system) while chasing. Player-controlled entities
/// leave `speed` at the default ([`DEFAULT_PLAYER_SPEED`]).
///
/// # Example
/// ```rust,ignore
/// // Player — uses default speed, no walk/run distinction needed.
/// commands.spawn((sprite, GridMover::new(GRID_SIZE)));
///
/// // AI entity — explicit walk speed; run speed added when aggro is implemented.
/// commands.spawn((sprite, GridMover::new(GRID_SIZE).with_walk_speed(16.0)));
/// ```
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct GridMover {
    /// Size of one grid cell in world units.
    pub grid_size: f32,
    /// Active movement speed in pixels per second.
    ///
    /// Player entities leave this at the default. AI entities call [`GridMover::walk`]
    /// each frame from their navigation system to keep this in sync with their current
    /// action. A `run()` method and `run_speed` field will be added with the aggro system.
    pub speed: f32,
    /// Speed used when the entity is wandering (walk-pace movement).
    pub walk_speed: f32,
    /// Requested direction for the next step. Set each frame by a controller.
    /// Uses cardinal directions only — diagonal movement is not supported.
    pub direction: Option<IVec2>,
    moving: bool,
    start: Vec2,
    target: Vec2,
    /// Normalized progress from 0.0 (start) to 1.0 (target).
    progress: f32,
}

impl GridMover {
    /// Creates a new `GridMover` for the given grid cell size.
    ///
    /// Initialises `speed` to [`DEFAULT_PLAYER_SPEED`] so player-controlled entities
    /// work without further configuration. AI entities should follow up with
    /// [`GridMover::with_walk_speed`] to set a slower wander speed.
    pub fn new(grid_size: f32) -> Self {
        Self {
            grid_size,
            speed: DEFAULT_PLAYER_SPEED,
            walk_speed: DEFAULT_WALK_SPEED,
            direction: None,
            moving: false,
            start: Vec2::ZERO,
            target: Vec2::ZERO,
            progress: 0.0,
        }
    }

    /// Sets the walk speed and initialises `speed` to match.
    ///
    /// Call this on AI entities after [`GridMover::new`] to configure a per-entity
    /// wander pace. The active `speed` is set to `walk_speed` immediately so the
    /// entity starts at walk pace even before the GOAP system ticks.
    pub fn with_walk_speed(mut self, walk_speed: f32) -> Self {
        self.walk_speed = walk_speed;
        self.speed = walk_speed;
        self
    }

    /// Sets the active speed to [`GridMover::walk_speed`].
    ///
    /// Call each frame from an AI navigation system when the entity is wandering.
    pub fn walk(&mut self) {
        self.speed = self.walk_speed;
    }
}

/// Simulates smooth grid-locked movement for all [`GridMover`] entities.
pub struct GridMoverPlugin;

impl Plugin for GridMoverPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GridMover>()
            .add_systems(Update, move_grid_movers.in_set(GridMoverSet));
    }
}

/// Advances all [`GridMover`] entities toward their target, then starts the next step if directed.
///
/// When a movement step completes and a direction is still set (key held), the next step begins
/// in the same frame — producing seamless continuous movement with no idle frame between cells.
///
/// Wall blocking is a two-stage check:
/// 1. [`LevelTiles`] walkability lookup — O(1) array read, blocks all wall tiles.
/// 2. [`SpatialQuery::point_intersections`] — detects entity colliders (chests, doors, etc.).
///    Sensor colliders (e.g. open doors) are passable and excluded from the blocked check.
///
/// Wall tiles carry no physics bodies; [`LevelTiles`] is the sole authority for walls.
fn move_grid_movers(
    time: Res<Time>,
    spatial_query: SpatialQuery,
    sensor_query: Query<(), With<Sensor>>,
    level: Res<LevelTiles>,
    mut query: Query<(&mut GridMover, &mut Transform)>,
) {
    for (mut mover, mut transform) in &mut query {
        if mover.moving {
            mover.progress += (mover.speed / mover.grid_size) * time.delta_secs();

            if mover.progress >= 1.0 {
                // Snap exactly to target and end the step.
                transform.translation = mover.target.extend(transform.translation.z);
                mover.moving = false;
                mover.progress = 0.0;
                // Fall through to immediately start the next step if a direction is queued.
            } else {
                transform.translation =
                    mover.start.lerp(mover.target, mover.progress).extend(transform.translation.z);
                continue;
            }
        }

        if let Some(dir) = mover.direction.take() {
            let current = snap_to_grid(transform.translation.truncate(), mover.grid_size);
            let potential_target = current + dir.as_vec2() * mover.grid_size;

            // Block movement into wall tiles via the walkability grid (no physics query needed).
            match level.world_to_tile(potential_target) {
                Some((tx, ty)) if !level.is_walkable(tx, ty) => continue,
                None => continue, // Off the map edge.
                _ => {}
            }

            // Block movement into solid entity colliders (chests, locked door, spawner, etc.).
            let blocked = spatial_query
                .point_intersections(potential_target, &SpatialQueryFilter::default())
                .iter()
                .any(|&e| !sensor_query.contains(e));
            if blocked {
                continue;
            }

            mover.start = current;
            mover.target = potential_target;
            mover.progress = 0.0;
            mover.moving = true;
            transform.translation = mover.start.extend(transform.translation.z);
        }
    }
}

/// Snaps a world position to the nearest grid cell center.
pub fn snap_to_grid(pos: Vec2, grid_size: f32) -> Vec2 {
    (pos / grid_size).round() * grid_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_mover_starts_idle() {
        let mover = GridMover::new(8.0);
        assert!(!mover.moving);
        assert!(mover.direction.is_none());
        assert_eq!(mover.grid_size, 8.0);
    }

    #[test]
    fn new_grid_mover_defaults_to_player_speed() {
        // Player-controlled entities use GridMover::new — they should start at the full
        // player speed without needing further configuration.
        let mover = GridMover::new(8.0);
        assert_eq!(mover.speed, DEFAULT_PLAYER_SPEED);
    }

    #[test]
    fn with_walk_speed_sets_walk_and_activates_it() {
        // AI entities call with_walk_speed; the active speed should start at walk.
        let mover = GridMover::new(8.0).with_walk_speed(10.0);
        assert_eq!(mover.walk_speed, 10.0);
        assert_eq!(mover.speed, 10.0);
    }

    #[test]
    fn walk_sets_active_speed_to_walk_speed() {
        let mut mover = GridMover::new(8.0).with_walk_speed(10.0);
        // Simulate something that changed speed (e.g. a future run action).
        mover.speed = 99.0;
        mover.walk();
        assert_eq!(mover.speed, 10.0);
    }

    #[test]
    fn snap_rounds_to_nearest_cell() {
        assert_eq!(snap_to_grid(Vec2::new(5.0, 7.0), 8.0), Vec2::new(8.0, 8.0));
        assert_eq!(snap_to_grid(Vec2::new(3.0, 3.0), 8.0), Vec2::new(0.0, 0.0));
        assert_eq!(snap_to_grid(Vec2::new(8.0, 8.0), 8.0), Vec2::new(8.0, 8.0));
        assert_eq!(snap_to_grid(Vec2::new(-5.0, -5.0), 8.0), Vec2::new(-8.0, -8.0));
    }

    #[test]
    fn snap_on_exact_grid_is_unchanged() {
        assert_eq!(snap_to_grid(Vec2::new(16.0, 24.0), 8.0), Vec2::new(16.0, 24.0));
    }

    #[test]
    fn snap_on_tile_center_is_stable() {
        // Tile centers must be fixed points of snap_to_grid so a GridMover starting
        // on a tile (e.g. at the player spawn) steps to the *next* tile, not to a
        // shifted grid. Any multiple of grid_size must round to itself.
        for k in [-4i32, -1, 0, 1, 4] {
            let pos = Vec2::splat(k as f32 * 8.0);
            assert_eq!(snap_to_grid(pos, 8.0), pos, "snap drifted for k={k}");
        }
    }
}
