use std::f32::consts::{FRAC_PI_2, PI};

use avian2d::prelude::*;
use bevy::prelude::*;

use crate::aim::AimState;
use crate::damageable::Damageable;
use crate::input::{ActionInput, GameAction};
use crate::inventory::{EquippedHotbarSlot, InputMode, HOTBAR_START};
use crate::item::{Inventory, ItemLibrary};
use crate::level::LevelTiles;
use crate::player_input::PlayerControlled;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

/// Total seconds an arrow stays in flight before landing (regardless of charge).
///
/// Charge controls speed (and therefore range) within this fixed time window,
/// so a fully-charged shot is fast and long-range while a tap shot is slow and short.
const ARROW_FLIGHT_SECONDS: f32 = 0.5;

/// Arrow flight speed at zero charge, in pixels per second.
const MIN_ARROW_SPEED: f32 = 50.0;

/// Arrow flight speed at full charge, in pixels per second.
const MAX_ARROW_SPEED: f32 = 160.0;

/// Damage dealt by an arrow at zero charge.
const MIN_ARROW_DAMAGE: u32 = 3;

/// Damage dealt by an arrow at full charge.
const MAX_ARROW_DAMAGE: u32 = 9;

/// Maximum distance from the player center (squared) to auto-pick-up a landed arrow.
///
/// Squared so we can compare against `Vec2::distance_squared` without a sqrt.
const PICKUP_RANGE_SQ: f32 = (GRID_SIZE * 0.7) * (GRID_SIZE * 0.7);

/// Atlas frame index for the arrow sprite (matches `"arrow"` in `sprite_animations.ron`).
const ARROW_ATLAS_INDEX: usize = 74;

/// Forward offset from the player center where a freshly-fired arrow spawns.
///
/// Half a tile keeps the arrow visually outside the player sprite while not pushing it
/// far enough to skip the very first frame of in-flight collision detection.
const ARROW_SPAWN_OFFSET: f32 = GRID_SIZE * 0.5;

/// Peak arc height (pixels) at zero charge — barely lifts off the ground.
const ARC_PEAK_MIN: f32 = 1.5;

/// Peak arc height (pixels) at full charge — about half a tile above the ground.
const ARC_PEAK_MAX: f32 = 5.0;

/// Maximum visual scale at the arc apex. Combined with the y-offset and ground shadow,
/// this sells the "in the air" illusion in a top-down 2D view.
const VISUAL_PEAK_SCALE: f32 = 1.2;

/// Visible width / height of the ground shadow that travels under the arrow.
const SHADOW_SIZE: Vec2 = Vec2::new(4.0, 2.0);

/// Marks an arrow projectile / pickup entity (the parent / logical-position entity).
///
/// Arrows transition from [`ArrowState::Flying`] to [`ArrowState::Landed`] when they
/// hit a wall, hit another solid entity, or run out of flight time. Landed arrows can
/// be picked up by the player simply by walking onto them.
///
/// The root entity sits at the *logical* (ground) position used for collision and
/// pickup. Two children render the visual: an [`ArrowVisual`] arrow sprite that
/// arcs up and back down, and an [`ArrowShadow`] tile that stays on the ground.
#[derive(Component, Debug)]
pub struct Arrow {
    /// Current state — drives whether the motion or pickup system handles this entity.
    pub state: ArrowState,
}

/// Lifecycle state of an [`Arrow`] entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArrowState {
    /// In flight — the motion system advances the arrow each frame.
    Flying {
        /// Velocity vector in pixels per second (direction × charged speed).
        velocity: Vec2,
        /// Distance left to travel before the arrow lands due to flight time elapsing.
        distance_remaining: f32,
        /// Total ground distance the arrow will travel — used to compute progress
        /// (and therefore arc height / scale) without retaining a separate timer.
        total_distance: f32,
        /// Arc apex height (pixels) for this shot — scales with charge.
        arc_height: f32,
        /// Damage dealt to the first [`Damageable`] this arrow hits, baked in at fire
        /// time so the value reflects the charge level when the shot was loosed
        /// (rather than the aim state at the moment of impact).
        damage: u32,
        /// Entity that fired the arrow. Excluded from collision so the arrow does
        /// not immediately hit its own shooter at the spawn position.
        shooter: Entity,
    },
    /// Landed on the ground (or stuck in a wall) — the pickup system handles this state.
    Landed,
}

