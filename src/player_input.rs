use bevy::prelude::*;

use crate::grid_mover::{GridMover, GridMoverSet};
use crate::input::{ActionInput, GameAction};
use crate::inventory::InputMode;

/// Which cardinal direction the player entity is currently facing.
///
/// Updated each frame from [`PlayerInput::direction`] whenever the player moves.
/// Drives sprite flipping (East = normal, West = flipped) and determines which tile
/// the [`crate::interaction_reticle::InteractionReticle`] highlights.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Component, Default)]
pub enum Facing {
    /// Facing right — the default, non-flipped sprite orientation.
    #[default]
    East,
    /// Facing left — sprite is flipped horizontally.
    West,
    /// Facing up — sprite orientation unchanged from East/West.
    North,
    /// Facing down — sprite orientation unchanged from East/West.
    South,
}

impl Facing {
    /// Converts a cardinal [`IVec2`] direction to a [`Facing`].
    ///
    /// Returns `None` for zero or diagonal vectors.
    pub fn from_direction(dir: IVec2) -> Option<Self> {
        match (dir.x, dir.y) {
            (1, 0) => Some(Facing::East),
            (-1, 0) => Some(Facing::West),
            (0, 1) => Some(Facing::North),
            (0, -1) => Some(Facing::South),
            _ => None,
        }
    }

    /// Returns the world-space unit offset vector for this direction.
    pub fn offset(self) -> Vec2 {
        match self {
            Facing::East => Vec2::X,
            Facing::West => Vec2::NEG_X,
            Facing::North => Vec2::Y,
            Facing::South => Vec2::NEG_Y,
        }
    }

    /// Returns the angle in radians for this direction (East = 0, counter-clockwise positive).
    ///
    /// Matches [`Vec2::from_angle`]: `cos` maps to x and `sin` maps to y.
    pub fn angle(self) -> f32 {
        match self {
            Facing::East => 0.0,
            Facing::North => std::f32::consts::FRAC_PI_2,
            Facing::West => std::f32::consts::PI,
            Facing::South => -std::f32::consts::FRAC_PI_2,
        }
    }
}

/// Marks the entity controlled by the player.
///
/// Attach alongside [`PlayerInput`], [`Facing`], and [`GridMover`] on the player entity.
/// AI-controlled entities use [`GridMover`] without this marker.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct PlayerControlled;

/// Captures the player's input intent for the current frame.
///
/// Populated each frame by [`read_keyboard_input`] and consumed by downstream systems.
/// Add this alongside [`PlayerControlled`] on the player entity.
#[derive(Component, Debug, Default, Reflect)]
#[reflect(Component, Default)]
pub struct PlayerInput {
    /// Requested movement direction. Cardinal only; diagonal movement is not supported.
    pub direction: Option<IVec2>,
}

/// Reads player input, updates [`Facing`], flips the sprite, and forwards direction to [`GridMover`].
pub struct PlayerInputPlugin;

impl Plugin for PlayerInputPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Facing>()
            .register_type::<PlayerControlled>()
            .register_type::<PlayerInput>()
            .add_systems(
                Update,
                (read_player_input, update_facing, apply_input_to_grid_mover)
                    .chain()
                    .before(GridMoverSet),
            );
    }
}

/// Reads movement actions and writes the result into [`PlayerInput`] on the player entity.
///
/// Accepts keyboard (WASD/arrows) and gamepad (left stick/D-pad) via [`ActionInput`].
/// Clears direction when [`InputMode`] is not [`InputMode::Playing`] so the player stops
/// moving the moment any menu or dialogue screen opens.
fn read_player_input(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    mut query: Query<&mut PlayerInput, With<PlayerControlled>>,
) {
    if *input_mode != InputMode::Playing {
        for mut input in &mut query {
            input.direction = None;
        }
        return;
    }
    for mut input in &mut query {
        // Priority: North > South > West > East (matches prior keyboard-only behavior).
        input.direction = if action_input.pressed(GameAction::MoveNorth) {
            Some(IVec2::Y)
        } else if action_input.pressed(GameAction::MoveSouth) {
            Some(IVec2::NEG_Y)
        } else if action_input.pressed(GameAction::MoveWest) {
            Some(IVec2::NEG_X)
        } else if action_input.pressed(GameAction::MoveEast) {
            Some(IVec2::X)
        } else {
            None
        };
    }
}

/// Updates [`Facing`] from [`PlayerInput::direction`] and flips the sprite to match.
///
/// [`Facing`] is the source of truth for sprite orientation: only East/West affect the flip.
/// North and South leave the horizontal orientation unchanged.
fn update_facing(
    mut query: Query<(&PlayerInput, &mut Facing, &mut Sprite), With<PlayerControlled>>,
) {
    for (input, mut facing, mut sprite) in &mut query {
        if let Some(dir) = input.direction {
            if let Some(new_facing) = Facing::from_direction(dir) {
                *facing = new_facing;
                // Only East/West change horizontal orientation; North/South leave flip_x as-is.
                match new_facing {
                    Facing::East => sprite.flip_x = false,
                    Facing::West => sprite.flip_x = true,
                    _ => {}
                }
            }
        }
    }
}

/// Copies [`PlayerInput::direction`] into [`GridMover::direction`] each frame.
///
/// Bridges player intent to the movement simulator, keeping [`GridMover`] agnostic
/// about where its direction comes from (keyboard, AI, cutscene, etc.).
///
/// When [`GameAction::Aim`] is held the direction is suppressed so the player
/// finishes their current step and then stands still — the left stick (or mouse)
/// aims the indicator rather than moving the character.
fn apply_input_to_grid_mover(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    mut query: Query<(&PlayerInput, &mut GridMover), With<PlayerControlled>>,
) {
    let aiming = *input_mode == InputMode::Playing && action_input.pressed(GameAction::Aim);
    for (input, mut mover) in &mut query {
        mover.direction = if aiming { None } else { input.direction };
    }
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
    fn facing_default_is_east() {
        assert_eq!(Facing::default(), Facing::East);
    }

    #[test]
    fn facing_from_direction_all_cardinals() {
        assert_eq!(Facing::from_direction(IVec2::X), Some(Facing::East));
        assert_eq!(Facing::from_direction(IVec2::NEG_X), Some(Facing::West));
        assert_eq!(Facing::from_direction(IVec2::Y), Some(Facing::North));
        assert_eq!(Facing::from_direction(IVec2::NEG_Y), Some(Facing::South));
    }

    #[test]
    fn facing_from_direction_rejects_diagonal_and_zero() {
        assert_eq!(Facing::from_direction(IVec2::ZERO), None);
        assert_eq!(Facing::from_direction(IVec2::new(1, 1)), None);
        assert_eq!(Facing::from_direction(IVec2::new(-1, -1)), None);
    }

    #[test]
    fn facing_angle_matches_expected_direction() {
        let cases = [
            (Facing::East, Vec2::X),
            (Facing::West, Vec2::NEG_X),
            (Facing::North, Vec2::Y),
            (Facing::South, Vec2::NEG_Y),
        ];
        for (facing, expected) in cases {
            let from_angle = Vec2::from_angle(facing.angle());
            assert!(
                (from_angle - expected).length() < 1e-5,
                "{facing:?}: angle gave {from_angle}, expected {expected}",
            );
        }
    }
}
