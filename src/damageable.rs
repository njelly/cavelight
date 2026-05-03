//! Damageable entities with floating health-bar + name-label UI.
//!
//! Any entity carrying a [`Damageable`] component can take damage. The first hit reveals
//! a world-space health bar (sprite children of the damageable) and a screen-space name
//! label (a UI [`Text`] node anchored to the entity's projected screen position). When
//! accumulated damage exceeds [`Damageable::toughness`] the entity is despawned and a
//! short [`PulseFx`] effect plays at its position.
//!
//! The name label lives in the UI tree rather than in world space so it renders at
//! native screen-pixel resolution — same crispness as dialogue and inventory text.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::ui::ComputedNode;

use crate::spawner::PulseFx;
use crate::sprite_animation::SpriteAnimation;

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Vertical offset (world pixels) of the health-bar root above the entity's sprite center.
const BAR_Y_OFFSET: f32 = 7.0;

/// Vertical offset (world pixels) of the name-label anchor point above the entity's sprite
/// center. The UI label is positioned so its bottom edge sits at the projected screen
/// position of this anchor, stacking it above the health bar — so this should be
/// roughly the bar's top edge (`BAR_Y_OFFSET + BAR_OUTER.y / 2`) plus a small visual gap.
const LABEL_ANCHOR_Y: f32 = 10.0;

/// Outer dimensions of the bar including the gold/brown border.
const BAR_OUTER: Vec2 = Vec2::new(14.0, 4.0);

/// Inner dimensions of the bar (the colored fill area).
const BAR_INNER: Vec2 = Vec2::new(12.0, 2.0);

/// Border color, matching the inventory/menu slot border so health bars feel native.
const BAR_BORDER_COLOR: Color = Color::srgb(0.32, 0.27, 0.22);

/// Background of the empty bar — a darker shade behind the fill.
const BAR_BG_COLOR: Color = Color::srgba(0.20, 0.05, 0.05, 0.95);

/// Foreground (remaining HP) color.
const BAR_FILL_COLOR: Color = Color::srgb(0.78, 0.18, 0.18);

/// Text color for the entity name, matching dialogue/menu text.
const NAME_TEXT_COLOR: Color = Color::srgb(0.85, 0.80, 0.70);

/// Screen-pixel font size for the UI name label. Picked to roughly match the
/// inventory/dialogue text density without dominating the small enemy sprite.
const NAME_FONT_SIZE: f32 = 11.0;

/// Stack order for the name label in the UI tree. Lower than the dialogue panel
/// (`GlobalZIndex(8)`) so a dialogue can overlap and obscure floating labels.
const NAME_LABEL_Z: i32 = 5;

/// Atlas frame index for the death pulse effect (same frame the spawn-in pulse uses).
const DEATH_PULSE_ATLAS_INDEX: usize = 25;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks an entity that can accept damage and be killed.
///
/// `toughness` is fixed at construction (base health). `damage` accumulates from hits
/// until [`Damageable::is_dead`] returns true, after which [`despawn_dead`] removes
/// the entity and plays a [`PulseFx`] at its last position.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Damageable {
    /// Maximum damage the entity can absorb before dying. Constant once set.
    pub toughness: u32,
    /// Accumulated damage. Starts at 0; the entity dies once this exceeds `toughness`.
    pub damage: u32,
    /// Player-facing name shown on the floating label once damage has been taken.
    pub display_name: String,
}

impl Damageable {
    /// Builds a new `Damageable` at full health with the given name.
    pub fn new(toughness: u32, display_name: impl Into<String>) -> Self {
        Self { toughness, damage: 0, display_name: display_name.into() }
    }

    /// Adds `amount` to accumulated damage, saturating at [`u32::MAX`].
    pub fn take(&mut self, amount: u32) {
        self.damage = self.damage.saturating_add(amount);
    }

    /// Returns true once accumulated damage exceeds `toughness`.
    pub fn is_dead(&self) -> bool {
        self.damage > self.toughness
    }

    /// Returns the fraction of HP remaining in `[0.0, 1.0]`.
    pub fn fraction_remaining(&self) -> f32 {
        if self.toughness == 0 {
            return 0.0;
        }
        let remaining = (self.toughness as i64 - self.damage as i64).max(0) as f32;
        (remaining / self.toughness as f32).clamp(0.0, 1.0)
    }
}

/// Marks the parent entity of the health-bar visual (border + bg + fill grand-children).
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct HealthBar;

/// Marks the inner fill sprite whose `custom_size.x` is scaled to remaining HP.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct HealthBarFill;

