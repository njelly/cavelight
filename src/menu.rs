use bevy::prelude::*;
use bevy::app::AppExit;
use bevy::ecs::message::MessageWriter;
use avian2d::prelude::PhysicsGizmos;

use crate::input::{ActionInput, GameAction};
use crate::inventory::{ActiveChest, HeldItem, InputMode};
use crate::item::Inventory;
use crate::player_input::PlayerControlled;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const PANEL_MIN_W_VH: f32 = 42.0;
const PANEL_PADDING_VH: f32 = 2.4;
const HEADER_PADDING_VH: f32 = 1.5;
const CLOSE_BTN_VH: f32 = 5.0;
const BTN_HEIGHT_VH: f32 = 7.2;
const BTN_GAP_VH: f32 = 1.6;
const SECTION_GAP_VH: f32 = 2.0;
const ROW_GAP_VH: f32 = 1.8;
const CHECK_SIZE_VH: f32 = 2.6;

const PAUSE_BUTTON_COUNT: usize = 2;

// ---------------------------------------------------------------------------
// Public resources
// ---------------------------------------------------------------------------

/// Whether the egui world inspector panel is visible.
///
/// Defined here so [`MenuPlugin`] can expose a settings toggle for it.
/// Registered and initialised by [`MenuPlugin`]; `main.rs` reads it via the
/// `WorldInspectorPlugin::run_if` condition.
#[derive(Resource, Default)]
pub struct WorldInspectorOpen(pub bool);

// ---------------------------------------------------------------------------
// Private resources
// ---------------------------------------------------------------------------

/// Zero-based index of the focused button in the pause menu (0 = Continue, 1 = Save & Quit).
///
/// Updated by WASD navigation and mouse hover. Space activates the focused button.
#[derive(Resource, Default)]
struct PauseFocusIndex(usize);

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Root node of the pause-menu overlay. Visibility is driven by [`InputMode::Paused`].
#[derive(Component)]
struct PauseOverlay;

/// Root node of the settings overlay. Visibility is driven by [`InputMode::Settings`].
#[derive(Component)]
struct SettingsOverlay;

/// A clickable button in the pause menu. Carries its list position and the action it performs.
#[derive(Component)]
struct PauseButton {
    index: usize,
    action: PauseAction,
}

/// The X close button present in the header of each menu panel.
/// Clicking it returns to [`InputMode::Playing`].
#[derive(Component)]
struct CloseMenuButton;

/// A toggle checkbox in the developer-settings section.
#[derive(Component)]
struct SettingsCheckbox(DevSetting);

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Actions that can be triggered from the pause menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PauseAction {
    Continue,
    SaveAndQuit,
}

/// Developer settings that can be toggled from the settings screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DevSetting {
    PhysicsDebug,
    WorldInspector,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers all menu-related UI and input systems.
