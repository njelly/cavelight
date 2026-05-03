use std::collections::HashSet;

use bevy::input::InputSystems;
use bevy::prelude::*;

/// Analog axis dead-zone — stick values below this magnitude are treated as zero.
const STICK_DEADZONE: f32 = 0.5;

/// All named gameplay actions that can be triggered from keyboard or gamepad.
///
/// Systems consume [`ActionInput`] rather than reading [`ButtonInput<KeyCode>`] directly,
/// so every system gains controller support without any per-system changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameAction {
    /// Move the player north (held). WASD/arrows, left stick up, or D-pad up.
    MoveNorth,
    /// Move the player south (held). WASD/arrows, left stick down, or D-pad down.
    MoveSouth,
    /// Move the player west (held). WASD/arrows, left stick left, or D-pad left.
    MoveWest,
    /// Move the player east (held). WASD/arrows, left stick right, or D-pad right.
    MoveEast,
    /// Confirm / interact / advance dialogue. Space or A (South button).
    Confirm,
    /// Cancel / close menu / go back. Escape or B (East button).
    Cancel,
    /// Open the player inventory. I key or Y (North button).
    OpenInventory,
    /// Open / cycle the pause menu. Tab or Start button.
    OpenPause,
    /// Hold to aim a ranged weapon. Shift or Right Trigger (RT).
    Aim,
    /// Select hotbar slot 1 directly. Key 1.
    HotbarSlot1,
    /// Select hotbar slot 2 directly. Key 2.
    HotbarSlot2,
    /// Select hotbar slot 3 directly. Key 3.
    HotbarSlot3,
    /// Select hotbar slot 4 directly. Key 4.
    HotbarSlot4,
    /// Cycle the hotbar to the previous slot. Q or Left Bumper (LB).
    HotbarPrev,
    /// Cycle the hotbar to the next slot. E or Right Bumper (RB).
    HotbarNext,
}

/// Merged keyboard + gamepad input state for the current frame.
///
/// Populated each frame during [`PreUpdate`] by [`update_action_input`] before any
/// gameplay system runs. Systems read this resource instead of querying
/// [`ButtonInput<KeyCode>`] directly, gaining automatic controller support.
#[derive(Resource, Default)]
pub struct ActionInput {
    pressed: HashSet<GameAction>,
    just_pressed: HashSet<GameAction>,
}

impl ActionInput {
    /// Returns `true` if `action` is currently held (this frame or a prior frame).
    pub fn pressed(&self, action: GameAction) -> bool {
        self.pressed.contains(&action)
    }

    /// Returns `true` if `action` was first pressed this frame (not held from before).
    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.just_pressed.contains(&action)
    }

    /// Marks `action` as held this frame (not a new press).
    fn press(&mut self, action: GameAction) {
        self.pressed.insert(action);
    }

    /// Marks `action` as freshly pressed this frame (also counts as held).
    fn just_press(&mut self, action: GameAction) {
        self.just_pressed.insert(action);
        self.pressed.insert(action);
    }

    /// Clears all state. Called at the start of each frame before re-populating.
    fn clear(&mut self) {
        self.pressed.clear();
        self.just_pressed.clear();
    }
}

/// Collects keyboard and gamepad input into [`ActionInput`] each frame.
pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionInput>()
            // Must run after InputSystems so the Gamepad component has been updated
            // by bevy_gilrs for the current frame before we read it.
            .add_systems(PreUpdate, update_action_input.after(InputSystems));
    }
}