/// Marks the child entity that renders the arrow sprite, lifted off the ground.
///
/// Translation `y` and uniform `scale` are animated each frame from the parent's
/// flight progress to fake an in-air arc.
#[derive(Component, Debug)]
pub struct ArrowVisual;

/// Marks the child entity that renders the arrow's ground shadow.
///
/// Stays at the parent's logical (ground) position — never lifted. Hidden once
/// the arrow lands, since the arrow is then sitting on the ground itself.
#[derive(Component, Debug)]
pub struct ArrowShadow;

/// Bow shooting + arrow projectile + landed-arrow pickup.
///
/// Listens for [`GameAction::Shoot`] while [`AimState::active`] is true; consumes one
/// piece of the equipped weapon's ammo and spawns an [`Arrow`]. The arrow flies in a
/// straight line until it hits a wall, another solid entity, or its time expires —
/// then transitions to [`ArrowState::Landed`] and can be walked over to pick back up.
pub struct ArrowPlugin;

impl Plugin for ArrowPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (fire_arrow, fly_arrows, pick_up_landed_arrows).chain());
    }
}

/// Fires an arrow when [`GameAction::Shoot`] is just-pressed and the player is aiming.
///
/// Resolves the equipped weapon's ammo id, consumes one of that ammo from the player
/// inventory, then spawns an [`Arrow`] entity at the player position pointing in
/// [`AimState::direction`]. Speed / range / arc height all scale linearly with
/// [`AimState::charge`]. The aim charge is reset to zero so the next shot must
/// recharge from the floor.
fn fire_arrow(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    mut aim_state: ResMut<AimState>,
    equipped: Res<EquippedHotbarSlot>,
    item_library: Option<Res<ItemLibrary>>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut player_query: Query<(Entity, &Transform, &mut Inventory), With<PlayerControlled>>,
    mut commands: Commands,
) {
    if *input_mode != InputMode::Playing { return; }
    if !action_input.just_pressed(GameAction::Shoot) { return; }
    if !aim_state.active { return; }

    let Some(library) = item_library else { return };
    let Ok((shooter, player_tf, mut inventory)) = player_query.single_mut() else { return };

    // Resolve the ammo id of the equipped item — only ranged weapons have one.
    let Some(ammo_id) = resolve_ammo_id(&equipped, &library, &inventory) else { return };
    if !inventory.take_one_by_id(&ammo_id) { return };

    let charge = aim_state.charge.clamp(0.0, 1.0);
    let speed = MIN_ARROW_SPEED + (MAX_ARROW_SPEED - MIN_ARROW_SPEED) * charge;
    let direction = aim_state.direction;
    let velocity = direction * speed;
    let total_distance = speed * ARROW_FLIGHT_SECONDS;
    let arc_height = ARC_PEAK_MIN + (ARC_PEAK_MAX - ARC_PEAK_MIN) * charge;
    // Damage scales linearly with charge so timing the release rewards a stronger hit.
    let damage = MIN_ARROW_DAMAGE
        + ((MAX_ARROW_DAMAGE - MIN_ARROW_DAMAGE) as f32 * charge).round() as u32;

    let player_pos = player_tf.translation.truncate();
    let spawn_pos = player_pos + direction * ARROW_SPAWN_OFFSET;

    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    // The arrow sprite (atlas frame 74) is drawn pointing south, so add 90° to align
    // its forward direction with the velocity angle (matches the bow overlay convention).
    let rotation = Quat::from_rotation_z(direction.to_angle() + FRAC_PI_2);

    commands
        .spawn((
            Arrow {
                state: ArrowState::Flying {
                    velocity,
                    distance_remaining: total_distance,
                    total_distance,
                    arc_height,
                    damage,
                    shooter,
                },
            },
            // Root sits at the logical ground position with no own sprite — children render.
            Transform::from_translation(spawn_pos.extend(0.0)),
            Visibility::default(),
        ))
        .with_children(|root| {
            // Ground shadow: dark transparent rectangle that travels under the arrow.
            root.spawn((
                ArrowShadow,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.4), SHADOW_SIZE),
                // Slightly negative z keeps the shadow under the player sprite (z=0).
                Transform::from_xyz(0.0, 0.0, -0.05),
            ));
            // Visual arrow: lifted by `translation.y` and uniform-scaled at the arc apex.
            // Rotation matches the velocity angle and is fixed for the entire flight.
            root.spawn((
                ArrowVisual,
                Sprite::from_atlas_image(
                    asset_server.load("atlas_8x8.png"),
                    TextureAtlas { layout: layout_handle, index: ARROW_ATLAS_INDEX },
                ),
                Transform {
                    translation: Vec3::new(0.0, 0.0, 0.5),
                    rotation,
                    ..default()
                },
                SpriteAnimation::with_name("arrow", false),
            ));
        });

    aim_state.reset_charge();
}

