use bevy::prelude::*;

/// Movement speed in pixels per second. At 8px per cell this gives ~4 steps/sec,
/// matching the feel of classic Pokémon (Game Boy).
const DEFAULT_SPEED: f32 = 32.0;

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
/// # Example
/// ```rust,ignore
/// commands.spawn((sprite, GridMover::new(GRID_SIZE)));
/// ```
#[derive(Component, Debug)]
pub struct GridMover {
    /// Size of one grid cell in world units.
    pub grid_size: f32,
    /// Movement speed in pixels per second.
    pub speed: f32,
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
    /// Creates a new `GridMover` for the given grid cell size with the default speed.
    pub fn new(grid_size: f32) -> Self {
        Self {
            grid_size,
            speed: DEFAULT_SPEED,
            direction: None,
            moving: false,
            start: Vec2::ZERO,
            target: Vec2::ZERO,
            progress: 0.0,
        }
    }
}

/// Simulates smooth grid-locked movement for all [`GridMover`] entities.
pub struct GridMoverPlugin;

impl Plugin for GridMoverPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, move_grid_movers.in_set(GridMoverSet));
    }
}

/// Advances all [`GridMover`] entities toward their target, then starts the next step if directed.
///
/// When a movement step completes and a direction is still set (key held), the next step begins
/// in the same frame — producing seamless continuous movement with no idle frame between cells.
fn move_grid_movers(time: Res<Time>, mut query: Query<(&mut GridMover, &mut Transform)>) {
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
            mover.start = current;
            mover.target = current + dir.as_vec2() * mover.grid_size;
            mover.progress = 0.0;
            mover.moving = true;
            transform.translation = mover.start.extend(transform.translation.z);
        }
    }
}

/// Snaps a world position to the nearest grid cell.
fn snap_to_grid(pos: Vec2, grid_size: f32) -> Vec2 {
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
}
