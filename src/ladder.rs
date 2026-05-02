use avian2d::prelude::*;
use bevy::prelude::*;

use crate::level::LadderSpawnPoint;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// Marks the ladder entity that leads to the next floor.
///
/// For now the ladder is a solid, inert prop — it blocks movement and shows the
/// correct sprite but does not yet trigger a floor transition when interacted with.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Ladder;

/// Spawns the exit ladder.
pub struct LadderPlugin;

impl Plugin for LadderPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Ladder>()
            .add_systems(Startup, spawn_ladder);
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the ladder at [`LadderSpawnPoint`].
///
/// The ladder is solid (static rigid body + collider) but has no [`Interactable`]
/// marker — it is purely decorative until floor-transition logic is implemented.
fn spawn_ladder(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<LadderSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Ladder,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: 15 },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("ladder_down", true),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}