///
/// Manages three overlay screens — Pause, Inventory (owned by [`InventoryPlugin`]),
/// and Settings — that share a dimmed backdrop and are cycled with Tab.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseFocusIndex>()
            .init_resource::<WorldInspectorOpen>()
            .add_systems(Startup, (spawn_pause_overlay, spawn_settings_overlay))
            .add_systems(
                Update,
                (
                    (
                        handle_tab_cycle,
                        handle_bumper_cycle,
                        handle_menu_escape,
                        handle_close_menu_button,
                        sync_pause_visibility,
                        sync_settings_visibility,
                        handle_pause_nav,
                    ),
                    (
                        handle_pause_confirm,
                        handle_pause_button_interaction,
                        sync_pause_button_styles,
                        sync_settings_checkboxes,
                        handle_settings_checkbox_click,
                    ),
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Startup: spawn overlays
// ---------------------------------------------------------------------------

/// Spawns the full-screen dim overlay containing the pause menu panel.
fn spawn_pause_overlay(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/RobotoMono-Regular.ttf");
    let bold = asset_server.load("fonts/RobotoMono-Bold.ttf");

    let dim = Color::srgba(0.0, 0.0, 0.0, 0.65);
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.96);
    let header_bg = Color::srgba(0.04, 0.03, 0.02, 1.0);
    let close_btn = Color::srgb(0.45, 0.12, 0.12);
    let text_col = Color::srgb(0.85, 0.80, 0.70);
    let btn_bg = Color::srgb(0.08, 0.07, 0.06);
    let btn_border = Color::srgb(0.25, 0.20, 0.16);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(dim),
            Visibility::Hidden,
            GlobalZIndex(5),
            PauseOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        min_width: Val::Vh(PANEL_MIN_W_VH),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                ))
                .with_children(|panel| {
                    // Header row: title + close button.
                    panel
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Vh(HEADER_PADDING_VH)),
                                ..default()
                            },
                            BackgroundColor(header_bg),
                        ))
                        .with_children(|header| {
                            header.spawn((
                                Text::new("Paused"),
                                TextFont { font: bold.clone(), font_size: 18.0, ..default() },
                                TextColor(text_col),
                            ));
                            header
                                .spawn((
                                    Node {
                                        width: Val::Vh(CLOSE_BTN_VH),
                                        height: Val::Vh(CLOSE_BTN_VH),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(close_btn),
                                    Interaction::default(),
                                    CloseMenuButton,
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("X"),
                                        TextFont { font: font.clone(), font_size: 14.0, ..default() },
                                        TextColor(Color::WHITE),
                                    ));
                                });
                        });

                    // Button list.
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Vh(PANEL_PADDING_VH)),
                            row_gap: Val::Vh(BTN_GAP_VH),
                            ..default()
                        })
                        .with_children(|content| {
                            let buttons = [
                                (0usize, PauseAction::Continue,    "Continue"),
                                (1usize, PauseAction::SaveAndQuit, "Save & Quit"),
                            ];
                            for (idx, action, label) in buttons {
                                content
                                    .spawn((
                                        Node {
                                            width: Val::Percent(100.0),
                                            height: Val::Vh(BTN_HEIGHT_VH),
                                            justify_content: JustifyContent::Center,
                                            align_items: AlignItems::Center,
                                            border: UiRect::all(Val::Px(2.0)),
                                            ..default()
                                        },
                                        BackgroundColor(btn_bg),
                                        BorderColor::all(btn_border),
                                        Interaction::default(),
                                        PauseButton { index: idx, action },
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((
                                            Text::new(label),
                                            TextFont { font: font.clone(), font_size: 15.0, ..default() },
                                            TextColor(text_col),
                                        ));
                                    });
                            }
                        });
                });
        });
}

/// Spawns the full-screen dim overlay containing the settings panel.
fn spawn_settings_overlay(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/RobotoMono-Regular.ttf");
    let bold = asset_server.load("fonts/RobotoMono-Bold.ttf");

    let dim = Color::srgba(0.0, 0.0, 0.0, 0.65);
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.96);
    let header_bg = Color::srgba(0.04, 0.03, 0.02, 1.0);
    let close_btn = Color::srgb(0.45, 0.12, 0.12);
    let text_col = Color::srgb(0.85, 0.80, 0.70);
    let section_col = Color::srgb(0.50, 0.45, 0.38);
    let divider_col = Color::srgba(0.30, 0.26, 0.20, 0.60);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(dim),
            Visibility::Hidden,
            GlobalZIndex(5),
            SettingsOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        min_width: Val::Vh(PANEL_MIN_W_VH),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                ))
                .with_children(|panel| {
                    // Header row: title + close button.
                    panel
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Vh(HEADER_PADDING_VH)),
                                ..default()
                            },
                            BackgroundColor(header_bg),
                        ))
                        .with_children(|header| {
                            header.spawn((
                                Text::new("Settings"),
                                TextFont { font: bold.clone(), font_size: 18.0, ..default() },
                                TextColor(Color::srgb(0.85, 0.80, 0.70)),
                            ));
                            header
                                .spawn((
                                    Node {
                                        width: Val::Vh(CLOSE_BTN_VH),
                                        height: Val::Vh(CLOSE_BTN_VH),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(close_btn),
                                    Interaction::default(),
                                    CloseMenuButton,
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("X"),
                                        TextFont { font: font.clone(), font_size: 14.0, ..default() },
                                        TextColor(Color::WHITE),
                                    ));
                                });
                        });

                    // Content area.
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Vh(PANEL_PADDING_VH)),
                            row_gap: Val::Vh(SECTION_GAP_VH),
                            ..default()
                        })
                        .with_children(|content| {
                            // ── Developer section ──
                            content.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Vh(ROW_GAP_VH),
                                ..default()
                            })
                            .with_children(|section| {
                                // Section label.
                                section.spawn((
                                    Text::new("Developer"),
                                    TextFont { font: bold.clone(), font_size: 11.0, ..default() },
                                    TextColor(section_col),
                                ));

                                // Thin divider line.
                                section.spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Px(1.0),
                                        ..default()
                                    },
                                    BackgroundColor(divider_col),
                                ));

                                // Toggle rows.
                                spawn_toggle_row(
                                    section,
                                    &font,
                                    text_col,
                                    "Physics Debug",
                                    "[F1]",
                                    DevSetting::PhysicsDebug,
                                );
                                spawn_toggle_row(
                                    section,
                                    &font,
                                    text_col,
                                    "World Inspector",
                                    "[F2]",
                                    DevSetting::WorldInspector,
                                );
                            });
                        });
                });
        });
}