/// Top-level UI entity that displays a damageable's display name above its sprite.
///
/// Lives in the UI tree (not the world hierarchy) so it renders at native screen-pixel
/// resolution. `target` points back at the damageable so [`update_name_labels`] can
/// re-project its world position each frame and [`cleanup_orphan_name_labels`] can
/// despawn the label when the damageable is gone.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct NameLabel {
    pub target: Entity,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers the damageable types and the systems that drive health-bar visuals,
/// name-label projection, and on-death cleanup.
pub struct DamageablePlugin;

impl Plugin for DamageablePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Damageable>()
            .register_type::<HealthBar>()
            .register_type::<HealthBarFill>()
            .register_type::<NameLabel>()
            .add_systems(
                Update,
                (
                    attach_health_bars,
                    update_health_bars,
                    update_name_labels,
                    cleanup_orphan_name_labels,
                    despawn_dead,
                )
                    .chain(),
            );
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Spawns the floating health-bar children and the screen-space name-label UI node
/// for any newly-added [`Damageable`].
///
/// The bar is built as a hierarchy of world-space sprites parented to the damageable
/// (so it follows the entity's transform automatically). The name label is a top-level
/// UI node tracked by [`NameLabel::target`]; positioning happens each frame in
/// [`update_name_labels`].
///
/// Both visuals start hidden — they only appear once damage is taken.
fn attach_health_bars(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    new_damageables: Query<(Entity, &Damageable), Added<Damageable>>,
) {
    if new_damageables.is_empty() {
        return;
    }
    let font: Handle<Font> = asset_server.load("fonts/RobotoMono-Bold.ttf");

    for (entity, damageable) in &new_damageables {
        // World-space health bar parented to the damageable.
        commands.entity(entity).with_children(|parent| {
            parent
                .spawn((
                    HealthBar,
                    Transform::from_xyz(0.0, BAR_Y_OFFSET, 0.5),
                    Visibility::Hidden,
                ))
                .with_children(|bar| {
                    // Border — drawn lowest in local z so the inner sprites overlay it.
                    bar.spawn((
                        Sprite::from_color(BAR_BORDER_COLOR, BAR_OUTER),
                        Transform::from_xyz(0.0, 0.0, 0.0),
                    ));
                    // Inner background — shows through where the fill has drained.
                    bar.spawn((
                        Sprite::from_color(BAR_BG_COLOR, BAR_INNER),
                        Transform::from_xyz(0.0, 0.0, 0.01),
                    ));
                    // Fill sprite, anchored at left edge so shrinking `custom_size.x`
                    // visually drains the bar from right to left while keeping the
                    // left edge pinned to the bar's left edge.
                    bar.spawn((
                        HealthBarFill,
                        Sprite::from_color(BAR_FILL_COLOR, BAR_INNER),
                        Anchor::CENTER_LEFT,
                        Transform::from_xyz(-BAR_INNER.x / 2.0, 0.0, 0.02),
                    ));
                });
        });

        // Screen-space UI name label, positioned each frame from the entity transform.
        commands.spawn((
            NameLabel { target: entity },
            Text::new(damageable.display_name.clone()),
            TextFont {
                font: font.clone(),
                font_size: NAME_FONT_SIZE,
                ..default()
            },
            TextColor(NAME_TEXT_COLOR),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(NAME_LABEL_Z),
            Visibility::Hidden,
        ));
    }
}

/// Drives the health-bar fill width and shows/hides the bar based on the parent's
/// [`Damageable`] state. Visibility flips from `Hidden` to `Inherited` once the entity
/// has taken any damage.
fn update_health_bars(
    damageables: Query<(&Damageable, &Children), Changed<Damageable>>,
    mut bar_query: Query<(&mut Visibility, &Children), With<HealthBar>>,
    mut fill_query: Query<&mut Sprite, With<HealthBarFill>>,
) {
    for (damageable, children) in &damageables {
        let visible = damageable.damage > 0;
        let fraction = damageable.fraction_remaining();
        let fill_width = BAR_INNER.x * fraction;

        for &child in children {
            if let Ok((mut vis, bar_children)) = bar_query.get_mut(child) {
                *vis = if visible { Visibility::Inherited } else { Visibility::Hidden };
                for &grand in bar_children {
                    if let Ok(mut sprite) = fill_query.get_mut(grand) {
                        if let Some(size) = sprite.custom_size.as_mut() {
                            size.x = fill_width;
                        }
                    }
                }
            }
        }
    }
}

