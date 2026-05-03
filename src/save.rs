//! Save / load system.
//!
//! State persistence model: a single [`SaveData`] snapshot per slot, written as RON
//! to `<data_dir>/cavelight/saves/<slot>/save.ron`. On macOS this is
//! `~/Library/Application Support/cavelight/...`.
//!
//! Currently a single-slot system (`SaveSlot(0)`), but the path layout treats the
//! slot as an index so additional slots can be added later without changing
//! the on-disk format.
//!
//! Lifecycle:
//! 1. **PreStartup** — [`try_load_save_file`] reads the active slot's save file. If
//!    valid, it inserts a [`LoadedSave`] resource and the rest of the world is rebuilt
//!    from that snapshot.
//! 2. **PreStartup** — [`crate::level::spawn_level`] reads [`LoadedSave`] (if present)
//!    instead of generating a fresh map, so tile geometry and spawn points match the
//!    saved game.
//! 3. **PostStartup** — [`apply_loaded_save`] patches the live default-spawned entities
//!    (player, chests, door, NPC) with their saved state and spawns the dynamic ones
//!    (skeletons, landed arrows). The [`LoadedSave`] resource is then removed.
//! 4. **Update** — when the player chooses Save & Quit, [`SaveAndExitRequested`] is
//!    sent. [`process_save_and_exit`] serializes the world to disk and writes
//!    [`AppExit::Success`].

use std::fs;
use std::path::PathBuf;

use bevy::app::AppExit;
use bevy::ecs::message::{Message, MessageReader, MessageWriter};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::arrow::{spawn_landed_arrow_entity, Arrow, ArrowState, ArrowVisual};
use crate::chest::{Chest, KeyChest, WeaponChest};
use crate::damageable::Damageable;
use crate::door::LockedDoor;
use crate::entity::EntityLibrary;
use crate::inventory::EquippedHotbarSlot;
use crate::item::{Inventory, ItemStack};
use crate::level::{
    CampfireSpawnPoint, DoorOrientation, KeyChestSpawnPoint, LadderSpawnPoint,
    LadderUpSpawnPoint, LevelTiles, LockedDoorSpawnPoint, NpcSpawnPoint, PlayerSpawnPoint,
    SignpostSpawnPoint, SpawnerSpawnPoint, WeaponChestSpawnPoint,
};
use crate::npc::Npc;
use crate::player_input::{Facing, PlayerControlled};
use crate::skeleton::{spawn_skeleton_entity, Skeleton};
use crate::spawner::Spawner;
use crate::sprite_animation::SpriteAnimation;
use avian2d::prelude::Sensor;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Schema version for save files. Bump when [`SaveData`] layout changes — older
/// versions are then rejected by [`try_load_save_file`] and the game regenerates.
pub const SAVE_SCHEMA_VERSION: u32 = 1;

/// Active save slot index. `0` is the only slot used today; the resource exists so
/// future UI can switch slots without changing path-formatting code.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SaveSlot(pub usize);

impl Default for SaveSlot {
    fn default() -> Self { Self(0) }
}

/// Inserted by [`try_load_save_file`] when a valid save exists for the active slot.
///
/// Read in PreStartup by [`crate::level::spawn_level`] (to skip generation) and in
/// PostStartup by [`apply_loaded_save`] (to patch entity state). Removed at the end
/// of PostStartup so subsequent save writes capture the live world.
#[derive(Resource, Debug)]
pub struct LoadedSave(pub SaveData);

/// Sent when the player clicks Save & Quit. The handler serializes the current
/// world to the active slot, then writes [`AppExit::Success`].
#[derive(Message, Debug, Default)]
pub struct SaveAndExitRequested;

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// Top-level RON document persisted to disk.
#[derive(Serialize, Deserialize, Debug)]
pub struct SaveData {
    pub version: u32,
    pub seed: u64,
    pub level: LevelSnapshot,
    pub player: PlayerSnapshot,
    pub equipped_hotbar: Option<usize>,
    pub chests: Vec<ChestSnapshot>,
    pub door: Option<DoorSnapshot>,
    pub npc: Option<NpcSnapshot>,
    pub skeletons: Vec<SkeletonSnapshot>,
    pub arrows: Vec<ArrowSnapshot>,
}