/// Spawns a single toggle row (checkbox + label + key hint) inside a settings section.
fn spawn_toggle_row(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    text_col: Color,
    label: &'static str,
    hint: &'static str,
    setting: DevSetting,
) {
    let check_inactive = Color::srgb(0.08, 0.07, 0.06);
    let check_border = Color::srgb(0.32, 0.27, 0.22);

    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Vh(1.4),
            ..default()
        })
        .with_children(|row| {
            // Checkbox square — BackgroundColor is synced each frame.
            row.spawn((
                Node {
                    width: Val::Vh(CHECK_SIZE_VH),
                    height: Val::Vh(CHECK_SIZE_VH),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(check_inactive),
                BorderColor::all(check_border),
                Interaction::default(),
                SettingsCheckbox(setting),
            ));

            // Label.
            row.spawn((
                Text::new(label),
                TextFont { font: font.clone(), font_size: 14.0, ..default() },
                TextColor(text_col),
            ));

            // Key hint (dimmer).
            row.spawn((
                Text::new(hint),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(Color::srgba(0.55, 0.50, 0.42, 0.80)),
            ));
        });
}

// ---------------------------------------------------------------------------
// Tab cycling
// ---------------------------------------------------------------------------

/// Cycles through menu screens with Tab or gamepad Start, or opens the pause screen from gameplay.
///
/// Order: Playing → Paused → Inventory → Settings → Paused (loops).
/// Any held inventory item is returned to the player when leaving the inventory screen.
fn handle_tab_cycle(
    action_input: Res<ActionInput>,
    mut input_mode: ResMut<InputMode>,
    mut held: ResMut<HeldItem>,
    mut active_chest: ResMut<ActiveChest>,
    mut player_inv: Query<&mut Inventory, With<PlayerControlled>>,
    mut focus: ResMut<PauseFocusIndex>,
) {
    if !action_input.just_pressed(GameAction::OpenPause) || *input_mode == InputMode::Dialogue {
        return;
    }

    // Return held item and close chest when leaving the inventory screen via Tab.
    if *input_mode == InputMode::Inventory {
        return_held_item(&mut held, &mut player_inv);
        active_chest.0 = None;
    }

    *input_mode = match *input_mode {
        InputMode::Playing => {
            focus.0 = 0;
            InputMode::Paused
        }
        InputMode::Paused    => InputMode::Inventory,
        InputMode::Inventory => InputMode::Settings,
        InputMode::Settings  => {
            focus.0 = 0;
            InputMode::Paused
        }
        InputMode::Dialogue  => return,
    };
}

