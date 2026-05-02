use std::collections::HashMap;

use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Item definitions (loaded from RON)
// ---------------------------------------------------------------------------

fn default_true() -> bool { true }

/// Static properties of an item type, loaded from `inventory_items.items.ron`.
///
/// [`ItemDef`]s are read-only and shared across all instances. Active items in
/// inventories are represented by [`ItemStack`]s that reference a def by [`id`](ItemDef::id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    /// Unique string key used to reference this item type. E.g. `"arrow"`.
    pub id: String,
    /// Human-readable name shown in the inventory UI.
    pub display_name: String,
    /// Asset path for the item icon PNG, relative to the `assets/` folder.
    pub icon_path: String,
    /// Maximum items per inventory slot. `1` = non-stackable (weapons, bows, etc.).
    pub max_stack: u32,
    /// Whether this item can be placed in a hotbar slot and equipped. Consumables
    /// like arrows are `false` — they are ammunition, not equippable weapons.
    #[serde(default = "default_true")]
    pub equippable: bool,
    /// Id of the ammo [`ItemDef`] required by this item (e.g. `"arrow"` for a bow).
    /// `None` means the item needs no ammo.
    #[serde(default)]
    pub ammo_id: Option<String>,
}

/// Raw list of [`ItemDef`]s deserialized directly from `inventory_items.items.ron`.
///
/// Bevy loads this as an [`Asset`]; [`ItemPlugin`] converts it into the runtime
/// [`ItemLibrary`] resource once loading completes.
#[derive(Asset, TypePath, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemDefList(pub Vec<ItemDef>);

/// In-flight asset handle stored until [`ItemLibrary`] is built.
#[derive(Resource)]
struct ItemDefListHandle(Handle<ItemDefList>);

// ---------------------------------------------------------------------------
// Runtime item registry
// ---------------------------------------------------------------------------

/// Runtime item registry built from [`ItemDefList`] after the RON asset loads.
///
/// Provides O(1) lookup of pre-loaded icon [`Handle<Image>`]s and full [`ItemDef`]s
/// by item id. Available as a [`Resource`] once [`ItemPlugin`] finishes initialisation.
#[derive(Resource)]
pub struct ItemLibrary {
    icons: HashMap<String, Handle<Image>>,
    pub defs: HashMap<String, ItemDef>,
}

impl ItemLibrary {
    /// Returns the pre-loaded icon handle for `id`, or `None` if unknown.
    pub fn icon(&self, id: &str) -> Option<&Handle<Image>> {
        self.icons.get(id)
    }

    /// Returns the [`ItemDef`] for `id`, or `None` if unknown.
    pub fn def(&self, id: &str) -> Option<&ItemDef> {
        self.defs.get(id)
    }
}

// ---------------------------------------------------------------------------
// ItemStack
// ---------------------------------------------------------------------------

/// A stack of items occupying a single [`Inventory`] slot.
///
/// `id` references an [`ItemDef`] in the [`ItemLibrary`]. `count` must be ≥ 1 and
/// ≤ the def's [`max_stack`](ItemDef::max_stack).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    /// Item type identifier matching an [`ItemDef::id`] in the [`ItemLibrary`].
    pub id: String,
    /// Number of items in this stack.
    pub count: u32,
}

impl ItemStack {
    /// Creates a new stack with the given item id and count.
    pub fn new(id: impl Into<String>, count: u32) -> Self {
        Self { id: id.into(), count }
    }
}

// ---------------------------------------------------------------------------
// Inventory component
// ---------------------------------------------------------------------------