/// Rebuilds [`ActionInput`] from the keyboard and all connected gamepads.
///
/// Runs in [`PreUpdate`] so gameplay systems in [`Update`] always see a fully
/// populated [`ActionInput`] on their first read each frame.
///
/// Xbox 360 mapping:
/// - Left stick / D-pad → movement
/// - A (South) → Confirm
/// - B (East) → Cancel
/// - Y (North) → OpenInventory
/// - Start → OpenPause
/// - LB (LeftTrigger) → HotbarPrev
/// - RB (RightTrigger) → HotbarNext
/// - RT (Right trigger axis) → Aim
fn update_action_input(
    mut action_input: ResMut<ActionInput>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
) {
    action_input.clear();

    // --- Keyboard: held (pressed) ---
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp)    { action_input.press(GameAction::MoveNorth); }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown)  { action_input.press(GameAction::MoveSouth); }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft)  { action_input.press(GameAction::MoveWest); }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) { action_input.press(GameAction::MoveEast); }
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { action_input.press(GameAction::Aim); }

    // --- Keyboard: initial press (for UI navigation — just_press also sets pressed) ---
    if keys.just_pressed(KeyCode::KeyW) || keys.just_pressed(KeyCode::ArrowUp)    { action_input.just_press(GameAction::MoveNorth); }
    if keys.just_pressed(KeyCode::KeyS) || keys.just_pressed(KeyCode::ArrowDown)  { action_input.just_press(GameAction::MoveSouth); }
    if keys.just_pressed(KeyCode::KeyA) || keys.just_pressed(KeyCode::ArrowLeft)  { action_input.just_press(GameAction::MoveWest); }
    if keys.just_pressed(KeyCode::KeyD) || keys.just_pressed(KeyCode::ArrowRight) { action_input.just_press(GameAction::MoveEast); }

    // --- Keyboard: just pressed ---
    if keys.just_pressed(KeyCode::Space)  { action_input.just_press(GameAction::Confirm); }
    if keys.just_pressed(KeyCode::Escape) { action_input.just_press(GameAction::Cancel); }
    if keys.just_pressed(KeyCode::KeyI)   { action_input.just_press(GameAction::OpenInventory); }
    if keys.just_pressed(KeyCode::Tab)    { action_input.just_press(GameAction::OpenPause); }
    if keys.just_pressed(KeyCode::Digit1) { action_input.just_press(GameAction::HotbarSlot1); }
    if keys.just_pressed(KeyCode::Digit2) { action_input.just_press(GameAction::HotbarSlot2); }
    if keys.just_pressed(KeyCode::Digit3) { action_input.just_press(GameAction::HotbarSlot3); }
    if keys.just_pressed(KeyCode::Digit4) { action_input.just_press(GameAction::HotbarSlot4); }
    if keys.just_pressed(KeyCode::KeyQ)   { action_input.just_press(GameAction::HotbarPrev); }
    if keys.just_pressed(KeyCode::KeyE)   { action_input.just_press(GameAction::HotbarNext); }

    // --- Gamepad (iterate all connected pads; first input wins via the HashSet) ---
    for gamepad in &gamepads {
        // Left analog stick → directional movement (held, with dead-zone).
        let stick_x = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        let stick_y = gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);

        if stick_y >  STICK_DEADZONE { action_input.press(GameAction::MoveNorth); }
        if stick_y < -STICK_DEADZONE { action_input.press(GameAction::MoveSouth); }
        if stick_x < -STICK_DEADZONE { action_input.press(GameAction::MoveWest); }
        if stick_x >  STICK_DEADZONE { action_input.press(GameAction::MoveEast); }

        // D-pad → directional movement (held + initial press for UI navigation).
        if gamepad.pressed(GamepadButton::DPadUp)    { action_input.press(GameAction::MoveNorth); }
        if gamepad.pressed(GamepadButton::DPadDown)  { action_input.press(GameAction::MoveSouth); }
        if gamepad.pressed(GamepadButton::DPadLeft)  { action_input.press(GameAction::MoveWest); }
        if gamepad.pressed(GamepadButton::DPadRight) { action_input.press(GameAction::MoveEast); }
        if gamepad.just_pressed(GamepadButton::DPadUp)    { action_input.just_press(GameAction::MoveNorth); }
        if gamepad.just_pressed(GamepadButton::DPadDown)  { action_input.just_press(GameAction::MoveSouth); }
        if gamepad.just_pressed(GamepadButton::DPadLeft)  { action_input.just_press(GameAction::MoveWest); }
        if gamepad.just_pressed(GamepadButton::DPadRight) { action_input.just_press(GameAction::MoveEast); }

        // Right trigger (analog axis, RT) → aim hold.
        let rt = gamepad.get(GamepadAxis::RightZ).unwrap_or(0.0);
        if rt > STICK_DEADZONE { action_input.press(GameAction::Aim); }

        // Face buttons and shoulders → just pressed actions.
        if gamepad.just_pressed(GamepadButton::South)        { action_input.just_press(GameAction::Confirm); }
        if gamepad.just_pressed(GamepadButton::East)         { action_input.just_press(GameAction::Cancel); }
        if gamepad.just_pressed(GamepadButton::North)        { action_input.just_press(GameAction::OpenInventory); }
        if gamepad.just_pressed(GamepadButton::Start)        { action_input.just_press(GameAction::OpenPause); }
        if gamepad.just_pressed(GamepadButton::LeftTrigger)  { action_input.just_press(GameAction::HotbarPrev); }
        if gamepad.just_pressed(GamepadButton::RightTrigger) { action_input.just_press(GameAction::HotbarNext); }
    }
}