/// Cycles through menu screens with LB (backward) / RB (forward) while any menu screen is active.
///
/// Screen order: Pause → Inventory → Settings → (wraps back to Pause).
/// LB steps backward through that order; RB steps forward. Performs the same inventory cleanup
/// as [`handle_tab_cycle`] when leaving the inventory screen (returns held item, closes chest).
fn handle_bumper_cycle(
    action_input: Res<ActionInput>,
    mut input_mode: ResMut<InputMode>,
    mut held: ResMut<HeldItem>,
    mut active_chest: ResMut<ActiveChest>,
    mut player_inv: Query<&mut Inventory, With<PlayerControlled>>,
    mut focus: ResMut<PauseFocusIndex>,
) {
    if !matches!(*input_mode, InputMode::Paused | InputMode::Inventory | InputMode::Settings) {
        return;
    }

    let prev = action_input.just_pressed(GameAction::HotbarPrev);
    let next = action_input.just_pressed(GameAction::HotbarNext);

    if !prev && !next {
        return;
    }

    if *input_mode == InputMode::Inventory {
        return_held_item(&mut held, &mut player_inv);
        active_chest.0 = None;
    }

    const SCREENS: [InputMode; 3] = [InputMode::Paused, InputMode::Inventory, InputMode::Settings];
    let current = SCREENS.iter().position(|&s| s == *input_mode).unwrap_or(0);
    let next_idx = if next {
        (current + 1) % SCREENS.len()
    } else {
        (current + SCREENS.len() - 1) % SCREENS.len()
    };

    *input_mode = SCREENS[next_idx];

    if *input_mode == InputMode::Paused {
        focus.0 = 0;
    }
}

// ---------------------------------------------------------------------------
// Close / escape
// ---------------------------------------------------------------------------

/// Closes the menu entirely (returns to gameplay) when Cancel (Escape / gamepad B) is pressed
/// while the pause or settings screen is active.
///
/// Inventory cancel is handled separately by `inventory::close_inventory` since it
/// has extra cleanup (returning held items, clearing active chest).
fn handle_menu_escape(
    action_input: Res<ActionInput>,
    mut input_mode: ResMut<InputMode>,
) {
    if !action_input.just_pressed(GameAction::Cancel) {
        return;
    }
    if matches!(*input_mode, InputMode::Paused | InputMode::Settings) {
        *input_mode = InputMode::Playing;
    }
}

/// Handles clicks on the shared close (X) button present in every menu panel header.
fn handle_close_menu_button(
    buttons: Query<&Interaction, (With<CloseMenuButton>, Changed<Interaction>)>,
    mut input_mode: ResMut<InputMode>,
) {
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            *input_mode = InputMode::Playing;
        }
    }
}

// ---------------------------------------------------------------------------
// Visibility sync
// ---------------------------------------------------------------------------

