use bevy::prelude::*;
use bevy::sprite::Anchor;

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

/// Seconds of continuous aiming required to reach full charge.
const CHARGE_DURATION: f32 = 1.0;

/// Arrow alpha for the dim background layer (uncharged state).
const ARROW_ALPHA_BG: f32 = 0.2;

/// Arrow alpha for the bright fill layer (charged state).
const ARROW_ALPHA_FILL: f32 = 0.8;

/// Live aim state shared with downstream systems (e.g. the bow shooting system).
///
/// Updated every frame by [`update_aim`]. When `active` is `true`, the player is
/// currently aiming a ranged weapon and `direction`, `origin`, and `charge` reflect
/// the current shot parameters.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct AimState {
    /// `true` when the aim indicator is visible and the player can fire this frame.
    pub active: bool,
    /// Charge fraction in `[0.0, 1.0]` — 1.0 means a full-power shot.
    pub charge: f32,
    /// World-space unit direction of the aim, from the player toward the cursor / stick.
    pub direction: Vec2,
    /// World-space position of the player at the time the aim was sampled.
    pub origin: Vec2,
}

impl AimState {
    /// Resets the charge accumulator without changing the active flag.
    ///
    /// Called by the shooting system after firing so the next shot starts fresh.
    pub fn reset_charge(&mut self) {
        self.charge = 0.0;
    }
}

/// Marks the dim background arrow sprite that shows the full indicator at low opacity.
///
/// Spawned once at startup and repositioned each frame. Visible only when the player
/// is holding Shift with an ammo-using item equipped in the hotbar.
#[derive(Component)]
pub struct AimIndicator;

/// Marks the bright fill arrow sprite that grows from tail to tip as charge builds.
///
/// Uses [`Anchor::CENTER_LEFT`] so its pivot is at the tail of the arrow. The `rect`
/// field is updated each frame to clip the source tile from the right, revealing only
/// the charged fraction at full opacity.
#[derive(Component)]
pub struct AimIndicatorFill;

/// Marks the bow sprite overlay that renders on top of the player while aiming.
///
/// Spawned once at startup and positioned at the player each frame. Rotates to face
/// the aim direction so the bow visually points toward the cursor (or stick input).
/// Visible only when [`AimIndicator`] would also be visible.
#[derive(Component)]
pub struct BowOverlay;

/// Drives the aim indicator sprite that orbits the player toward the mouse cursor.
///
/// When the player has an item with `ammo_id` equipped and holds Shift, the indicator
/// becomes visible and rotates to point from the player toward the cursor position.
pub struct AimPlugin;

impl Plugin for AimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimState>()
            .add_systems(Startup, (spawn_aim_indicator, spawn_aim_indicator_fill, spawn_bow_overlay))
            .add_systems(Update, update_aim);
    }
}

/// Spawns the dim background arrow sprite as a hidden entity.
///
/// Always shows the full arrow at [`ARROW_ALPHA_BG`] opacity. Uses atlas frame 22
/// (`direction_arrow_east`) and a [`SpriteAnimation`] to drive the atlas index.
fn spawn_aim_indicator(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        AimIndicator,
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, ARROW_ALPHA_BG),
            ..Sprite::from_atlas_image(
                asset_server.load("atlas_8x8.png"),
                TextureAtlas { layout: layout_handle, index: 22 },
            )
        },
        Transform::from_xyz(0.0, 0.0, 1.0),
        Visibility::Hidden,
        SpriteAnimation::with_name("direction_arrow_east", false),
    ));
}

/// Spawns the bright fill arrow sprite as a hidden entity.
///
/// Left-anchored at the arrow tail so the clipped rect grows toward the tip.
/// Uses the same atlas as [`AimIndicator`] but without [`SpriteAnimation`]
/// (the atlas index and rect are driven directly by [`update_aim`]).
fn spawn_aim_indicator_fill(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        AimIndicatorFill,
        Sprite {
            color: Color::srgba(1.0, 1.0, 1.0, ARROW_ALPHA_FILL),
            ..Sprite::from_atlas_image(
                asset_server.load("atlas_8x8.png"),
                TextureAtlas { layout: layout_handle, index: 22 },
            )
        },
        Anchor::CENTER_LEFT,
        Transform::from_xyz(0.0, 0.0, 1.1),
        Visibility::Hidden,
    ));
}