/// Spawns a landed [`Arrow`] entity at `position` with the given facing rotation (radians, z-axis).
///
/// Used by the save-load system to restore arrows that were lying on the ground when
/// the player saved. The arrow is created in [`ArrowState::Landed`] so it can be
/// picked up immediately by walking onto it. The shadow child is hidden — landed
/// arrows sit flush with the floor.
pub fn spawn_landed_arrow_entity(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    position: Vec2,
    rotation_z: f32,
) -> Entity {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands
        .spawn((
            Arrow { state: ArrowState::Landed },
            Transform::from_translation(position.extend(0.0)),
            Visibility::default(),
        ))
        .with_children(|root| {
            root.spawn((
                ArrowShadow,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.4), SHADOW_SIZE),
                Transform::from_xyz(0.0, 0.0, -0.05),
                Visibility::Hidden,
            ));
            root.spawn((
                ArrowVisual,
                Sprite::from_atlas_image(
                    asset_server.load("atlas_8x8.png"),
                    TextureAtlas { layout: layout_handle, index: ARROW_ATLAS_INDEX },
                ),
                Transform {
                    translation: Vec3::new(0.0, 0.0, 0.5),
                    rotation: Quat::from_rotation_z(rotation_z),
                    ..default()
                },
                SpriteAnimation::with_name("arrow", false),
            ));
        })
        .id()
}

/// Returns the ammo item id for the equipped hotbar item, or `None` if there is no
/// equipped slot, no item, or the item does not consume ammo.
fn resolve_ammo_id(
    equipped: &EquippedHotbarSlot,
    library: &ItemLibrary,
    inventory: &Inventory,
) -> Option<String> {
    let hotbar_idx = equipped.0?;
    let stack = inventory.get(HOTBAR_START + hotbar_idx)?;
    library.def(&stack.id)?.ammo_id.clone()
}

