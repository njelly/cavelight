use avian2d::prelude::*;
use bevy::prelude::*;

use crate::goap::GoapAgent;
use crate::grid_mover::GridMover;
use crate::spawner::{PulseFx, SpawnRequested, SpawnType, SpawnedBy};
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

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
/// Each skeleton is placed at the event position with:
/// - A [`GoapAgent`] wander controller (smaller radius and longer idle pauses than the NPC).
/// - A [`GridMover`] at half the NPC's speed (`16.0 px/s`).
/// - A [`SpawnedBy`] tag so the origin spawner can count active instances.
/// - A [`PulseFx`] standalone entity that plays `spawner_pulse` at the spawn tile
///   and auto-despawns when the animation finishes.
fn spawn_skeletons(
    event: On<SpawnRequested>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let ev = event.event();
    if !matches!(ev.spawn_type, SpawnType::Skeleton) {
        return;
    }

    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);
    let pulse_layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let pulse_layout_handle = layouts.add(pulse_layout);

    let mut mover = GridMover::new(GRID_SIZE);
    mover.speed = 16.0;

    commands.spawn((
        Skeleton,
        SpawnedBy(ev.spawner),
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 320,
            },
        ),
        Transform::from_xyz(ev.position.x, ev.position.y, 0.0),
        SpriteAnimation::with_name("skeleton", true),
        mover,
        GoapAgent::wander(4, 8, 2.0, 5.0),
        RigidBody::Kinematic,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));

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
