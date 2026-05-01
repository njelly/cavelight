use bevy::prelude::*;

use crate::grid_mover::GridMoverSet;
use crate::player_input::{Facing, PlayerControlled};
use crate::GRID_SIZE;

/// How fast the reticle orbits around the player, in radians per second.
const ORBIT_SPEED: f32 = 12.0;
/// Duration of the fade-in from transparent to [`MAX_ALPHA`], in seconds.
const FADE_IN_SECS: f32 = 0.15;
/// Duration of the fade-out from [`MAX_ALPHA`] to transparent, in seconds.
const FADE_OUT_SECS: f32 = 0.4;
/// How long after the last Space press before the fade-out begins, in seconds.
const HOLD_SECS: f32 = 1.0;
/// Fully-faded-in alpha for the reticle sprite.
const MAX_ALPHA: f32 = 0.5;

/// A tile-sized square that highlights the tile the player is currently facing.
///
/// Spawned as a child of the player entity by [`InteractionReticlePlugin`]. Pressing
/// Space fades it in; it fades out [`HOLD_SECS`] after the last press.
///
/// When the reticle is visible and the player changes direction, it orbits around
/// the player along the shortest arc to the new facing angle. While invisible it
/// snaps instantly so it is already in position when it fades back in.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct InteractionReticle {
    /// Current orbit angle in radians. East = 0, counter-clockwise positive.
    current_angle: f32,
    /// Target angle driven by the player's [`Facing`].
    target_angle: f32,
    /// App time in seconds when Space was last pressed.
    last_interact_secs: f32,
    /// Guards against a spurious fade-out on the first frame before Space is ever pressed.
    activated: bool,
}

impl Default for InteractionReticle {
    fn default() -> Self {
        Self {
            current_angle: Facing::East.angle(),
            target_angle: Facing::East.angle(),
            last_interact_secs: 0.0,
            activated: false,
        }
    }
}

/// Spawns and drives the [`InteractionReticle`].
pub struct InteractionReticlePlugin;

impl Plugin for InteractionReticlePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<InteractionReticle>()
            .add_systems(PostStartup, spawn_reticle)
            .add_systems(Update, update_reticle.after(GridMoverSet));
    }
}

/// Attaches an [`InteractionReticle`] child entity to every [`PlayerControlled`] entity.
///
/// Runs in [`PostStartup`] so the player is guaranteed to exist.
fn spawn_reticle(
    mut commands: Commands,
    player_query: Query<Entity, With<PlayerControlled>>,
) {
    for player in &player_query {
        commands.entity(player).with_children(|parent| {
            parent.spawn((
                InteractionReticle::default(),
                // White so it picks up tint from nearby lights (e.g. the player lantern).
                Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.0), Vec2::splat(GRID_SIZE)),
                // Start at the East offset; z=-0.5 places the reticle below the player sprite.
                Transform::from_xyz(GRID_SIZE, 0.0, -0.5),
            ));
        });
    }
}

/// Handles Space input, advances the orbit animation, and updates the reticle sprite color.
///
/// Runs after [`GridMoverSet`] so [`Facing`] has already been updated by the input chain.
fn update_reticle(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    player_query: Query<&Facing, With<PlayerControlled>>,
    mut reticle_query: Query<(&mut InteractionReticle, &mut Sprite, &mut Transform)>,
) {
    let Ok(facing) = player_query.single() else {
        return;
    };

    for (mut reticle, mut sprite, mut transform) in &mut reticle_query {
        reticle.target_angle = facing.angle();

        if keys.just_pressed(KeyCode::Space) {
            reticle.last_interact_secs = time.elapsed_secs();
            reticle.activated = true;
        }

        let alpha = compute_alpha(&reticle, time.elapsed_secs());

        if alpha < f32::EPSILON {
            // Snap to the target while invisible so the reticle is already in position when it fades in.
            reticle.current_angle = reticle.target_angle;
        } else {
            let target = reticle.target_angle;
            orbit_toward(&mut reticle.current_angle, target, ORBIT_SPEED * time.delta_secs());
        }

        sprite.color = sprite.color.with_alpha(alpha);
        transform.translation = (Vec2::from_angle(reticle.current_angle) * GRID_SIZE).extend(-0.5);
    }
}