/// Fixed-capacity item storage for entities that carry items (player, chest, etc.).
///
/// Slots are indexed from `0` (top-left in UI display order) to `capacity - 1`.
/// Each slot holds at most one [`ItemStack`]. Use [`take`](Inventory::take) /
/// [`put`](Inventory::put) together to implement drag-and-drop swapping, and
/// [`insert_first_empty`](Inventory::insert_first_empty) to pick up loot.
#[derive(Component, Default, Debug)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    /// Creates an inventory with `capacity` empty slots.
    pub fn new(capacity: usize) -> Self {
        Self { slots: vec![None; capacity] }
    }

    /// Returns a reference to the stack at `index`, or `None` if empty or out of bounds.
    pub fn get(&self, index: usize) -> Option<&ItemStack> {
        self.slots.get(index)?.as_ref()
    }

    /// Removes and returns the stack at `index`, leaving that slot empty.
    ///
    /// Returns `None` if the slot is already empty or `index` is out of bounds.
    pub fn take(&mut self, index: usize) -> Option<ItemStack> {
        self.slots.get_mut(index)?.take()
    }

    /// Places `stack` into `index` and returns whatever was there before.
    ///
    /// Returns `Err` containing `stack` if `index` is out of bounds.
    pub fn put(
        &mut self,
        index: usize,
        stack: Option<ItemStack>,
    ) -> Result<Option<ItemStack>, Option<ItemStack>> {
        match self.slots.get_mut(index) {
            Some(slot) => {
                let old = slot.take();
                *slot = stack;
                Ok(old)
            }
            None => Err(stack),
        }
    }

    /// Returns the total number of slots (both occupied and empty).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Inserts `stack` into the first empty slot. Returns `false` if the inventory is full.
    pub fn insert_first_empty(&mut self, stack: ItemStack) -> bool {
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some(stack);
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Loads `inventory_items.items.ron` and provides the [`ItemLibrary`] resource.
pub struct ItemPlugin;

impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<ItemDefList>::new(&["ron"]))
            .add_systems(Startup, load_item_defs)
            .add_systems(Update, build_item_library);
    }
}

/// Kicks off the async load of `inventory_items.items.ron`.
fn load_item_defs(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handle = asset_server.load("item_definitions.ron");
    commands.insert_resource(ItemDefListHandle(handle));
}

/// Builds and inserts [`ItemLibrary`] once the [`ItemDefList`] asset finishes loading.
///
/// Pre-loads all item icon textures so the inventory UI can display them without
/// per-frame asset requests. Runs every frame until the library is ready, then exits early.
fn build_item_library(
    mut commands: Commands,
    handle: Option<Res<ItemDefListHandle>>,
    list_assets: Res<Assets<ItemDefList>>,
    asset_server: Res<AssetServer>,
    library: Option<Res<ItemLibrary>>,
) {
    if library.is_some() {
        return;
    }
    let Some(handle) = handle else { return };
    let Some(list) = list_assets.get(&handle.0) else { return };

    let mut icons = HashMap::new();
    let mut defs = HashMap::new();
    for def in &list.0 {
        icons.insert(def.id.clone(), asset_server.load::<Image>(&def.icon_path));
        defs.insert(def.id.clone(), def.clone());
    }
    commands.insert_resource(ItemLibrary { icons, defs });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_new_is_empty() {
        let inv = Inventory::new(4);
        assert_eq!(inv.slots.len(), 4);
        for i in 0..4 {
            assert!(inv.get(i).is_none());
        }
    }

    #[test]
    fn inventory_put_and_get() {
        let mut inv = Inventory::new(4);
        inv.put(0, Some(ItemStack::new("arrow", 8))).unwrap();
        let s = inv.get(0).unwrap();
        assert_eq!(s.id, "arrow");
        assert_eq!(s.count, 8);
    }

    #[test]
    fn inventory_take_leaves_slot_empty() {
        let mut inv = Inventory::new(4);
        inv.put(1, Some(ItemStack::new("bow", 1))).unwrap();
        let taken = inv.take(1).unwrap();
        assert_eq!(taken.id, "bow");
        assert!(inv.get(1).is_none());
    }

    #[test]
    fn inventory_put_returns_old_contents() {
        let mut inv = Inventory::new(4);
        inv.put(0, Some(ItemStack::new("dagger", 1))).unwrap();
        let old = inv.put(0, Some(ItemStack::new("bow", 1))).unwrap();
        assert_eq!(old.unwrap().id, "dagger");
        assert_eq!(inv.get(0).unwrap().id, "bow");
    }

    #[test]
    fn inventory_put_out_of_bounds_returns_err() {
        let mut inv = Inventory::new(2);
        let result = inv.put(5, Some(ItemStack::new("arrow", 1)));
        assert!(result.is_err());
    }

    #[test]
    fn inventory_insert_first_empty_fills_in_order() {
        let mut inv = Inventory::new(3);
        assert!(inv.insert_first_empty(ItemStack::new("a", 1)));
        assert!(inv.insert_first_empty(ItemStack::new("b", 1)));
        assert!(inv.insert_first_empty(ItemStack::new("c", 1)));
        assert!(!inv.insert_first_empty(ItemStack::new("d", 1)));
        assert_eq!(inv.get(0).unwrap().id, "a");
        assert_eq!(inv.get(1).unwrap().id, "b");
        assert_eq!(inv.get(2).unwrap().id, "c");
    }

    #[test]
    fn item_stack_new() {
        let s = ItemStack::new("arrow", 16);
        assert_eq!(s.id, "arrow");
        assert_eq!(s.count, 16);
    }
}
