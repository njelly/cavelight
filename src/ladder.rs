use avian2d::prelude::*;
use bevy::prelude::*;

use crate::dialogue::DialogueSource;
use crate::interaction::Interactable;
use crate::level::{LadderSpawnPoint, LadderUpSpawnPoint};
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks the ladder entity that leads down to the next floor.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Ladder;

/// Marks the ladder entity that leads up to the previous floor (or the surface).
///
/// On the first level this ladder is blocked — interacting with it shows a dialogue
/// explaining the way out is caved in. On later levels it will transition the player
/// back to the previous floor.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct LadderUp;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Spawns both the exit ladder (down) and the entrance ladder (up).
pub struct LadderPlugin;

impl Plugin for LadderPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Ladder>()
            .register_type::<LadderUp>()
            .add_systems(Startup, (spawn_ladder, spawn_ladder_up));
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the exit ladder (leading down) at [`LadderSpawnPoint`] inside the end room.
///
/// Inert for now — no [`Interactable`] marker until floor-transition logic is implemented.
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

/// Spawns the entrance ladder (leading up) at [`LadderUpSpawnPoint`] inside the start room.
///
/// Interacting with it on the first level shows a dialogue explaining the way out is blocked.
fn spawn_ladder_up(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<LadderUpSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        LadderUp,
        Interactable,
        DialogueSource {
            display_name: "Ladder".to_string(),
            dialogue_id: "first_ladder_up".to_string(),
        },
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: 16 },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("ladder_up", true),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}