/// Spawns the bow overlay sprite as a hidden entity.
///
/// Uses atlas frame 70 (`"bow"`) from `atlas_8x8.png`. Positioned above the player
/// (z = 0.5) so it overlays the character sprite while aiming.
fn spawn_bow_overlay(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        BowOverlay,
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas { layout: layout_handle, index: 70 },
        ),
        Transform::from_xyz(0.0, 0.0, 0.5),
        Visibility::Hidden,
        SpriteAnimation::with_name("bow", false),
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
///
/// The fill layer uses `Sprite.rect` (tile-local coordinates) to clip the arrow sprite
/// from the right, revealing only the charged fraction. Its pivot sits at the arrow tail
/// so the visible region grows from tail toward tip as `charge_elapsed` accumulates.
fn update_aim(
    time: Res<Time>,
    mut aim_state: ResMut<AimState>,
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
    mut fill_query: Query<(&mut Transform, &mut Visibility, &mut Sprite), (With<AimIndicatorFill>, Without<PlayerControlled>, Without<AimIndicator>)>,
    mut bow_query: Query<(&mut Transform, &mut Visibility), (With<BowOverlay>, Without<PlayerControlled>, Without<AimIndicator>, Without<AimIndicatorFill>)>,
) {
    let Ok((mut ind_tf, mut ind_vis)) = indicator_query.single_mut() else { return };
    let Ok((mut fill_tf, mut fill_vis, mut fill_sprite)) = fill_query.single_mut() else { return };
    let Ok((mut bow_tf, mut bow_vis)) = bow_query.single_mut() else { return };

    let aiming = *input_mode == InputMode::Playing
        && action_input.pressed(GameAction::Aim)
        && equipped.0.is_some()
        && check_has_ammo(&equipped, &item_library, &player_query);

    if !aiming {
        *ind_vis = Visibility::Hidden;
        *fill_vis = Visibility::Hidden;
        *bow_vis = Visibility::Hidden;
        aim_state.active = false;
        aim_state.charge = 0.0;
        return;
    }

    let Ok((player_tf, _, facing)) = player_query.single() else {
        *ind_vis = Visibility::Hidden;
        *fill_vis = Visibility::Hidden;
        *bow_vis = Visibility::Hidden;
        aim_state.active = false;
        aim_state.charge = 0.0;
        return;
    };
    let player_pos = player_tf.translation.truncate();

    let direction = match *input_source {
        InputSource::KeyboardMouse => {
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
                None => {
                    *ind_vis = Visibility::Hidden;
                    *fill_vis = Visibility::Hidden;
                    *bow_vis = Visibility::Hidden;
                    aim_state.active = false;
                    return;
                }
            }
        }
        InputSource::Gamepad => {
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
    let rotation = Quat::from_rotation_z(direction.to_angle());

    // Background: centered at orbit position, full arrow at dim opacity.
    ind_tf.translation = orbit_pos.extend(1.0);
    ind_tf.rotation = rotation;
    *ind_vis = Visibility::Inherited;

    // Accumulate charge while aim is held, capped at CHARGE_DURATION. The shoot
    // system resets `aim_state.charge` to 0 after firing so the next shot must
    // recharge from the floor — that reset still arrives here as `charge`.
    let elapsed = (aim_state.charge * CHARGE_DURATION + time.delta_secs()).min(CHARGE_DURATION);
    let charge = elapsed / CHARGE_DURATION;
    aim_state.active = true;
    aim_state.charge = charge;
    aim_state.direction = direction;
    aim_state.origin = player_pos;

    // Fill layer: pivot at the arrow tail (left end in sprite-local space).
    // Sprite.rect in tile-local coordinates clips the right side so only `charge`
    // fraction of the tile is sampled — growing from tail toward tip.
    let tail_pos = orbit_pos - direction * (GRID_SIZE / 2.0);
    fill_tf.translation = tail_pos.extend(1.1);
    fill_tf.rotation = rotation;
    fill_sprite.rect = Some(Rect::new(0.0, 0.0, GRID_SIZE * charge, GRID_SIZE));
    *fill_vis = Visibility::Inherited;

    // Bow overlay: on the player, rotated toward aim direction.
    bow_tf.translation = player_pos.extend(0.5);
    // The bow sprite faces south in the atlas, so offset by +90° to align east with angle 0.
    bow_tf.rotation = Quat::from_rotation_z(direction.to_angle() + std::f32::consts::FRAC_PI_2);
    *bow_vis = Visibility::Inherited;
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
