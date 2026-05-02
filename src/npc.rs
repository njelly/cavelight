use avian2d::prelude::*;
use bevy::prelude::*;

use crate::goap::GoapAgent;
use crate::grid_mover::GridMover;
use crate::level::NpcSpawnPoint;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks the female NPC entity.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Npc;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Spawns the NPC and registers its type.
///
/// Movement and pathfinding are driven by [`GoapPlugin`](crate::goap::GoapPlugin)
/// via the [`GoapAgent`] component with the [`Goal::Wander`] goal.
pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Npc>()
            .add_systems(Startup, spawn_npc);
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the female NPC at [`NpcSpawnPoint`] with a GOAP wander agent and a grid mover.
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
        GridMover::new(GRID_SIZE).with_walk_speed(16.0),
        RigidBody::Kinematic,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
        GoapAgent::wander(6, 10, 1.0, 3.0),
    ));
}