/// Tile grid + spawn point positions for the current level.
#[derive(Serialize, Deserialize, Debug)]
pub struct LevelSnapshot {
    pub width: usize,
    pub height: usize,
    /// Row-major walkability flags. `true` = floor, `false` = wall.
    pub walkable: Vec<bool>,
    pub player_start: (usize, usize),
    pub campfire_spawn: (usize, usize),
    pub signpost_spawn: (usize, usize),
    pub npc_spawn: (usize, usize),
    pub weapon_chest_spawn: (usize, usize),
    pub key_chest_spawn: (usize, usize),
    pub locked_door_pos: (usize, usize),
    pub locked_door_orientation: DoorOrientationSerde,
    pub ladder_pos: (usize, usize),
    pub ladder_up_pos: (usize, usize),
    pub spawner_pos: (usize, usize),
}

/// Serializable mirror of [`DoorOrientation`] (which is not itself `Serialize`).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum DoorOrientationSerde { NorthSouth, EastWest }

impl From<DoorOrientation> for DoorOrientationSerde {
    fn from(o: DoorOrientation) -> Self {
        match o {
            DoorOrientation::NorthSouth => Self::NorthSouth,
            DoorOrientation::EastWest => Self::EastWest,
        }
    }
}

impl From<DoorOrientationSerde> for DoorOrientation {
    fn from(o: DoorOrientationSerde) -> Self {
        match o {
            DoorOrientationSerde::NorthSouth => Self::NorthSouth,
            DoorOrientationSerde::EastWest => Self::EastWest,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerSnapshot {
    pub pos: [f32; 2],
    pub facing: FacingSerde,
    pub inventory: InventorySnapshot,
}

/// Serializable mirror of [`Facing`] (avoids depending on the runtime enum's `Reflect`
/// machinery for serde derive).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum FacingSerde { East, West, North, South }

impl From<Facing> for FacingSerde {
    fn from(f: Facing) -> Self {
        match f {
            Facing::East => Self::East,
            Facing::West => Self::West,
            Facing::North => Self::North,
            Facing::South => Self::South,
        }
    }
}

impl From<FacingSerde> for Facing {
    fn from(f: FacingSerde) -> Self {
        match f {
            FacingSerde::East => Self::East,
            FacingSerde::West => Self::West,
            FacingSerde::North => Self::North,
            FacingSerde::South => Self::South,
        }
    }
}

/// Capacity-preserving snapshot of an [`Inventory`]. Empty slots are encoded as `None`.
#[derive(Serialize, Deserialize, Debug)]
pub struct InventorySnapshot {
    pub capacity: usize,
    pub slots: Vec<Option<ItemStackSerde>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ItemStackSerde {
    pub id: String,
    pub count: u32,
}

impl InventorySnapshot {
    /// Captures the current state of an [`Inventory`] for serialization.
    pub fn from_inventory(inv: &Inventory) -> Self {
        let capacity = inv.len();
        let slots = (0..capacity)
            .map(|i| {
                inv.get(i).map(|s| ItemStackSerde {
                    id: s.id.clone(),
                    count: s.count,
                })
            })
            .collect();
        Self { capacity, slots }
    }

    /// Builds a fresh [`Inventory`] from this snapshot.
    pub fn into_inventory(self) -> Inventory {
        let mut inv = Inventory::new(self.capacity);
        for (i, slot) in self.slots.into_iter().enumerate() {
            if let Some(stack) = slot {
                let _ = inv.put(i, Some(ItemStack::new(stack.id, stack.count)));
            }
        }
        inv
    }
}

/// Identifies which level chest a [`ChestSnapshot`] applies to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ChestKindSerde { Weapon, Key }

#[derive(Serialize, Deserialize, Debug)]
pub struct ChestSnapshot {
    pub kind: ChestKindSerde,
    pub is_open: bool,
    pub inventory: InventorySnapshot,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DoorSnapshot {
    pub locked: bool,
    pub is_open: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NpcSnapshot {
    pub pos: [f32; 2],
    pub flip_x: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SkeletonSnapshot {
    pub pos: [f32; 2],
    pub flip_x: bool,
    /// Accumulated damage when the save was written. Restored onto the spawned
    /// skeleton's [`Damageable`] so a wounded enemy stays wounded across save/load.
    /// Defaults to `0` for older save files written before HP was tracked.
    #[serde(default)]
    pub damage: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ArrowSnapshot {
    pub pos: [f32; 2],
    /// Z-axis rotation (radians) of the visual sprite — preserves the firing direction
    /// of the original shot so reloaded arrows look like they did when they landed.
    pub rotation_z: f32,
}

// ---------------------------------------------------------------------------
// System set
// ---------------------------------------------------------------------------

/// All save-load PreStartup work runs in this set so other plugins can order
/// themselves with `.after(SaveLoadSet)`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SaveLoadSet;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Wires up save-file IO, load-time world hydration, and the Save & Quit message handler.
pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveSlot>()
            .add_message::<SaveAndExitRequested>()
            .add_systems(PreStartup, try_load_save_file.in_set(SaveLoadSet))
            .add_systems(PostStartup, apply_loaded_save)
            .add_systems(Update, process_save_and_exit);
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the on-disk path for the save file at `slot`.
///
/// Resolves to the OS user-data directory (`~/Library/Application Support/cavelight/...`
/// on macOS, `~/.local/share/cavelight/...` on Linux, `%APPDATA%\cavelight\...` on
/// Windows). Returns `None` if the platform does not expose a data directory.
pub fn save_file_path(slot: usize) -> Option<PathBuf> {
    let mut p = dirs::data_dir()?;
    p.push("cavelight");
    p.push("saves");
    p.push(slot.to_string());
    p.push("save.ron");
    Some(p)
}

// ---------------------------------------------------------------------------
// PreStartup: load
// ---------------------------------------------------------------------------

/// Reads the save file for the active slot and inserts [`LoadedSave`] on success.
///
/// Failure modes (file absent, IO error, parse error, version mismatch) all result
/// in no [`LoadedSave`] resource being inserted — downstream systems then proceed
/// with normal world generation. Errors are logged but never fatal.
fn try_load_save_file(mut commands: Commands, slot: Res<SaveSlot>) {
    let Some(path) = save_file_path(slot.0) else {
        warn!("No data directory available on this platform — running without persistence.");
        return;
    };

    if !path.exists() {
        info!("No save file at {} — starting a fresh game.", path.display());
        return;
    }

    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to read save file {}: {e}. Starting fresh.", path.display());
            return;
        }
    };

    let data: SaveData = match ron::from_str(&raw) {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to parse save file {}: {e}. Starting fresh.", path.display());
            return;
        }
    };

    if data.version != SAVE_SCHEMA_VERSION {
        warn!(
            "Save schema version mismatch (file v{}, expected v{}). Starting fresh.",
            data.version, SAVE_SCHEMA_VERSION
        );
        return;
    }

    info!(
        "Loaded save from {} (seed {}, {} chest(s), {} skeleton(s), {} arrow(s)).",
        path.display(),
        data.seed,
        data.chests.len(),
        data.skeletons.len(),
        data.arrows.len(),
    );
    commands.insert_resource(LoadedSave(data));
}

// ---------------------------------------------------------------------------
// PostStartup: apply
// ---------------------------------------------------------------------------

/// Applies a loaded snapshot to the just-spawned world.
///
/// Runs in PostStartup, after every plugin's Startup spawners have created the
/// default-state versions of player / chests / door / NPC / spawner. This system
/// then patches their state to match the snapshot and adds dynamic entities
/// (skeletons, landed arrows) that don't have Startup spawners.
///
/// Removes the [`LoadedSave`] resource at the end so subsequent saves see the
/// live world rather than the snapshot.
#[allow(clippy::too_many_arguments)]
fn apply_loaded_save(
    mut commands: Commands,
    loaded: Option<Res<LoadedSave>>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut equipped: ResMut<EquippedHotbarSlot>,
    mut player: Query<
        (&mut Transform, &mut Facing, &mut Inventory, &mut Sprite),
        (With<PlayerControlled>, Without<Chest>, Without<LockedDoor>, Without<Npc>),
    >,
    mut weapon_chest: Query<
        (Entity, &mut Chest, &mut Inventory, &mut Sprite),
        (With<WeaponChest>, Without<PlayerControlled>, Without<LockedDoor>, Without<Npc>, Without<KeyChest>),
    >,
    mut key_chest: Query<
        (Entity, &mut Chest, &mut Inventory, &mut Sprite),
        (With<KeyChest>, Without<PlayerControlled>, Without<LockedDoor>, Without<Npc>, Without<WeaponChest>),
    >,
    mut door: Query<
        (Entity, &mut LockedDoor, &mut SpriteAnimation),
        (Without<PlayerControlled>, Without<Chest>, Without<Npc>),
    >,
    mut npc: Query<
        (&mut Transform, &mut Sprite),
        (With<Npc>, Without<PlayerControlled>, Without<Chest>, Without<LockedDoor>),
    >,
    spawner: Query<Entity, With<Spawner>>,
    entity_library: Option<Res<EntityLibrary>>,
) {
    let Some(loaded) = loaded else { return };
    // Move the snapshot out of the resource so we can consume owned fields.
    let SaveData {
        player: player_snap,
        equipped_hotbar,
        chests,
        door: door_snap,
        npc: npc_snap,
        skeletons,
        arrows,
        ..
    } = loaded.0.clone_for_apply();

    // --- Player ---
    if let Ok((mut transform, mut facing, mut inv, mut sprite)) = player.single_mut() {
        transform.translation.x = player_snap.pos[0];
        transform.translation.y = player_snap.pos[1];
        *facing = Facing::from(player_snap.facing);
        sprite.flip_x = matches!(*facing, Facing::West);
        let new_inv = player_snap.inventory.into_inventory();
        // Replace contents in-place to preserve component identity / capacity.
        for i in 0..inv.len().max(new_inv.len()) {
            let stack = new_inv.get(i).cloned();
            let _ = inv.put(i, stack);
        }
    }
    equipped.0 = equipped_hotbar;

    // --- Chests ---
    for chest_snap in chests {
        match chest_snap.kind {
            ChestKindSerde::Weapon => {
                if let Ok((_e, mut chest, mut inv, mut sprite)) = weapon_chest.single_mut() {
                    apply_chest_state(&mut chest, &mut inv, &mut sprite, chest_snap);
                }
            }
            ChestKindSerde::Key => {
                if let Ok((_e, mut chest, mut inv, mut sprite)) = key_chest.single_mut() {
                    apply_chest_state(&mut chest, &mut inv, &mut sprite, chest_snap);
                }
            }
        }
    }

    // --- Door ---
    if let (Some(snap), Ok((entity, mut door, mut anim))) = (door_snap, door.single_mut()) {
        door.locked = snap.locked;
        if snap.is_open {
            let open = door.open_anim();
            anim.switch_to(open);
            door.is_open = true;
            commands.entity(entity).insert(Sensor);
        } else {
            let closed = door.closed_anim();
            anim.switch_to(closed);
            door.is_open = false;
            commands.entity(entity).remove::<Sensor>();
        }
    }

    // --- NPC ---
    if let (Some(snap), Ok((mut transform, mut sprite))) = (npc_snap, npc.single_mut()) {
        transform.translation.x = snap.pos[0];
        transform.translation.y = snap.pos[1];
        sprite.flip_x = snap.flip_x;
    }

    // --- Skeletons ---
    let spawner_entity = spawner.iter().next();
    if let Some(spawner_entity) = spawner_entity {
        let library = entity_library.as_deref();
        for snap in skeletons {
            let pos = Vec2::new(snap.pos[0], snap.pos[1]);
            let entity = spawn_skeleton_entity(
                &mut commands,
                &asset_server,
                &mut layouts,
                library,
                pos,
                spawner_entity,
            );
            // Restore wounded HP after the entity is materialised. We can't mutate
            // the [`Damageable`] inline because the spawn is queued via [`Commands`].
            let damage = snap.damage;
            let flip_x = snap.flip_x;
            commands.queue(move |world: &mut World| {
                if flip_x {
                    if let Some(mut sprite) = world.get_mut::<Sprite>(entity) {
                        sprite.flip_x = true;
                    }
                }
                if damage > 0 {
                    if let Some(mut d) = world.get_mut::<Damageable>(entity) {
                        d.damage = damage;
                    }
                }
            });
        }
    } else if !skeletons.is_empty() {
        warn!("Loaded save contains skeletons but no Spawner entity exists — skeletons skipped.");
    }

    // --- Landed arrows ---
    for snap in arrows {
        let pos = Vec2::new(snap.pos[0], snap.pos[1]);
        spawn_landed_arrow_entity(&mut commands, &asset_server, &mut layouts, pos, snap.rotation_z);
    }

    commands.remove_resource::<LoadedSave>();
}

/// Patches a chest entity's state to match a [`ChestSnapshot`] (open flag, sprite frame, inventory).
fn apply_chest_state(
    chest: &mut Chest,
    inv: &mut Inventory,
    sprite: &mut Sprite,
    snap: ChestSnapshot,
) {
    chest.is_open = snap.is_open;
    if let Some(atlas) = &mut sprite.texture_atlas {
        atlas.index = if chest.is_open { 4 } else { 3 };
    }
    let new_inv = snap.inventory.into_inventory();
    for i in 0..inv.len().max(new_inv.len()) {
        let stack = new_inv.get(i).cloned();
        let _ = inv.put(i, stack);
    }
}

// ---------------------------------------------------------------------------
// Update: save trigger
// ---------------------------------------------------------------------------

/// Bundle of every level-setup resource consumed when serializing a save.
///
/// Wrapping these as a single [`SystemParam`] keeps [`process_save_and_exit`] under
/// Bevy's 16-parameter limit while preserving the read-only access pattern.
#[derive(SystemParam)]
struct LevelSnapshotParams<'w> {
    tiles: Option<Res<'w, LevelTiles>>,
    player_spawn: Option<Res<'w, PlayerSpawnPoint>>,
    campfire_spawn: Option<Res<'w, CampfireSpawnPoint>>,
    signpost_spawn: Option<Res<'w, SignpostSpawnPoint>>,
    npc_spawn: Option<Res<'w, NpcSpawnPoint>>,
    weapon_chest_spawn: Option<Res<'w, WeaponChestSpawnPoint>>,
    key_chest_spawn: Option<Res<'w, KeyChestSpawnPoint>>,
    locked_door_spawn: Option<Res<'w, LockedDoorSpawnPoint>>,
    ladder_spawn: Option<Res<'w, LadderSpawnPoint>>,
    ladder_up_spawn: Option<Res<'w, LadderUpSpawnPoint>>,
    spawner_spawn: Option<Res<'w, SpawnerSpawnPoint>>,
}

/// Bundle of every entity query consumed when serializing a save.
#[derive(SystemParam)]
struct WorldSnapshotQueries<'w, 's> {
    player: Query<
        'w,
        's,
        (&'static Transform, &'static Facing, &'static Inventory),
        (With<PlayerControlled>, Without<Chest>, Without<LockedDoor>, Without<Npc>, Without<Skeleton>, Without<Arrow>),
    >,
    weapon_chest: Query<
        'w,
        's,
        (&'static Chest, &'static Inventory),
        (With<WeaponChest>, Without<PlayerControlled>, Without<KeyChest>),
    >,
    key_chest: Query<
        'w,
        's,
        (&'static Chest, &'static Inventory),
        (With<KeyChest>, Without<PlayerControlled>, Without<WeaponChest>),
    >,
    door: Query<'w, 's, &'static LockedDoor>,
    npc: Query<'w, 's, (&'static Transform, &'static Sprite), (With<Npc>, Without<PlayerControlled>)>,
    skeletons: Query<
        'w,
        's,
        (&'static Transform, &'static Sprite, Option<&'static Damageable>),
        (With<Skeleton>, Without<PlayerControlled>, Without<Npc>),
    >,
    arrows: Query<
        'w,
        's,
        (&'static Arrow, &'static Transform, &'static Children),
        (With<Arrow>, Without<Skeleton>),
    >,
    arrow_visual_rotation: Query<'w, 's, &'static Transform, (With<ArrowVisual>, Without<Arrow>)>,
}

/// Reads [`SaveAndExitRequested`] messages, snapshots the world to the active slot,
/// and writes [`AppExit::Success`] so the app exits at end of frame.
fn process_save_and_exit(
    mut requests: MessageReader<SaveAndExitRequested>,
    mut exit: MessageWriter<AppExit>,
    slot: Res<SaveSlot>,
    level_params: LevelSnapshotParams,
    equipped: Res<EquippedHotbarSlot>,
    queries: WorldSnapshotQueries,
) {
    if requests.is_empty() { return; }
    requests.clear();

    let data = match build_save_data(&level_params, &equipped, &queries) {
        Some(d) => d,
        None => {
            warn!("Save aborted — missing level resources. Exiting without saving.");
            exit.write(AppExit::Success);
            return;
        }
    };

    if let Err(e) = write_save_to_disk(slot.0, &data) {
        warn!("Failed to write save file: {e}. Exiting without saving.");
    } else {
        info!("Saved game to slot {}.", slot.0);
    }

    exit.write(AppExit::Success);
}

/// Builds a [`SaveData`] snapshot from the live world.
///
/// Returns `None` if any required level resource is missing — for example, when the
/// player triggers Save & Quit before [`LevelTiles`] has been inserted (which should
/// be impossible in practice but the early-out keeps the call infallible).
fn build_save_data(
    level_params: &LevelSnapshotParams,
    equipped: &Res<EquippedHotbarSlot>,
    queries: &WorldSnapshotQueries,
) -> Option<SaveData> {
    let level_tiles = level_params.tiles.as_ref()?;
    let player_spawn = level_params.player_spawn.as_ref()?;
    let campfire_spawn = level_params.campfire_spawn.as_ref()?;
    let signpost_spawn = level_params.signpost_spawn.as_ref()?;
    let npc_spawn = level_params.npc_spawn.as_ref()?;
    let weapon_chest_spawn = level_params.weapon_chest_spawn.as_ref()?;
    let key_chest_spawn = level_params.key_chest_spawn.as_ref()?;
    let locked_door_spawn = level_params.locked_door_spawn.as_ref()?;
    let ladder_spawn = level_params.ladder_spawn.as_ref()?;
    let ladder_up_spawn = level_params.ladder_up_spawn.as_ref()?;
    let spawner_spawn = level_params.spawner_spawn.as_ref()?;

    let level = LevelSnapshot {
        width: level_tiles.width(),
        height: level_tiles.height(),
        walkable: level_tiles.walkable_vec(),
        player_start: world_to_tile_unwrap(level_tiles, player_spawn.0),
        campfire_spawn: world_to_tile_unwrap(level_tiles, campfire_spawn.0),
        signpost_spawn: world_to_tile_unwrap(level_tiles, signpost_spawn.0),
        npc_spawn: world_to_tile_unwrap(level_tiles, npc_spawn.0),
        weapon_chest_spawn: world_to_tile_unwrap(level_tiles, weapon_chest_spawn.0),
        key_chest_spawn: world_to_tile_unwrap(level_tiles, key_chest_spawn.0),
        locked_door_pos: world_to_tile_unwrap(level_tiles, locked_door_spawn.pos),
        locked_door_orientation: locked_door_spawn.orientation.into(),
        ladder_pos: world_to_tile_unwrap(level_tiles, ladder_spawn.0),
        ladder_up_pos: world_to_tile_unwrap(level_tiles, ladder_up_spawn.0),
        spawner_pos: world_to_tile_unwrap(level_tiles, spawner_spawn.0),
    };

    // Player snapshot is optional — if no player exists for some reason, fall back to
    // a synthetic snapshot at the player spawn so the save remains loadable.
    let player_snap = match queries.player.single() {
        Ok((tf, facing, inv)) => PlayerSnapshot {
            pos: [tf.translation.x, tf.translation.y],
            facing: (*facing).into(),
            inventory: InventorySnapshot::from_inventory(inv),
        },
        Err(_) => PlayerSnapshot {
            pos: [player_spawn.0.x, player_spawn.0.y],
            facing: FacingSerde::East,
            inventory: InventorySnapshot { capacity: 20, slots: vec![None; 20] },
        },
    };

    let mut chests_out: Vec<ChestSnapshot> = Vec::new();
    if let Ok((chest, inv)) = queries.weapon_chest.single() {
        chests_out.push(ChestSnapshot {
            kind: ChestKindSerde::Weapon,
            is_open: chest.is_open,
            inventory: InventorySnapshot::from_inventory(inv),
        });
    }
    if let Ok((chest, inv)) = queries.key_chest.single() {
        chests_out.push(ChestSnapshot {
            kind: ChestKindSerde::Key,
            is_open: chest.is_open,
            inventory: InventorySnapshot::from_inventory(inv),
        });
    }

    let door_snap = queries.door.single().ok().map(|d| DoorSnapshot {
        locked: d.locked,
        is_open: d.is_open,
    });

    let npc_snap = queries.npc.single().ok().map(|(tf, sprite)| NpcSnapshot {
        pos: [tf.translation.x, tf.translation.y],
        flip_x: sprite.flip_x,
    });

    let skeletons_out: Vec<SkeletonSnapshot> = queries
        .skeletons
        .iter()
        .map(|(tf, sprite, damageable)| SkeletonSnapshot {
            pos: [tf.translation.x, tf.translation.y],
            flip_x: sprite.flip_x,
            damage: damageable.map(|d| d.damage).unwrap_or(0),
        })
        .collect();

    let arrows_out: Vec<ArrowSnapshot> = queries
        .arrows
        .iter()
        .filter(|(arrow, _, _)| matches!(arrow.state, ArrowState::Landed))
        .map(|(_, tf, children)| {
            // Read the visual child's rotation so reloaded arrows preserve their
            // landed-direction orientation.
            let rotation_z = children
                .iter()
                .filter_map(|child| queries.arrow_visual_rotation.get(child).ok())
                .next()
                .map(|t| t.rotation.to_euler(EulerRot::ZYX).0)
                .unwrap_or(0.0);
            ArrowSnapshot {
                pos: [tf.translation.x, tf.translation.y],
                rotation_z,
            }
        })
        .collect();

    Some(SaveData {
        version: SAVE_SCHEMA_VERSION,
        seed: 0,
        level,
        player: player_snap,
        equipped_hotbar: equipped.0,
        chests: chests_out,
        door: door_snap,
        npc: npc_snap,
        skeletons: skeletons_out,
        arrows: arrows_out,
    })
}

/// Converts a world-space position to tile coords, falling back to `(0, 0)` if outside
/// the level. Out-of-bounds spawn points should never happen in practice but the
/// fallback keeps the save infallible rather than panicking on an edge case.
fn world_to_tile_unwrap(level: &LevelTiles, pos: Vec2) -> (usize, usize) {
    level.world_to_tile(pos).unwrap_or((0, 0))
}

/// Serializes `data` as pretty RON and writes it to the active slot's save path,
/// creating parent directories as needed.
fn write_save_to_disk(slot: usize, data: &SaveData) -> std::io::Result<()> {
    let path = save_file_path(slot)
        .ok_or_else(|| std::io::Error::other("no data directory on this platform"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let pretty = ron::ser::PrettyConfig::new();
    let serialized = ron::ser::to_string_pretty(data, pretty)
        .map_err(std::io::Error::other)?;
    fs::write(&path, serialized)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers — make snapshot consumable from a Resource borrow
// ---------------------------------------------------------------------------

impl SaveData {
    /// Produces an owned copy of this snapshot for use during the apply phase.
    ///
    /// `apply_loaded_save` borrows [`LoadedSave`] immutably (because it also holds
    /// many other queries / resources mutably) but needs to consume `Vec` fields
    /// like the chest list. Cloning here is cheap relative to entity spawning.
    fn clone_for_apply(&self) -> SaveData {
        // Manual clone rather than deriving Clone — keeps Clone out of the public API
        // while letting the apply system own the data.
        SaveData {
            version: self.version,
            seed: self.seed,
            level: LevelSnapshot {
                width: self.level.width,
                height: self.level.height,
                walkable: self.level.walkable.clone(),
                player_start: self.level.player_start,
                campfire_spawn: self.level.campfire_spawn,
                signpost_spawn: self.level.signpost_spawn,
                npc_spawn: self.level.npc_spawn,
                weapon_chest_spawn: self.level.weapon_chest_spawn,
                key_chest_spawn: self.level.key_chest_spawn,
                locked_door_pos: self.level.locked_door_pos,
                locked_door_orientation: self.level.locked_door_orientation,
                ladder_pos: self.level.ladder_pos,
                ladder_up_pos: self.level.ladder_up_pos,
                spawner_pos: self.level.spawner_pos,
            },
            player: PlayerSnapshot {
                pos: self.player.pos,
                facing: self.player.facing,
                inventory: InventorySnapshot {
                    capacity: self.player.inventory.capacity,
                    slots: self.player.inventory.slots.clone(),
                },
            },
            equipped_hotbar: self.equipped_hotbar,
            chests: self.chests.iter().map(|c| ChestSnapshot {
                kind: c.kind,
                is_open: c.is_open,
                inventory: InventorySnapshot {
                    capacity: c.inventory.capacity,
                    slots: c.inventory.slots.clone(),
                },
            }).collect(),
            door: self.door.as_ref().map(|d| DoorSnapshot { locked: d.locked, is_open: d.is_open }),
            npc: self.npc.as_ref().map(|n| NpcSnapshot { pos: n.pos, flip_x: n.flip_x }),
            skeletons: self.skeletons.iter().map(|s| SkeletonSnapshot {
                pos: s.pos,
                flip_x: s.flip_x,
                damage: s.damage,
            }).collect(),
            arrows: self.arrows.iter().map(|a| ArrowSnapshot { pos: a.pos, rotation_z: a.rotation_z }).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_snapshot_round_trips() {
        let mut inv = Inventory::new(4);
        inv.put(0, Some(ItemStack::new("arrow", 8))).unwrap();
        inv.put(2, Some(ItemStack::new("bow", 1))).unwrap();

        let snap = InventorySnapshot::from_inventory(&inv);
        let restored = snap.into_inventory();

        assert_eq!(restored.len(), 4);
        assert_eq!(restored.get(0).unwrap().id, "arrow");
        assert_eq!(restored.get(0).unwrap().count, 8);
        assert!(restored.get(1).is_none());
        assert_eq!(restored.get(2).unwrap().id, "bow");
        assert!(restored.get(3).is_none());
    }

    #[test]
    fn facing_serde_round_trips() {
        for f in [Facing::East, Facing::West, Facing::North, Facing::South] {
            let s: FacingSerde = f.into();
            let back: Facing = s.into();
            assert_eq!(f, back);
        }
    }

    #[test]
    fn door_orientation_serde_round_trips() {
        for o in [DoorOrientation::NorthSouth, DoorOrientation::EastWest] {
            let s: DoorOrientationSerde = o.into();
            let back: DoorOrientation = s.into();
            // Compare by their serde discriminant since DoorOrientation is not PartialEq-friendly here.
            let s2: DoorOrientationSerde = back.into();
            // round-trip to a comparable form
            let json_a = format!("{:?}", s);
            let json_b = format!("{:?}", s2);
            assert_eq!(json_a, json_b);
        }
    }

    #[test]
    fn save_path_is_under_data_dir_when_available() {
        if let Some(p) = save_file_path(0) {
            let s = p.to_string_lossy();
            assert!(s.contains("cavelight"));
            assert!(s.contains("saves"));
            assert!(s.ends_with("save.ron"));
        }
    }
}
