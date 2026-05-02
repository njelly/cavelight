use std::time::Duration;

use avian2d::prelude::*;
use bevy::prelude::*;
use rand::seq::SliceRandom;
use rand::{Rng, thread_rng};

use crate::level::{LevelTiles, SpawnerSpawnPoint};
use crate::sprite_animation::SpriteAnimation;
use crate::wander::astar;
use crate::GRID_SIZE;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Which kind of entity a [`Spawner`] produces.
///
/// Add new variants here to support additional enemy or object types without
/// changing any spawner logic — only the observer in the relevant plugin needs updating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum SpawnType {
    Skeleton,
}

/// Triggered via [`Commands::trigger`] when a spawner has room for a new entity.
///
/// Systems in other plugins (e.g. [`SkeletonPlugin`](crate::skeleton::SkeletonPlugin))
/// observe this event and handle the actual entity construction.
#[derive(Event, Debug)]
pub struct SpawnRequested {
    /// What kind of entity to create.
    pub spawn_type: SpawnType,
    /// World-space position to spawn the entity at.
    pub position: Vec2,
    /// The spawner entity — attach [`SpawnedBy`] to the new entity to count it.
    pub spawner: Entity,
}

/// Tags a spawned entity so its origin spawner can count live instances.
///
/// Attach this to every entity created in response to a [`SpawnRequested`] event.
/// The spawner counts entities with `SpawnedBy(self_entity)` to enforce [`Spawner::capacity`].
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct SpawnedBy(pub Entity);

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Drives interval-based entity spawning for one tile position.
///
/// Each tick the timer counts down. When it fires and active entity count is
/// below `capacity`, a [`SpawnRequested`] event is triggered and the timer resets to a
/// new random duration in `[interval_min, interval_max]`.
///
/// Spawn positions are chosen at random within `spawn_radius` tiles of the spawner,
/// filtered to walkable tiles reachable via A* so entities never appear through walls.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Spawner {
    /// The type of entity produced by this spawner.
    pub spawn_type: SpawnType,
    /// Maximum number of simultaneously alive entities from this spawner.
    pub capacity: usize,
    /// Minimum seconds between spawn attempts.
    pub interval_min: f32,
    /// Maximum seconds between spawn attempts.
    pub interval_max: f32,
    /// Tile radius (Euclidean) within which spawn positions are chosen.
    pub spawn_radius: usize,
    /// Countdown to the next spawn attempt.
    #[reflect(ignore)]
    timer: Timer,
}

impl Spawner {
    /// Creates a new `Spawner` with a randomised initial interval.
    pub fn new(
        spawn_type: SpawnType,
        capacity: usize,
        interval_min: f32,
        interval_max: f32,
        spawn_radius: usize,
    ) -> Self {
        let initial = thread_rng().gen_range(interval_min..=interval_max);
        Self {
            spawn_type,
            capacity,
            interval_min,
            interval_max,
            spawn_radius,
            timer: Timer::new(Duration::from_secs_f32(initial), TimerMode::Once),
        }
    }

    fn reset_timer(&mut self, rng: &mut impl Rng) {
        let interval = rng.gen_range(self.interval_min..=self.interval_max);
        self.timer = Timer::new(Duration::from_secs_f32(interval), TimerMode::Once);
    }
}

/// Temporarily added to a spawner entity when it creates a new entity.
///
/// [`spin_spawners`] rotates the transform by `angular_velocity` each frame while
/// this component is present, producing a visual "activation" cue. Removed and
/// transform snapped back to identity rotation when `timer` expires.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct SpawnerSpin {
    /// How long the spin lasts.
    #[reflect(ignore)]
    timer: Timer,
    /// Rotation speed in radians per second.
    pub angular_velocity: f32,
}

impl SpawnerSpin {
    /// Creates a spin effect lasting `duration_secs` at the given `angular_velocity`.
    pub fn new(duration_secs: f32, angular_velocity: f32) -> Self {
        Self {
            timer: Timer::from_seconds(duration_secs, TimerMode::Once),
            angular_velocity,
        }
    }
}

/// Marker for the pulse visual spawned beneath an entity when it appears.
///
/// [`despawn_pulse_fx`] removes this entity automatically once its
/// [`SpriteAnimation`] reports `is_complete` (i.e. the non-looping animation finishes).
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct PulseFx;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers spawner types, the [`SpawnRequested`] trigger, and all spawner systems.
///
/// Also spawns the spawner entity in the key chest room at [`SpawnerSpawnPoint`].
pub struct SpawnerPlugin;

