use std::collections::HashSet;

use bevy::input::InputSystems;
use bevy::prelude::*;

/// Analog axis dead-zone — stick values below this magnitude are treated as zero.
const STICK_DEADZONE: f32 = 0.5;

/// Tracks which input device produced the most recent significant input.
///
/// Updated every frame by [`update_action_input`]. Systems that need to switch
/// between mouse-driven and gamepad-driven UI (aim indicator, inventory cursor)
/// should read this resource rather than polling devices directly.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InputSource {
    /// Last input came from keyboard or mouse.
    #[default]
    KeyboardMouse,
    /// Last input came from a gamepad.
    Gamepad,
}

/// Tracks whether the left analog stick was past the dead-zone threshold last frame,
/// per cardinal direction.
///
/// Compared against the current frame to generate `just_press` on the initial threshold
/// crossing, giving menu/inventory navigation the same single-step feel as D-pad input.
#[derive(Resource, Default)]
struct StickNavState {
    north: bool,
    south: bool,
    west: bool,
    east: bool,
}

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
            .init_resource::<StickNavState>()
            .init_resource::<InputSource>()
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
/// - Left stick / D-pad → movement (left stick also navigates menus via threshold crossing)
/// - A (South) → Confirm
/// - B (East) → Cancel
/// - Y (North) → OpenInventory
/// - Start → OpenPause
/// - LB (LeftTrigger) → HotbarPrev
/// - RB (RightTrigger) → HotbarNext
/// - RT (Right trigger axis) → Aim
fn update_action_input(
    mut action_input: ResMut<ActionInput>,
    mut stick_nav: ResMut<StickNavState>,
    mut input_source: ResMut<InputSource>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
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

    // --- Gamepad ---
    // Accumulate left stick state across all connected pads (any-pad OR logic).
    let mut new_stick = StickNavState::default();

    for gamepad in &gamepads {
        let stick_x = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        let stick_y = gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);

        if stick_y >  STICK_DEADZONE { new_stick.north = true; }
        if stick_y < -STICK_DEADZONE { new_stick.south = true; }
        if stick_x < -STICK_DEADZONE { new_stick.west  = true; }
        if stick_x >  STICK_DEADZONE { new_stick.east  = true; }

        // D-pad → directional movement (held + initial press for UI navigation).
        if gamepad.pressed(GamepadButton::DPadUp)    { action_input.press(GameAction::MoveNorth); }
        if gamepad.pressed(GamepadButton::DPadDown)  { action_input.press(GameAction::MoveSouth); }
        if gamepad.pressed(GamepadButton::DPadLeft)  { action_input.press(GameAction::MoveWest); }
        if gamepad.pressed(GamepadButton::DPadRight) { action_input.press(GameAction::MoveEast); }
        if gamepad.just_pressed(GamepadButton::DPadUp)    { action_input.just_press(GameAction::MoveNorth); }
        if gamepad.just_pressed(GamepadButton::DPadDown)  { action_input.just_press(GameAction::MoveSouth); }
        if gamepad.just_pressed(GamepadButton::DPadLeft)  { action_input.just_press(GameAction::MoveWest); }
        if gamepad.just_pressed(GamepadButton::DPadRight) { action_input.just_press(GameAction::MoveEast); }

        // Right trigger → aim hold.
        // PS5 reports R2 as RightTrigger2 (digital button); Xbox reports it as RightZ (analog axis).
        let rt_axis = gamepad.get(GamepadAxis::RightZ).unwrap_or(0.0);
        if rt_axis > STICK_DEADZONE || gamepad.pressed(GamepadButton::RightTrigger2) {
            action_input.press(GameAction::Aim);
        }

        // Face buttons and shoulders → just pressed actions.
        if gamepad.just_pressed(GamepadButton::South)        { action_input.just_press(GameAction::Confirm); }
        if gamepad.just_pressed(GamepadButton::East)         { action_input.just_press(GameAction::Cancel); }
        if gamepad.just_pressed(GamepadButton::North)        { action_input.just_press(GameAction::OpenInventory); }
        if gamepad.just_pressed(GamepadButton::Start)        { action_input.just_press(GameAction::OpenPause); }
        if gamepad.just_pressed(GamepadButton::LeftTrigger)  { action_input.just_press(GameAction::HotbarPrev); }
        if gamepad.just_pressed(GamepadButton::RightTrigger) { action_input.just_press(GameAction::HotbarNext); }
    }

    // Apply left stick with threshold-crossing detection.
    // Fires just_press on the first frame the stick crosses the dead-zone (enabling
    // single-step UI navigation), then press-only while held.
    if new_stick.north {
        if !stick_nav.north { action_input.just_press(GameAction::MoveNorth); }
        else { action_input.press(GameAction::MoveNorth); }
    }
    if new_stick.south {
        if !stick_nav.south { action_input.just_press(GameAction::MoveSouth); }
        else { action_input.press(GameAction::MoveSouth); }
    }
    if new_stick.west {
        if !stick_nav.west { action_input.just_press(GameAction::MoveWest); }
        else { action_input.press(GameAction::MoveWest); }
    }
    if new_stick.east {
        if !stick_nav.east { action_input.just_press(GameAction::MoveEast); }
        else { action_input.press(GameAction::MoveEast); }
    }

    // Persist stick state for next frame's crossing detection.
    *stick_nav = new_stick;

    // --- Detect active input source ---
    // Switch to Gamepad when any significant gamepad input is seen; switch back to
    // KeyboardMouse when any key or mouse button is pressed. Only switches on activity
    // so the source stays stable between inputs.
    let gamepad_active = gamepads.iter().any(|gp| {
        let lx = gp.get(GamepadAxis::LeftStickX).unwrap_or(0.0).abs();
        let ly = gp.get(GamepadAxis::LeftStickY).unwrap_or(0.0).abs();
        let rx = gp.get(GamepadAxis::RightStickX).unwrap_or(0.0).abs();
        let ry = gp.get(GamepadAxis::RightStickY).unwrap_or(0.0).abs();
        let rz = gp.get(GamepadAxis::RightZ).unwrap_or(0.0);
        let stick_moved = lx > STICK_DEADZONE || ly > STICK_DEADZONE
            || rx > STICK_DEADZONE || ry > STICK_DEADZONE
            || rz > STICK_DEADZONE;
        let btn_pressed = gp.get_just_pressed().next().is_some()
            || gp.pressed(GamepadButton::RightTrigger2);
        stick_moved || btn_pressed
    });
    let kb_active = keys.get_just_pressed().next().is_some()
        || mouse_buttons.get_just_pressed().next().is_some();

    if gamepad_active {
        *input_source = InputSource::Gamepad;
    } else if kb_active {
        *input_source = InputSource::KeyboardMouse;
    }
}