/// Advances every flying [`Arrow`] each frame, handling wall and entity collisions
/// and animating the visual child to fake an arc.
///
/// Movement is checked against two stop conditions in order:
/// 1. [`LevelTiles::is_walkable`] at the new position — wall tiles stop the arrow.
/// 2. [`SpatialQuery::point_intersections`] — any non-sensor entity stops the arrow,
///    excluding the original shooter (so an arrow does not immediately hit its firer).
///
/// When stopping, the arrow snaps back to its previous position (so it does not land
/// inside a wall), transitions to [`ArrowState::Landed`], drops the visual to ground
/// level, and hides its shadow.
///
/// In flight, the visual child's local `translation.y` follows a sine arc
/// (`arc_height * sin(progress * π)`) so the arrow lifts off and lands smoothly,
/// while uniform scale lerps up to [`VISUAL_PEAK_SCALE`] at the arc apex.
fn fly_arrows(
    time: Res<Time>,
    level: Option<Res<LevelTiles>>,
    spatial_query: SpatialQuery,
    sensor_query: Query<(), With<Sensor>>,
    mut arrows: Query<(&mut Arrow, &mut Transform, &Children), Without<ArrowVisual>>,
    mut visual_query: Query<&mut Transform, With<ArrowVisual>>,
    mut shadow_query: Query<&mut Visibility, With<ArrowShadow>>,
    mut damageables: Query<&mut Damageable>,
) {
    let Some(level) = level else { return };
    let dt = time.delta_secs();

    for (mut arrow, mut transform, children) in &mut arrows {
        // --- Update logical position + state for flying arrows ---
        if let ArrowState::Flying {
            velocity,
            mut distance_remaining,
            total_distance,
            arc_height,
            damage,
            shooter,
        } = arrow.state
        {
            let prev_pos = transform.translation.truncate();
            let step = velocity * dt;
            let new_pos = prev_pos + step;
            let step_len = step.length();

            // Wall hit: any tile that is not walkable stops the arrow.
            let blocked_by_wall = match level.world_to_tile(new_pos) {
                Some((tx, ty)) => !level.is_walkable(tx, ty),
                None => true,
            };

            // Solid entity hit: the first non-sensor collider that isn't the shooter.
            // We capture the entity (rather than just a `bool`) so we can apply damage
            // to it if it is a [`Damageable`].
            let filter = SpatialQueryFilter::default().with_excluded_entities([shooter]);
            let hit_entity = spatial_query
                .point_intersections(new_pos, &filter)
                .iter()
                .copied()
                .find(|&e| !sensor_query.contains(e));

            if blocked_by_wall || hit_entity.is_some() {
                // Apply damage to a Damageable target before landing. The arrow itself
                // still lands at `prev_pos` (matching the existing behavior the player
                // expects — arrows can be picked back up after a hit).
                if let Some(target) = hit_entity {
                    if let Ok(mut d) = damageables.get_mut(target) {
                        d.take(damage);
                    }
                }
                // Land at the previous position so the arrow does not visually overlap the wall.
                transform.translation = prev_pos.extend(transform.translation.z);
                arrow.state = ArrowState::Landed;
            } else {
                transform.translation = new_pos.extend(transform.translation.z);
                distance_remaining -= step_len;
                if distance_remaining <= 0.0 {
                    arrow.state = ArrowState::Landed;
                } else {
                    arrow.state = ArrowState::Flying {
                        velocity,
                        distance_remaining,
                        total_distance,
                        arc_height,
                        damage,
                        shooter,
                    };
                }
            }
        }

        // --- Drive the visual children from the (possibly just-updated) state ---
        let (visual_y, visual_scale, shadow_visible) = match arrow.state {
            ArrowState::Flying { distance_remaining, total_distance, arc_height, .. } => {
                // progress goes 0 (just fired) → 1 (about to land); sin gives a smooth arc.
                let progress = (1.0 - distance_remaining / total_distance.max(f32::EPSILON))
                    .clamp(0.0, 1.0);
                let arc = (progress * PI).sin();
                let y = arc_height * arc;
                let scale = 1.0 + (VISUAL_PEAK_SCALE - 1.0) * arc;
                (y, scale, true)
            }
            // On the ground: visual sits flush, shadow goes away.
            ArrowState::Landed => (0.0, 1.0, false),
        };

        for &child in children {
            if let Ok(mut visual_tf) = visual_query.get_mut(child) {
                visual_tf.translation.y = visual_y;
                // Preserve the rotation set at spawn while updating scale uniformly.
                visual_tf.scale = Vec3::splat(visual_scale);
            }
            if let Ok(mut shadow_vis) = shadow_query.get_mut(child) {
                *shadow_vis = if shadow_visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

/// Picks up landed arrows when the player walks within [`PICKUP_RANGE_SQ`] of them.
///
/// One arrow goes back into the player's inventory (merging with an existing arrow
/// stack first, then any empty slot). If the inventory cannot hold any more, the
/// arrow stays on the ground for later.
fn pick_up_landed_arrows(
    mut commands: Commands,
    item_library: Option<Res<ItemLibrary>>,
    mut player_query: Query<(&Transform, &mut Inventory), With<PlayerControlled>>,
    arrows: Query<(Entity, &Arrow, &Transform), Without<PlayerControlled>>,
) {
    let Some(library) = item_library else { return };
    let Some(arrow_def) = library.def("arrow") else { return };
    let max_stack = arrow_def.max_stack;

    let Ok((player_tf, mut inventory)) = player_query.single_mut() else { return };
    let player_pos = player_tf.translation.truncate();

    for (entity, arrow, tf) in &arrows {
        if !matches!(arrow.state, ArrowState::Landed) {
            continue;
        }
        if tf.translation.truncate().distance_squared(player_pos) > PICKUP_RANGE_SQ {
            continue;
        }
        let leftover = inventory.add_items("arrow", 1, max_stack);
        if leftover == 0 {
            commands.entity(entity).despawn();
        }
    }
}