/// Shows the pause overlay only while [`InputMode::Paused`] is active.
fn sync_pause_visibility(
    input_mode: Res<InputMode>,
    mut overlay: Query<&mut Visibility, With<PauseOverlay>>,
) {
    if !input_mode.is_changed() {
        return;
    }
    let vis = if *input_mode == InputMode::Paused {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut overlay {
        *v = vis;
    }
}

/// Shows the settings overlay only while [`InputMode::Settings`] is active.
fn sync_settings_visibility(
    input_mode: Res<InputMode>,
    mut overlay: Query<&mut Visibility, With<SettingsOverlay>>,
) {
    if !input_mode.is_changed() {
        return;
    }
    let vis = if *input_mode == InputMode::Settings {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut overlay {
        *v = vis;
    }
}

// ---------------------------------------------------------------------------
// Pause menu navigation
// ---------------------------------------------------------------------------

/// Moves focus up (W / ↑ / D-pad up / left stick up) or down through the pause button list.
fn handle_pause_nav(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    mut focus: ResMut<PauseFocusIndex>,
) {
    if *input_mode != InputMode::Paused {
        return;
    }
    if action_input.just_pressed(GameAction::MoveNorth) {
        focus.0 = focus.0.saturating_sub(1);
    } else if action_input.just_pressed(GameAction::MoveSouth) {
        focus.0 = (focus.0 + 1).min(PAUSE_BUTTON_COUNT - 1);
    }
}

/// Activates the focused pause-menu button when Confirm (Space / gamepad A) is pressed.
fn handle_pause_confirm(
    action_input: Res<ActionInput>,
    mut input_mode: ResMut<InputMode>,
    focus: Res<PauseFocusIndex>,
    buttons: Query<&PauseButton>,
    mut exit: MessageWriter<AppExit>,
) {
    if *input_mode != InputMode::Paused || !action_input.just_pressed(GameAction::Confirm) {
        return;
    }
    for btn in &buttons {
        if btn.index == focus.0 {
            execute_pause_action(btn.action, &mut input_mode, &mut exit);
            return;
        }
    }
}

/// Syncs keyboard focus with mouse hover and fires the action on click.
fn handle_pause_button_interaction(
    mut buttons: Query<(&Interaction, &PauseButton), Changed<Interaction>>,
    mut input_mode: ResMut<InputMode>,
    mut focus: ResMut<PauseFocusIndex>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, btn) in &mut buttons {
        match *interaction {
            Interaction::Hovered => focus.0 = btn.index,
            Interaction::Pressed => {
                focus.0 = btn.index;
                execute_pause_action(btn.action, &mut input_mode, &mut exit);
            }
            _ => {}
        }
    }
}

/// Updates button background and border colours to reflect the current focus index.
fn sync_pause_button_styles(
    focus: Res<PauseFocusIndex>,
    mut buttons: Query<(&PauseButton, &mut BackgroundColor, &mut BorderColor)>,
) {
    if !focus.is_changed() {
        return;
    }
    for (btn, mut bg, mut border) in &mut buttons {
        if btn.index == focus.0 {
            *bg = BackgroundColor(Color::srgb(0.20, 0.17, 0.13));
            *border = BorderColor::all(Color::srgb(0.65, 0.55, 0.32));
        } else {
            *bg = BackgroundColor(Color::srgb(0.08, 0.07, 0.06));
            *border = BorderColor::all(Color::srgb(0.25, 0.20, 0.16));
        }
    }
}

/// Executes the given pause-menu action, mutating mode or sending an exit event.
fn execute_pause_action(
    action: PauseAction,
    input_mode: &mut ResMut<InputMode>,
    exit: &mut MessageWriter<AppExit>,
) {
    match action {
        PauseAction::Continue    => **input_mode = InputMode::Playing,
        PauseAction::SaveAndQuit => { exit.write(AppExit::Success); }
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Updates checkbox background colours to reflect current developer-setting states.
fn sync_settings_checkboxes(
    gizmo_store: Res<GizmoConfigStore>,
    world_inspector: Res<WorldInspectorOpen>,
    mut checkboxes: Query<(&SettingsCheckbox, &mut BackgroundColor)>,
) {
    let physics_on  = gizmo_store.config::<PhysicsGizmos>().0.enabled;
    let inspector_on = world_inspector.0;

    for (checkbox, mut bg) in &mut checkboxes {
        let active = match checkbox.0 {
            DevSetting::PhysicsDebug    => physics_on,
            DevSetting::WorldInspector  => inspector_on,
        };
        *bg = BackgroundColor(if active {
            Color::srgb(0.30, 0.68, 0.28)
        } else {
            Color::srgb(0.08, 0.07, 0.06)
        });
    }
}

/// Toggles a developer setting when its checkbox is clicked.
fn handle_settings_checkbox_click(
    checkboxes: Query<(&Interaction, &SettingsCheckbox), Changed<Interaction>>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
    mut world_inspector: ResMut<WorldInspectorOpen>,
) {
    for (interaction, checkbox) in &checkboxes {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match checkbox.0 {
            DevSetting::PhysicsDebug => {
                gizmo_store.config_mut::<PhysicsGizmos>().0.enabled ^= true;
            }
            DevSetting::WorldInspector => {
                world_inspector.0 ^= true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns any item held by the cursor back into the player's inventory.
fn return_held_item(
    held: &mut ResMut<HeldItem>,
    player_inv: &mut Query<&mut Inventory, With<PlayerControlled>>,
) {
    if let Some(stack) = held.0.take() {
        if let Ok(mut inv) = player_inv.single_mut() {
            if !inv.insert_first_empty(stack.clone()) {
                warn!("Inventory full — dropped '{}' when leaving inventory screen.", stack.id);
            }
        }
    }
}
