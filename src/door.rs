use avian2d::prelude::*;
use bevy::prelude::*;

use crate::dialogue::ActiveDialogue;
use crate::interaction::{Interactable, InteractEvent};
use crate::inventory::InputMode;
use crate::item::Inventory;
use crate::level::{DoorOrientation, LockedDoorSpawnPoint};
use crate::player_input::PlayerControlled;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// State and display names for a door entity.
///
/// Doors start locked and closed. Using a key unlocks AND opens the door.
/// After unlocking, interaction toggles between open and closed.
/// Closed doors have a static [`Collider`] that blocks all [`crate::grid_mover::GridMover`]
/// entities — player, NPCs, and enemies alike.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct LockedDoor {
    /// `true` until the player unlocks the door with a key. A locked door cannot
    /// be toggled — the player must use a key first.
    pub locked: bool,
    /// Current open/closed state. `true` = passable, `false` = solid.
    pub is_open: bool,
    /// Animation name for the closed sprite.
    closed_animation: &'static str,
    /// Animation name for the open sprite.
    open_animation: &'static str,
}

impl LockedDoor {
    /// Returns the animation name played when the door is closed.
    pub fn closed_anim(&self) -> &'static str { self.closed_animation }
    /// Returns the animation name played when the door is open.
    pub fn open_anim(&self) -> &'static str { self.open_animation }
}

/// Spawns the locked door and registers its interaction observer.
pub struct DoorPlugin;

impl Plugin for DoorPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<LockedDoor>()
            .add_systems(Startup, spawn_door)
            .add_observer(on_door_interact);
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the locked door at [`LockedDoorSpawnPoint`].
///
/// Chooses the correct closed-sprite and open-sprite animation based on corridor
/// orientation. The door starts locked, closed, and solid.
fn spawn_door(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<LockedDoorSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    let (closed_anim, open_anim, closed_frame) = match spawn_point.orientation {
        DoorOrientation::NorthSouth => ("door_northsouth_closed", "door_northsouth_open", 146usize),
        DoorOrientation::EastWest   => ("door_eastwest_closed",   "door_eastwest_open",   145usize),
    };

    commands.spawn((
        LockedDoor {
            locked: true,
            is_open: false,
            closed_animation: closed_anim,
            open_animation: open_anim,
        },
        Interactable,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: closed_frame },
        ),
        Transform::from_xyz(spawn_point.pos.x, spawn_point.pos.y, 0.0),
        SpriteAnimation::with_name(closed_anim, false),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}

// ---------------------------------------------------------------------------
// Interaction observer
// ---------------------------------------------------------------------------

/// Handles player interaction with the door.
///
/// **Locked door:**
/// - Player has a key → consume the key, unlock, and open the door.
/// - Player has no key → show a "locked" dialogue.
///
/// **Unlocked door:**
/// - Toggles between open (passable, no collider) and closed (solid, collider restored).
fn on_door_interact(
    on: On<InteractEvent>,
    mut doors: Query<(&mut LockedDoor, &mut SpriteAnimation)>,
    mut player_inventory: Query<&mut Inventory, With<PlayerControlled>>,
    mut active_dialogue: ResMut<ActiveDialogue>,
    mut input_mode: ResMut<InputMode>,
    mut commands: Commands,
) {
    let entity = on.event().entity;
    let Ok((mut door, mut anim)) = doors.get_mut(entity) else { return };

    if door.locked {
        let Ok(mut inventory) = player_inventory.single_mut() else { return };

        // Search the player's inventory for a key.
        let key_slot = (0..inventory.len()).find(|&i| {
            inventory.get(i).map_or(false, |s| s.id == "key")
        });

        match key_slot {
            Some(slot) => {
                inventory.take(slot);
                door.locked = false;
                // Unlocking always opens the door immediately.
                set_door_open(&mut door, &mut anim, entity, &mut commands);
            }
            None => {
                active_dialogue.open(
                    "Locked Door",
                    vec!["The door is locked. You'll need a key.".to_string()],
                    &mut input_mode,
                );
            }
        }
    } else {
        // Toggle open ↔ closed.
        if door.is_open {
            set_door_closed(&mut door, &mut anim, entity, &mut commands);
        } else {
            set_door_open(&mut door, &mut anim, entity, &mut commands);
        }
    }
}

/// Opens the door: swaps to the open animation and marks the collider as a sensor.
///
/// The [`RigidBody::Static`] and [`Collider`] components are kept at all times so the
/// physics world always has a shape to query — removing and re-adding them would create
/// a window where [`SpatialQuery::point_intersections`] misses the door, causing dropped
/// interaction inputs. Adding [`Sensor`] makes the collider passable while still
/// detectable; [`crate::grid_mover::GridMover`] already skips sensor entities.
fn set_door_open(
    door: &mut LockedDoor,
    anim: &mut SpriteAnimation,
    entity: Entity,
    commands: &mut Commands,
) {
    door.is_open = true;
    anim.switch_to(door.open_animation);
    commands.entity(entity).insert(Sensor);
}

/// Closes the door: swaps to the closed animation and removes the sensor flag,
/// restoring the collider to a solid blocking shape.
fn set_door_closed(
    door: &mut LockedDoor,
    anim: &mut SpriteAnimation,
    entity: Entity,
    commands: &mut Commands,
) {
    door.is_open = false;
    anim.switch_to(door.closed_animation);
    commands.entity(entity).remove::<Sensor>();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_door_starts_locked() {
        let door = LockedDoor {
            locked: true,
            is_open: false,
            closed_animation: "door_northsouth_closed",
            open_animation: "door_northsouth_open",
        };
        assert!(door.locked);
        assert!(!door.is_open);
    }

    #[test]
    fn locked_door_eastwest_variant_exists() {
        let door = LockedDoor {
            locked: true,
            is_open: false,
            closed_animation: "door_eastwest_closed",
            open_animation: "door_eastwest_open",
        };
        assert_eq!(door.closed_animation, "door_eastwest_closed");
    }
}
