use avian2d::prelude::*;
use bevy::prelude::*;

use crate::damageable::Damageable;
use crate::entity::EntityLibrary;
use crate::goap::GoapAgent;
use crate::grid_mover::GridMover;
use crate::spawner::{PulseFx, SpawnRequested, SpawnType, SpawnedBy};
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// Entity-def id used to look up the skeleton's [`Damageable`] in the [`EntityLibrary`].
const SKELETON_ID: &str = "skeleton";

/// Fallback toughness used when the [`EntityLibrary`] has not yet finished loading
/// (e.g. during PostStartup save-restore). Should match `entity_definitions.ron`.
const SKELETON_FALLBACK_TOUGHNESS: u32 = 20;

/// Fallback display name used in the same scenario as [`SKELETON_FALLBACK_TOUGHNESS`].
const SKELETON_FALLBACK_NAME: &str = "Skeleton";

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks a skeleton enemy entity.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Skeleton;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers the skeleton type and the observer that creates skeletons in response
/// to [`SpawnRequested`] triggers.
pub struct SkeletonPlugin;

impl Plugin for SkeletonPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Skeleton>()
            .add_observer(spawn_skeletons);
    }
}

// ---------------------------------------------------------------------------
// Observer
// ---------------------------------------------------------------------------

/// Observes [`SpawnRequested`] triggers and spawns a skeleton when the type matches.
///
/// Defers the skeleton entity construction to [`spawn_skeleton_entity`] (so the same
/// helper is reusable from the save system) and adds a [`PulseFx`] standalone entity
/// that plays `spawner_pulse` at the spawn tile and auto-despawns when the animation
/// finishes.
fn spawn_skeletons(
    event: On<SpawnRequested>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    entity_library: Option<Res<EntityLibrary>>,
) {
    let ev = event.event();
    if !matches!(ev.spawn_type, SpawnType::Skeleton) {
        return;
    }

    spawn_skeleton_entity(
        &mut commands,
        &asset_server,
        &mut layouts,
        entity_library.as_deref(),
        ev.position,
        ev.spawner,
    );

    let pulse_layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let pulse_layout_handle = layouts.add(pulse_layout);

    // Pulse effect is a standalone entity at the spawn tile — not a child of the skeleton
    // so it stays put while the skeleton walks away. Auto-despawned when animation finishes.
    commands.spawn((
        PulseFx,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: pulse_layout_handle,
                index: 25,
            },
        ),
        Transform::from_xyz(ev.position.x, ev.position.y, -0.1),
        SpriteAnimation::with_name("spawner_pulse", false),
    ));
}

/// Builds a fully-initialised skeleton entity at `position`, tagged as spawned by `spawner`.
///
/// Used both by the [`SpawnRequested`] observer and by the save-load system. Returns the
/// new [`Entity`] so callers can attach extra components (e.g. visual effects).
///
/// `library` is consulted for the [`Damageable`] (display name + base toughness).
/// When it is `None` (e.g. during PostStartup save-restore, before the asset has
/// finished loading) the function falls back to hardcoded values matching
/// `entity_definitions.ron`.
///
/// Each skeleton has:
/// - A [`Damageable`] so arrows can hurt and eventually kill it.
/// - A [`GoapAgent`] wander controller (smaller radius and longer idle pauses than the NPC).
/// - A [`GridMover`] at half the NPC's speed (`10.0 px/s`).
/// - A [`SpawnedBy`] tag so the origin spawner can count active instances.
pub fn spawn_skeleton_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    library: Option<&EntityLibrary>,
    position: Vec2,
    spawner: Entity,
) -> Entity {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    let mover = GridMover::new(GRID_SIZE).with_walk_speed(10.0);

    let damageable = library
        .and_then(|l| l.damageable(SKELETON_ID))
        .unwrap_or_else(|| Damageable::new(SKELETON_FALLBACK_TOUGHNESS, SKELETON_FALLBACK_NAME));

    commands.spawn((
        Skeleton,
        SpawnedBy(spawner),
        damageable,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 320,
            },
        ),
        Transform::from_xyz(position.x, position.y, 0.0),
        SpriteAnimation::with_name("skeleton", true),
        mover,
        GoapAgent::wander(4, 8, 2.0, 5.0),
        RigidBody::Kinematic,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    )).id()
}
