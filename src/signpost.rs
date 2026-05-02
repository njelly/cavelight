use avian2d::prelude::*;
use bevy::prelude::*;

use crate::dialogue::DialogueSource;
use crate::interaction::Interactable;
use crate::level::SignpostSpawnPoint;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// Marks a signpost entity.
///
/// Signposts are static [`Interactable`] objects that display dialogue when
/// the player presses Space while facing them.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Signpost;

/// Spawns the signpost and wires it to the dialogue system.
pub struct SignpostPlugin;

impl Plugin for SignpostPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Signpost>()
            .add_systems(Startup, spawn_signpost);
    }
}

/// Spawns the signpost at [`SignpostSpawnPoint`] with a static sprite, a solid collider,
/// and a [`DialogueSource`] so the player can read it by pressing Space.
fn spawn_signpost(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<SignpostSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Signpost,
        Interactable,
        DialogueSource {
            display_name: "Sign".to_string(),
            dialogue_id: "signpost_welcome".to_string(),
        },
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 136,
            },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("signpost", true),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}
