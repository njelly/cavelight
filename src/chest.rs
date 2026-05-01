use avian2d::prelude::*;
use bevy::prelude::*;

use crate::interaction::{Interactable, InteractEvent};
use crate::level::ChestSpawnPoint;
use crate::GRID_SIZE;

/// State component for a chest entity.
///
/// A chest starts closed and can be toggled open by the player interacting with it.
/// `is_open` drives the displayed sprite frame and will gate future inventory logic.
#[derive(Component, Debug)]
pub struct Chest {
    pub is_open: bool,
}

/// Spawns the chest and registers the observer that toggles it open/closed on interaction.
pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_chest)
            .add_observer(on_chest_interact);
    }
}

/// Spawns the chest at [`ChestSpawnPoint`] with a closed sprite (atlas frame 3).
///
/// The chest gets [`Interactable`] so the interaction system can detect it via
/// spatial query, and a static [`Collider`] so the player cannot walk through it.
fn spawn_chest(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<ChestSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Chest { is_open: false },
        Interactable,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 3,
            },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}

/// Observer that toggles a chest between open and closed when the player interacts with it.
///
/// Reacts to [`InteractEvent`] triggers. Switches [`Chest::is_open`] and the sprite
/// atlas index (3 = closed, 4 = open) for the targeted chest entity.
fn on_chest_interact(
    on: On<InteractEvent>,
    mut chest_query: Query<(&mut Chest, &mut Sprite)>,
) {
    if let Ok((mut chest, mut sprite)) = chest_query.get_mut(on.event().entity) {
        chest.is_open = !chest.is_open;
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = if chest.is_open { 4 } else { 3 };
        }
    }
}
