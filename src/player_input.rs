use bevy::prelude::*;

use crate::grid_mover::{GridMover, GridMoverSet};

/// Marks the entity controlled by the player.
///
/// Attach alongside [`PlayerInput`] and [`GridMover`] on the player entity. AI-controlled
/// entities use [`GridMover`] without this marker; their direction is set by an AI system.
#[derive(Component, Debug)]
pub struct PlayerControlled;

/// Captures the player's input intent for the current frame.
///
/// Populated each frame by `read_keyboard_input` and consumed by downstream systems
/// (grid movement, sprite flipping, and eventually interact/attack). Add this alongside
/// [`PlayerControlled`] on the player entity.
#[derive(Component, Debug, Default)]
pub struct PlayerInput {
    /// Requested movement direction. Cardinal only; diagonal movement is not supported.
    pub direction: Option<IVec2>,
}

/// Reads player input, routes it to [`GridMover`], and flips the sprite to match facing.
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (read_keyboard_input, apply_input_to_grid_mover, flip_sprite_with_direction)
                .chain()
                .before(GridMoverSet),
        );
    }
}

/// Reads WASD/arrow keys and writes the result into [`PlayerInput`] on the player entity.
fn read_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut PlayerInput, With<PlayerControlled>>,
) {
    for mut input in &mut query {
        input.direction = if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
            Some(IVec2::Y)
        } else if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
            Some(IVec2::NEG_Y)
        } else if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
            Some(IVec2::NEG_X)
        } else if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
            Some(IVec2::X)
        } else {
            None
        };
    }
}

/// Copies [`PlayerInput::direction`] into [`GridMover::direction`] each frame.
///
/// This bridges player intent to the movement simulator, keeping [`GridMover`] agnostic
/// about where its direction comes from (keyboard, AI, cutscene, etc.).
fn apply_input_to_grid_mover(
    mut query: Query<(&PlayerInput, &mut GridMover), With<PlayerControlled>>,
) {
    for (input, mut mover) in &mut query {
        mover.direction = input.direction;
    }
}

/// Flips the player sprite horizontally to match the horizontal movement direction.
///
/// Only updates on left/right input; vertical movement does not change facing.
fn flip_sprite_with_direction(
    mut query: Query<(&PlayerInput, &mut Sprite), With<PlayerControlled>>,
) {
    for (input, mut sprite) in &mut query {
        if let Some(flip) = horizontal_flip(input.direction) {
            sprite.flip_x = flip;
        }
    }
}

/// Returns `Some(true)` to flip left, `Some(false)` to flip right, or `None` for vertical/no input.
fn horizontal_flip(direction: Option<IVec2>) -> Option<bool> {
    let dir = direction?;
    if dir.x != 0 { Some(dir.x < 0) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_input_default_has_no_direction() {
        let input = PlayerInput::default();
        assert!(input.direction.is_none());
    }

    #[test]
    fn horizontal_flip_left() {
        assert_eq!(horizontal_flip(Some(IVec2::NEG_X)), Some(true));
    }

    #[test]
    fn horizontal_flip_right() {
        assert_eq!(horizontal_flip(Some(IVec2::X)), Some(false));
    }

    #[test]
    fn horizontal_flip_vertical_returns_none() {
        assert_eq!(horizontal_flip(Some(IVec2::Y)), None);
        assert_eq!(horizontal_flip(Some(IVec2::NEG_Y)), None);
    }

    #[test]
    fn horizontal_flip_no_input_returns_none() {
        assert_eq!(horizontal_flip(None), None);
    }
}
