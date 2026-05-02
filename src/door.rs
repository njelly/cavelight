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

/// Marks the locked door entity.
///
/// A door starts locked. When the player interacts with it while holding a key the
/// key is consumed, the door opens (sprite swaps to the open frame and the collider
/// is removed so the player can pass), and the entity is no longer [`Interactable`].
///
/// If the player interacts without a key the dialogue system shows a "locked" message.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct LockedDoor {
    /// `true` until the player unlocks the door with a key.
    pub locked: bool,
    /// Sprite animation name to swap to when the door opens.
    open_animation: &'static str,
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
/// The door is a static solid entity. Its sprite and open animation are chosen
/// based on the corridor orientation reported by the level generator.
/// [`Interactable`] allows the player to trigger an interaction by pressing Space
/// while facing the door tile.
fn spawn_door(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<LockedDoorSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    // Level 1 only has north-south corridors. Additional orientations will be
    // added alongside new DoorOrientation variants when east-west doors are needed.
    let (closed_anim, open_anim, frame_idx) = match spawn_point.orientation {
        DoorOrientation::NorthSouth => ("door_northsouth_closed", "door_northsouth_open", 146usize),
    };

    commands.spawn((
        LockedDoor { locked: true, open_animation: open_anim },
        Interactable,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: frame_idx },
        ),
        Transform::from_xyz(spawn_point.pos.x, spawn_point.pos.y, 0.0),
        SpriteAnimation::with_name(closed_anim, true),
        RigidBody::Static,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
    ));
}

// ---------------------------------------------------------------------------
// Interaction observer
// ---------------------------------------------------------------------------

/// Handles player interaction with the locked door.
///
/// - **Key in inventory**: consumes the key, switches to the open sprite, removes
///   the collider and [`Interactable`] so the player can walk through and the door
///   can no longer be re-triggered.
/// - **No key**: opens the dialogue panel with a "locked" message.
fn on_door_interact(
    on: On<InteractEvent>,
    mut doors: Query<(&mut LockedDoor, &mut Sprite)>,
    mut player_inventory: Query<&mut Inventory, With<PlayerControlled>>,
    mut active_dialogue: ResMut<ActiveDialogue>,
    mut input_mode: ResMut<InputMode>,
    mut commands: Commands,
) {
    let Ok((mut door, mut sprite)) = doors.get_mut(on.event().entity) else { return };
    if !door.locked {
        return;
    }

    let Ok(mut inventory) = player_inventory.single_mut() else { return };

    // Search the player's inventory for a key stack.
    let key_slot = (0..inventory.len()).find(|&slot| {
        inventory.get(slot).map_or(false, |s| s.id == "key")
    });

    if let Some(slot) = key_slot {
        // Consume the key and open the door.
        inventory.take(slot);
        door.locked = false;

        // Swap the sprite to the open animation.
        if let Some(atlas) = &mut sprite.texture_atlas {
            // Open-frame index is the frame immediately after the closed one in the atlas row.
            // NorthSouth open = 210, EastWest open = 209 (from sprite_animations.ron).
            atlas.index = match door.open_animation {
                "door_northsouth_open" => 210,
                _ => 209,
            };
        }

        // Remove the collider and interactable so the player can walk through.
        commands
            .entity(on.event().entity)
            .remove::<(Collider, RigidBody, Interactable)>();
    } else {
        // No key — show the locked message via the dialogue system.
        active_dialogue.open(
            "Locked Door",
            vec!["The door is locked. You'll need a key.".to_string()],
            &mut input_mode,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_door_starts_locked() {
        let door = LockedDoor { locked: true, open_animation: "door_northsouth_open" };
        assert!(door.locked);
    }
}