/// Re-projects each [`NameLabel`]'s world anchor to viewport coordinates and updates
/// its UI [`Node`] absolute position so the label tracks the damageable on screen.
///
/// Centers horizontally using the previously-computed [`ComputedNode::size`] (one frame
/// of layout lag is invisible at 60fps). Hides the label when the target sits outside
/// the camera frustum or has not yet taken damage.
fn update_name_labels(
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    targets: Query<(&GlobalTransform, &Damageable)>,
    mut labels: Query<(&NameLabel, &mut Node, &ComputedNode, &mut Visibility)>,
) {
    let Ok((cam, cam_tf)) = camera.single() else {
        return;
    };

    for (label, mut node, computed, mut vis) in &mut labels {
        let Ok((target_tf, damageable)) = targets.get(label.target) else {
            // Target despawn is handled separately; stay hidden until it lands.
            *vis = Visibility::Hidden;
            continue;
        };

        if damageable.damage == 0 {
            *vis = Visibility::Hidden;
            continue;
        }

        let world_anchor =
            target_tf.translation() + Vec3::new(0.0, LABEL_ANCHOR_Y, 0.0);
        let Ok(viewport) = cam.world_to_viewport(cam_tf, world_anchor) else {
            *vis = Visibility::Hidden;
            continue;
        };

        // ComputedNode reports physical pixels; UI position values are logical pixels.
        let logical_size = computed.size() * computed.inverse_scale_factor;
        node.left = Val::Px(viewport.x - logical_size.x / 2.0);
        node.top = Val::Px(viewport.y - logical_size.y);

        *vis = Visibility::Inherited;
    }
}

/// Despawns name labels whose target [`Damageable`] entity is gone.
///
/// We can't rely on parent-child despawn cascade because the label lives in the UI
/// tree as a top-level entity (so it can render at native screen-pixel resolution).
/// Instead we listen for [`Damageable`] component removals and despawn any label
/// pointing at one of the removed targets.
fn cleanup_orphan_name_labels(
    mut commands: Commands,
    mut removed: RemovedComponents<Damageable>,
    labels: Query<(Entity, &NameLabel)>,
) {
    let removed_targets: HashSet<Entity> = removed.read().collect();
    if removed_targets.is_empty() {
        return;
    }
    for (label_entity, label) in &labels {
        if removed_targets.contains(&label.target) {
            commands.entity(label_entity).despawn();
        }
    }
}

/// Despawns any [`Damageable`] whose damage exceeds toughness and spawns a death
/// [`PulseFx`] at its last position. The pulse is a standalone entity so it stays put
/// after the dying entity is gone.
fn despawn_dead(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    dead: Query<(Entity, &Damageable, &Transform)>,
) {
    let mut layout_handle: Option<Handle<TextureAtlasLayout>> = None;

    for (entity, damageable, transform) in &dead {
        if !damageable.is_dead() {
            continue;
        }

        let handle = layout_handle.get_or_insert_with(|| {
            let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
            layouts.add(layout)
        });

        commands.spawn((
            PulseFx,
            Sprite::from_atlas_image(
                asset_server.load("atlas_8x8.png"),
                TextureAtlas {
                    layout: handle.clone(),
                    index: DEATH_PULSE_ATLAS_INDEX,
                },
            ),
            Transform::from_xyz(transform.translation.x, transform.translation.y, -0.1),
            SpriteAnimation::with_name("spawner_pulse", false),
        ));

        commands.entity(entity).despawn();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_full_hp() {
        let d = Damageable::new(20, "Skeleton");
        assert_eq!(d.toughness, 20);
        assert_eq!(d.damage, 0);
        assert!(!d.is_dead());
        assert_eq!(d.fraction_remaining(), 1.0);
    }

    #[test]
    fn take_accumulates_damage() {
        let mut d = Damageable::new(20, "Skeleton");
        d.take(5);
        d.take(3);
        assert_eq!(d.damage, 8);
        assert!(!d.is_dead());
    }

    #[test]
    fn dies_only_when_damage_exceeds_toughness() {
        let mut d = Damageable::new(10, "Test");
        d.take(10);
        // damage == toughness is still alive per the design (`damage > toughness`).
        assert!(!d.is_dead());
        d.take(1);
        assert!(d.is_dead());
    }

    #[test]
    fn fraction_remaining_drops_linearly() {
        let mut d = Damageable::new(10, "Test");
        d.take(5);
        assert!((d.fraction_remaining() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn fraction_remaining_clamps_to_zero_when_overkilled() {
        let mut d = Damageable::new(10, "Test");
        d.take(50);
        assert_eq!(d.fraction_remaining(), 0.0);
    }

    #[test]
    fn take_saturates_instead_of_overflowing() {
        let mut d = Damageable::new(10, "Test");
        d.damage = u32::MAX - 1;
        d.take(10);
        assert_eq!(d.damage, u32::MAX);
    }
}
