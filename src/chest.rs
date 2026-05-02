use avian2d::prelude::*;
use bevy::prelude::*;

use crate::interaction::{Interactable, InteractEvent};
use crate::inventory::{ActiveChest, InputMode};
use crate::item::{Inventory, ItemStack};
use crate::level::{KeyChestSpawnPoint, WeaponChestSpawnPoint};
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

/// Spawns both level chests and registers the shared interaction observer.
pub struct ChestPlugin;

impl Plugin for ChestPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Chest>()
            .add_systems(Startup, (spawn_weapon_chest, spawn_key_chest))
            .add_observer(on_chest_interact);
    }
}

// ---------------------------------------------------------------------------
// Spawners
// ---------------------------------------------------------------------------

/// Spawns the weapon chest at [`WeaponChestSpawnPoint`] pre-loaded with a bow and arrows.
fn spawn_weapon_chest(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<WeaponChestSpawnPoint>,
) {
    let mut inventory = Inventory::new(16);
    inventory.insert_first_empty(ItemStack::new("arrow", 8));
    inventory.insert_first_empty(ItemStack::new("bow", 1));

    spawn_chest_entity(&mut commands, &asset_server, &mut layouts, spawn_point.0, inventory);
}

/// Spawns the key chest at [`KeyChestSpawnPoint`] pre-loaded with a key.
fn spawn_key_chest(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<KeyChestSpawnPoint>,
) {
    let mut inventory = Inventory::new(16);
    inventory.insert_first_empty(ItemStack::new("key", 1));

    spawn_chest_entity(&mut commands, &asset_server, &mut layouts, spawn_point.0, inventory);
}

/// Spawns a chest entity at `pos` with the given `inventory`.
///
/// The chest starts closed (atlas frame 3). [`Interactable`] lets the interaction
/// system detect it, and a static [`Collider`] prevents the player walking through it.
fn spawn_chest_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    pos: Vec2,
    inventory: Inventory,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Chest { is_open: false },
        inventory,
        Interactable,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: 3 },
        ),
        Transform::from_xyz(pos.x, pos.y, 0.0),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}

// ---------------------------------------------------------------------------
// Interaction observer
// ---------------------------------------------------------------------------

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
    }
}
