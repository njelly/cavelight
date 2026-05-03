use bevy::prelude::*;

use crate::input::{ActionInput, GameAction, InputSource};
use crate::inventory::{EquippedHotbarSlot, InputMode, HOTBAR_START};
use crate::item::{Inventory, ItemLibrary};
use crate::player_input::{Facing, PlayerControlled};
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// Right stick dead-zone for gamepad aim direction.
const AIM_STICK_DEADZONE: f32 = 0.3;

/// Distance from the player center at which the aim indicator orbits, in world units.
const ORBIT_RADIUS: f32 = GRID_SIZE;

/// Marks the orbiting aim indicator sprite entity.
///
/// Spawned once at startup and repositioned each frame. Visible only when the player
/// is holding Shift with an ammo-using item equipped in the hotbar.
#[derive(Component)]
pub struct AimIndicator;

/// Drives the aim indicator sprite that orbits the player toward the mouse cursor.
///
/// When the player has an item with `ammo_id` equipped and holds Shift, the indicator
/// becomes visible and rotates to point from the player toward the cursor position.
pub struct AimPlugin;

impl Plugin for AimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_aim_indicator)
            .add_systems(Update, update_aim);
    }
}

/// Spawns the aim indicator sprite as a hidden entity.
///
/// Uses atlas frame 22 (`direction_arrow_east`) from `atlas_8x8.png`.
fn spawn_aim_indicator(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        AimIndicator,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: 22 },
        ),
        Transform::from_xyz(0.0, 0.0, 1.0),
        Visibility::Hidden,
        SpriteAnimation::with_name("direction_arrow_east", false),
    ));
}

/// Positions the aim indicator to orbit the player toward the aim direction.
///
/// Active only while `InputMode::Playing`, Aim is held (Shift or RT), and the equipped
/// hotbar item has a non-`None` `ammo_id`. Hides the indicator when any condition fails.
///
/// Direction is resolved from the active input source:
/// - `KeyboardMouse` → mouse cursor world position relative to the player.
/// - `Gamepad` → left stick deflection (the movement stick doubles as aim while RT is held),
///   falling back to the player's current `Facing`.
fn update_aim(
    input_mode: Res<InputMode>,
    action_input: Res<ActionInput>,
    input_source: Res<InputSource>,
    equipped: Res<EquippedHotbarSlot>,
    item_library: Option<Res<ItemLibrary>>,
    player_query: Query<(&Transform, &Inventory, &Facing), With<PlayerControlled>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    window_query: Query<&Window>,
    gamepads: Query<&Gamepad>,
    mut indicator_query: Query<(&mut Transform, &mut Visibility), (With<AimIndicator>, Without<PlayerControlled>)>,
) {
    let Ok((mut ind_tf, mut ind_vis)) = indicator_query.single_mut() else { return };

    let aiming = *input_mode == InputMode::Playing
        && action_input.pressed(GameAction::Aim)
        && equipped.0.is_some()
        && check_has_ammo(&equipped, &item_library, &player_query);

    if !aiming {
        *ind_vis = Visibility::Hidden;
        return;
    }

    let Ok((player_tf, _, facing)) = player_query.single() else {
        *ind_vis = Visibility::Hidden;
        return;
    };
    let player_pos = player_tf.translation.truncate();

    let direction = match *input_source {
        InputSource::KeyboardMouse => {
            // Aim toward the mouse cursor in world space.
            let dir = (|| -> Option<Vec2> {
                let (camera, camera_gtf) = camera_query.single().ok()?;
                let window = window_query.single().ok()?;
                let cursor_screen = window.cursor_position()?;
                let cursor_world = camera.viewport_to_world_2d(camera_gtf, cursor_screen).ok()?;
                let d = (cursor_world - player_pos).normalize_or_zero();
                if d == Vec2::ZERO { None } else { Some(d) }
            })();
            match dir {
                Some(d) => d,
                None => { *ind_vis = Visibility::Hidden; return; }
            }
        }
        InputSource::Gamepad => {
            // Aim toward the left stick (movement stick); fall back to facing direction.
            let stick = gamepads.iter().find_map(|gp| {
                let x = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
                let y = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
                let v = Vec2::new(x, y);
                if v.length() > AIM_STICK_DEADZONE { Some(v.normalize()) } else { None }
            });
            stick.unwrap_or_else(|| facing.offset())
        }
    };

    let orbit_pos = player_pos + direction * ORBIT_RADIUS;
    ind_tf.translation = orbit_pos.extend(1.0);
    ind_tf.rotation = Quat::from_rotation_z(direction.to_angle());
    *ind_vis = Visibility::Inherited;
}

/// Returns `true` if the currently equipped hotbar item has a non-`None` `ammo_id`.
fn check_has_ammo(
    equipped: &EquippedHotbarSlot,
    item_library: &Option<Res<ItemLibrary>>,
    player_query: &Query<(&Transform, &Inventory, &Facing), With<PlayerControlled>>,
) -> bool {
    let Some(hotbar_idx) = equipped.0 else { return false };
    let Some(library) = item_library.as_ref() else { return false };
    let Ok((_, inventory, _)) = player_query.single() else { return false };
    let slot_index = HOTBAR_START + hotbar_idx;
    let Some(stack) = inventory.get(slot_index) else { return false };
    library.def(&stack.id).and_then(|d| d.ammo_id.as_ref()).is_some()
}