/// Returns the reticle's alpha at `now_secs` based on the time since it was last activated.
fn compute_alpha(reticle: &InteractionReticle, now_secs: f32) -> f32 {
    if !reticle.activated {
        return 0.0;
    }
    let elapsed = now_secs - reticle.last_interact_secs;
    if elapsed < FADE_IN_SECS {
        (elapsed / FADE_IN_SECS) * MAX_ALPHA
    } else if elapsed < HOLD_SECS {
        MAX_ALPHA
    } else if elapsed < HOLD_SECS + FADE_OUT_SECS {
        ((HOLD_SECS + FADE_OUT_SECS - elapsed) / FADE_OUT_SECS) * MAX_ALPHA
    } else {
        0.0
    }
}

/// Advances `current` toward `target` by at most `max_step` radians via the shortest arc.
fn orbit_toward(current: &mut f32, target: f32, max_step: f32) {
    let diff = angle_diff(*current, target);
    if diff.abs() <= max_step {
        *current = target;
    } else {
        *current += diff.signum() * max_step;
    }
}

/// Returns the signed shortest angular difference from `from` to `to`, clamped to `[-π, π]`.
fn angle_diff(from: f32, to: f32) -> f32 {
    let raw = (to - from).rem_euclid(std::f32::consts::TAU);
    if raw > std::f32::consts::PI {
        raw - std::f32::consts::TAU
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    #[test]
    fn angle_diff_shortest_arc() {
        assert!((angle_diff(0.0, FRAC_PI_2) - FRAC_PI_2).abs() < 1e-5);
        assert!((angle_diff(0.0, -FRAC_PI_2) + FRAC_PI_2).abs() < 1e-5);
        // Full turn collapses to zero.
        assert!(angle_diff(0.0, TAU).abs() < 1e-5);
        // Half turn: shortest diff is exactly ±π.
        assert!((angle_diff(0.0, PI).abs() - PI).abs() < 1e-5);
    }

    #[test]
    fn orbit_toward_snaps_when_within_step() {
        let mut angle = 0.0_f32;
        orbit_toward(&mut angle, FRAC_PI_2, 2.0);
        assert_eq!(angle, FRAC_PI_2);
    }

    #[test]
    fn orbit_toward_advances_by_max_step() {
        let mut angle = 0.0_f32;
        orbit_toward(&mut angle, PI, 0.1);
        assert!((angle - 0.1).abs() < 1e-5);
    }

    #[test]
    fn orbit_toward_takes_shortest_arc_going_negative() {
        // From East (0) toward South (-π/2): shortest path is clockwise (negative direction).
        let mut angle = 0.0_f32;
        orbit_toward(&mut angle, -FRAC_PI_2, 0.1);
        assert!(angle < 0.0, "should step clockwise (negative): got {angle}");
    }

    #[test]
    fn compute_alpha_before_activation() {
        let reticle = InteractionReticle::default();
        assert_eq!(compute_alpha(&reticle, 0.0), 0.0);
        assert_eq!(compute_alpha(&reticle, 100.0), 0.0);
    }

    #[test]
    fn compute_alpha_full_lifecycle() {
        let reticle = InteractionReticle {
            activated: true,
            last_interact_secs: 0.0,
            ..InteractionReticle::default()
        };
        // Mid fade-in.
        let mid_in = compute_alpha(&reticle, FADE_IN_SECS / 2.0);
        assert!((mid_in - MAX_ALPHA / 2.0).abs() < 1e-4, "mid fade-in: {mid_in}");
        // Fully faded in.
        assert_eq!(compute_alpha(&reticle, FADE_IN_SECS), MAX_ALPHA);
        // Still at max during hold period.
        assert_eq!(compute_alpha(&reticle, HOLD_SECS - 0.01), MAX_ALPHA);
        // Fully faded out.
        assert_eq!(compute_alpha(&reticle, HOLD_SECS + FADE_OUT_SECS), 0.0);
    }
}