impl Plugin for SpawnerPlugin {
    fn build(&self, app: &mut App) {
        app
            .register_type::<SpawnedBy>()
            .register_type::<Spawner>()
            .register_type::<SpawnerSpin>()
            .register_type::<PulseFx>()
            .add_systems(Startup, spawn_spawner)
            .add_systems(Update, (tick_spawners, spin_spawners, despawn_pulse_fx));
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the skeleton spawner entity at [`SpawnerSpawnPoint`] in the key chest room.
fn spawn_spawner(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<SpawnerSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Spawner::new(SpawnType::Skeleton, 2, 8.0, 20.0, 4),
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 18,
            },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("spawner_source", true),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}

// ---------------------------------------------------------------------------
// Update systems
// ---------------------------------------------------------------------------

/// Ticks all spawner timers and triggers [`SpawnRequested`] when the interval elapses
/// and the active entity count is below [`Spawner::capacity`].
///
/// Active count is determined by querying [`SpawnedBy`] components that point to
/// each spawner entity — no bookkeeping state required.
///
/// Spawn position is chosen randomly within [`Spawner::spawn_radius`] tiles, filtered
/// to walkable tiles that are reachable via A* from the spawner. If no valid position
/// is found (e.g. the level hasn't loaded yet), the attempt is skipped.
fn tick_spawners(
    time: Res<Time>,
    level: Option<Res<LevelTiles>>,
    mut commands: Commands,
    mut spawners: Query<(Entity, &mut Spawner, &Transform)>,
    spawned_by: Query<&SpawnedBy>,
) {
    let Some(level) = level else { return };
    let mut rng = thread_rng();

    for (entity, mut spawner, transform) in &mut spawners {
        spawner.timer.tick(time.delta());
        if !spawner.timer.just_finished() {
            continue;
        }

        // Count living entities that belong to this spawner.
        let active_count = spawned_by.iter().filter(|s| s.0 == entity).count();

        if active_count < spawner.capacity {
            let spawner_world = transform.translation.truncate();
            let radius = spawner.spawn_radius;

            if let Some(spawn_pos) = pick_reachable_spawn(&level, spawner_world, radius, &mut rng) {
                commands.trigger(SpawnRequested {
                    spawn_type: spawner.spawn_type,
                    position: spawn_pos,
                    spawner: entity,
                });
                // Spin the spawner sprite as a visual activation cue.
                commands.entity(entity).insert(SpawnerSpin::new(0.6, std::f32::consts::TAU * 2.0));
            }
        }

        spawner.reset_timer(&mut rng);
    }
}

/// Picks a random walkable tile within `radius` tiles (Euclidean) of `spawner_world`
/// that is reachable from the spawner via A*.
///
/// Candidates are shuffled randomly and tested one at a time so the first reachable
/// tile wins. Returns `None` if the spawner position is off-grid or no reachable
/// walkable tile exists within the radius.
fn pick_reachable_spawn(
    level: &LevelTiles,
    spawner_world: Vec2,
    radius: usize,
    rng: &mut impl Rng,
) -> Option<Vec2> {
    let origin = level.world_to_tile(spawner_world)?;
    let r = radius as i32;
    let (ox, oy) = (origin.0 as i32, origin.1 as i32);
    let r_sq = (radius * radius) as i32;

    let mut candidates: Vec<(usize, usize)> = (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| dx * dx + dy * dy <= r_sq)
        .map(|(dx, dy)| (ox + dx, oy + dy))
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .map(|(x, y)| (x as usize, y as usize))
        .filter(|&(x, y)| level.is_walkable(x, y) && (x, y) != origin)
        .collect();

    candidates.shuffle(rng);

    for candidate in candidates {
        if astar(level, origin, candidate, &[]).is_some() {
            return Some(level.tile_to_world(candidate.0, candidate.1));
        }
    }

    None
}

/// Rotates spawner entities that have a [`SpawnerSpin`] component.
///
/// Removes the component and resets the transform rotation when the timer expires.
fn spin_spawners(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut SpawnerSpin, &mut Transform)>,
) {
    for (entity, mut spin, mut transform) in &mut query {
        spin.timer.tick(time.delta());
        transform.rotation *= Quat::from_rotation_z(spin.angular_velocity * time.delta_secs());
        if spin.timer.just_finished() {
            transform.rotation = Quat::IDENTITY;
            commands.entity(entity).remove::<SpawnerSpin>();
        }
    }
}

/// Despawns [`PulseFx`] entities whose [`SpriteAnimation`] has completed.
fn despawn_pulse_fx(
    mut commands: Commands,
    query: Query<(Entity, &SpriteAnimation), With<PulseFx>>,
) {
    for (entity, animation) in &query {
        if animation.is_complete {
            commands.entity(entity).despawn();
        }
    }
}
