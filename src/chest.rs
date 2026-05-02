use avian2d::prelude::*;
use bevy::prelude::*;

use crate::interaction::{Interactable, InteractEvent};
use crate::inventory::{ActiveChest, InputMode};
use crate::item::{Inventory, ItemStack};
use crate::level::ChestSpawnPoint;
use crate::GRID_SIZE;

/// State component for a chest entity.
///
/// Tracks whether the chest is open or closed. Opening a chest also opens the
/// inventory UI and sets [`ActiveChest`] so the dual-panel view is displayed.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Chest {
    pub is_open: bool,
}

/// Spawns the chest and registers the observer that handles interactions.
pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Chest>()
            .add_systems(Startup, spawn_chest)
            .add_observer(on_chest_interact);
    }
}

/// Spawns the chest at [`ChestSpawnPoint`] with starting inventory (8 arrows + 1 bow).
///
/// The chest starts closed (atlas frame 3). [`Interactable`] lets the interaction
/// system detect it, and a static [`Collider`] prevents the player walking through it.
fn spawn_chest(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<ChestSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    // Stub: always start with 8 arrows and 1 bow. The level generator will
    // eventually vary chest contents based on depth / seed.
    let mut inventory = Inventory::new(16);
    inventory.insert_first_empty(ItemStack::new("arrow", 8));
    inventory.insert_first_empty(ItemStack::new("bow", 1));

    commands.spawn((
        Chest { is_open: false },
        inventory,
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

/// Observer that toggles a chest open/closed on player interaction.
///
/// **Opening**: flips sprite to frame 4, sets [`ActiveChest`] to this entity,
/// and switches [`InputMode`] to [`InputMode::Inventory`] so the inventory UI opens.
///
/// **Closing**: flips sprite back to frame 3. The inventory UI is closed
/// separately (via Escape or the X button); closing the chest does not reopen it.
fn on_chest_interact(
    on: On<InteractEvent>,
    mut chest_query: Query<(&mut Chest, &mut Sprite)>,
    mut input_mode: ResMut<InputMode>,
    mut active_chest: ResMut<ActiveChest>,
) {
    if let Ok((mut chest, mut sprite)) = chest_query.get_mut(on.event().entity) {
        chest.is_open = !chest.is_open;
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = if chest.is_open { 4 } else { 3 };
        }
        if chest.is_open {
            active_chest.0 = Some(on.event().entity);
            *input_mode = InputMode::Inventory;
        }
        // When closing the chest the inventory was already dismissed by the UI —
        // no need to change InputMode here.
    }
}
