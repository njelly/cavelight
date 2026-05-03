use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::chest::Chest;
use crate::input::{ActionInput, GameAction, InputSource};
use crate::item::{Inventory, ItemLibrary, ItemStack};
use crate::player_input::PlayerControlled;

// ---------------------------------------------------------------------------
// Layout constants (sizes as % of viewport height so the UI scales uniformly)
// ---------------------------------------------------------------------------

const GRID_COLS: usize = 4;
const GRID_ROWS: usize = 4;
const HOTBAR_SLOTS: usize = 4;

const SLOT_VH: f32 = 10.0;
const SLOT_GAP_VH: f32 = 2.4;
const GRID_PADDING_VH: f32 = 1.2;
const PANEL_GAP_VH: f32 = 4.0;

const HOTBAR_SLOT_VH: f32 = SLOT_VH;
const HOTBAR_PADDING_VH: f32 = 0.8;
const HOTBAR_BOTTOM_MARGIN_VH: f32 = 1.5;
/// Height of the ammo sub-view strip that floats below the equipped hotbar slot.
const AMMO_SUBVIEW_VH: f32 = 3.2;
/// Gap between a hotbar slot and its ammo sub-view.
const AMMO_SUBVIEW_GAP_VH: f32 = 0.5;
/// Total vh the hotbar occupies from the bottom edge of the screen.
///
/// Used by sibling modules (e.g. dialogue) to anchor panels directly above the hotbar.
/// Includes space for the ammo sub-view that appears beneath each slot.
pub const HOTBAR_HEIGHT_VH: f32 =
    HOTBAR_SLOT_VH + AMMO_SUBVIEW_GAP_VH + AMMO_SUBVIEW_VH
    + 2.0 * HOTBAR_PADDING_VH + HOTBAR_BOTTOM_MARGIN_VH;

/// Index of the first hotbar slot inside the player's [`Inventory`].
///
/// Slots 0..HOTBAR_START are the 4×4 main grid; slots HOTBAR_START.. are hotbar slots.
pub const HOTBAR_START: usize = GRID_COLS * GRID_ROWS;

const SLOT_BORDER_PX: f32 = 2.0;
const HEADER_PADDING_VH: f32 = 1.5;
const CLOSE_BTN_SIZE_VH: f32 = 5.0;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Controls whether player movement / world interaction or a UI panel
/// receives keyboard and mouse input.
///
/// Systems that handle player input check this resource and bail early unless
/// [`InputMode::Playing`] is active. Each exclusive mode owns input for its lifetime.
#[derive(Resource, Default, PartialEq, Eq, Debug, Clone, Copy, Reflect)]
#[reflect(Resource)]
pub enum InputMode {
    /// Normal gameplay — player movement and world interaction are active.
    #[default]
    Playing,
    /// Inventory screen is open — input goes to the UI instead.
    Inventory,
    /// Dialogue panel is open — Space advances pages, player cannot move or interact.
    Dialogue,
    /// Pause menu is open — WASD navigates buttons, Space confirms.
    Paused,
    /// Settings screen is open.
    Settings,
}

/// Which hotbar slot (0–3) is currently equipped, if any.
///
/// Set by pressing keys 1–4 during [`InputMode::Playing`]. Systems that need to
/// know the active item (e.g. combat, use-item) read this resource.
#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
pub struct EquippedHotbarSlot(pub Option<usize>);

/// Tracks the chest entity (if any) whose inventory is currently displayed.
///
/// `Some(entity)` shows the chest panel alongside the player panel.
/// `None` shows only the player inventory.
#[derive(Resource, Default)]
pub struct ActiveChest(pub Option<Entity>);

/// The item stack currently held by the player's cursor inside the inventory UI.
///
/// Clicking a slot always swaps this with the slot's contents (even if one or
/// both sides are empty). Returned to the player inventory when the screen closes.
#[derive(Resource, Default)]
pub struct HeldItem(pub Option<ItemStack>);

/// Tracks the slot from which the currently held item was originally picked up.
///
/// Set when a pickup begins (slot had an item, `HeldItem` was empty); cleared when
/// the item is placed. Used by the Cancel action to return the item to its origin
/// rather than dumping it into the first available slot.
#[derive(Resource, Default)]
pub struct HeldItemSource(pub Option<InventorySlotRef>);

/// Tracks the focused inventory slot for keyboard and gamepad navigation.
///
/// Row and column are relative to the panel's [`GRID_ROWS`] × [`GRID_COLS`] grid, except
/// when `in_hotbar` is true, in which case `col` indexes the hotbar (0–3) and `row` is
/// ignored. Updated by [`navigate_inventory`]; read by [`confirm_inventory_slot`] and
/// [`sync_inventory_focus_highlight`]. Reset to default when the inventory opens.
#[derive(Resource)]
pub struct InventoryFocusSlot {
    /// Which panel currently holds focus (ignored when `in_hotbar` is true).
    pub panel: InventoryPanel,
    /// Grid row (0 = top row, 3 = bottom row). Ignored when `in_hotbar` is true.
    pub row: usize,
    /// Grid column / hotbar slot index (0 = leftmost, 3 = rightmost).
    pub col: usize,
    /// When true, focus is on the hotbar row rather than the inventory grid.
    pub in_hotbar: bool,
}

impl Default for InventoryFocusSlot {
    fn default() -> Self {
        Self { panel: InventoryPanel::Player, row: 0, col: 0, in_hotbar: false }
    }
}

// ---------------------------------------------------------------------------
// Panel identifier
// ---------------------------------------------------------------------------

/// Which inventory panel a UI slot belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryPanel {
    Player,
    Chest,
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marker for the inventory screen overlay root node.
///
/// Toggling [`Visibility`] on this entity hides the dim background and all panels.
#[derive(Component)]
struct InventoryOverlay;

/// Marker for the chest inventory panel, shown only when [`ActiveChest`] is set.
#[derive(Component)]
struct ChestPanel;

/// Identifies an individual inventory slot in the UI.
///
/// Present on both the slot background node (has [`Interaction`] for click detection)
/// and on the icon child node (queried by the icon-sync system).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct InventorySlotRef {
    /// Which inventory this slot belongs to.
    pub panel: InventoryPanel,
    /// Zero-based slot index (top-left = 0).
    pub index: usize,
}

/// Marker for the icon [`ImageNode`] child inside an inventory slot.
///
/// Hidden when the slot is empty; shows the item's icon texture when occupied.
#[derive(Component)]
struct SlotIcon;

/// Marker for the stack-count [`Text`] overlay in the bottom-right of an inventory slot.
///
/// Visible only when the slot holds a stackable item (`max_stack > 1`) with `count > 0`.
/// Carries [`InventorySlotRef`] so [`sync_stack_counts`] can map it to the correct stack.
#[derive(Component)]
struct StackCount;

/// Container node shown in the bottom-left corner of an equipped hotbar slot when the
/// equipped item uses ammo and the player has that ammo in their inventory.
///
/// Carries [`InventorySlotRef`] matching its hotbar slot. Visibility is controlled by
/// [`sync_ammo_subview`].
#[derive(Component)]
struct AmmoSubView;

/// Marker for the ammo icon [`ImageNode`] inside an [`AmmoSubView`].
#[derive(Component)]
struct AmmoIcon;

/// Marker for the ammo count [`Text`] inside an [`AmmoSubView`].
#[derive(Component)]
struct AmmoCount;

/// Marker for the `X` close button in the inventory header.
#[derive(Component)]
struct CloseInventoryButton;

/// Floating icon node that follows the cursor while an item is held.
///
/// Updated every frame from the window cursor position. Has no [`Interaction`]
/// so it never intercepts clicks on the slot nodes beneath it.
#[derive(Component)]
struct HeldItemCursor;

/// Marker for the always-visible hotbar slot nodes.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct HotbarSlot;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Manages the inventory UI, item swapping, and input mode switching.
pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMode>()
            .init_resource::<ActiveChest>()
            .init_resource::<HeldItem>()
            .init_resource::<HeldItemSource>()
            .init_resource::<InventoryFocusSlot>()
            .init_resource::<EquippedHotbarSlot>()
            .register_type::<InputMode>()
            .register_type::<HotbarSlot>()
            .register_type::<EquippedHotbarSlot>()
            .add_systems(Startup, (spawn_inventory_ui, spawn_hotbar))
            .add_systems(
                Update,
                (
                    toggle_inventory,
                    close_inventory,
                    handle_slot_click,
                    navigate_inventory,
                    confirm_inventory_slot,
                    sync_slot_icons,
                    update_held_cursor,
                    sync_overlay_visibility,
                    sync_chest_panel_visibility,
                    sync_inventory_focus_highlight,
                    reset_focus_on_chest_close,
                    select_hotbar_slot,
                    sync_hotbar_borders,
                    sync_stack_counts,
                    sync_ammo_subview,
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

/// Builds the inventory overlay (dim + chest/player panels + close button) and the
/// floating held-item cursor node.
///
/// The overlay is hidden until [`InputMode::Inventory`] is set. The chest panel
/// is always present but hidden until [`ActiveChest`] holds a target entity.
///
/// All child spawning is done through `with_children` closures so that `Commands`
/// is never borrowed twice simultaneously.
fn spawn_inventory_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    let count_font: Handle<Font> = asset_server.load("fonts/RobotoMono-Bold.ttf");
    let slot_bg = Color::srgb(0.08, 0.07, 0.06);
    let slot_border = Color::srgb(0.32, 0.27, 0.22);
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.92);
    let header_bg = Color::srgba(0.04, 0.03, 0.02, 1.0);
    let dim = Color::srgba(0.0, 0.0, 0.0, 0.65);
    let text_col = Color::srgb(0.85, 0.80, 0.70);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                // Push panels above the always-visible hotbar.
                padding: UiRect::bottom(Val::Vh(HOTBAR_HEIGHT_VH)),
                ..default()
            },
            BackgroundColor(dim),
            Visibility::Hidden,
            GlobalZIndex(5),
            InventoryOverlay,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                ))
                .with_children(|container| {
                    // --- Header: title + X close button ---
                    container
                        .spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::all(Val::Vh(HEADER_PADDING_VH)),
                                column_gap: Val::Vh(PANEL_GAP_VH),
                                ..default()
                            },
                            BackgroundColor(header_bg),
                        ))
                        .with_children(|header| {
                            header.spawn((
                                Text::new("Inventory"),
                                TextFont { font_size: 16.0, ..default() },
                                TextColor(text_col),
                            ));
                            header
                                .spawn((
                                    Node {
                                        width: Val::Vh(CLOSE_BTN_SIZE_VH),
                                        height: Val::Vh(CLOSE_BTN_SIZE_VH),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgb(0.45, 0.12, 0.12)),
                                    Interaction::default(),
                                    CloseInventoryButton,
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new("X"),
                                        TextFont { font_size: 14.0, ..default() },
                                        TextColor(Color::WHITE),
                                    ));
                                });
                        });

                    // --- Content row: chest panel (hidden) + player panel ---
                    container
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Vh(PANEL_GAP_VH),
                            padding: UiRect::all(Val::Vh(GRID_PADDING_VH)),
                            ..default()
                        })
                        .with_children(|content| {
                            // --- Chest panel (hidden until ActiveChest is set) ---
                            content
                                .spawn((
                                    Node {
                                        flex_direction: FlexDirection::Column,
                                        row_gap: Val::Vh(SLOT_GAP_VH),
                                        // Start collapsed so it takes no space until a chest is opened.
                                        display: Display::None,
                                        ..default()
                                    },
                                    Visibility::Hidden,
                                    ChestPanel,
                                ))
                                .with_children(|panel| {
                                    panel.spawn((
                                        Text::new("Chest"),
                                        TextFont { font_size: 13.0, ..default() },
                                        TextColor(text_col),
                                        Node {
                                            margin: UiRect::bottom(Val::Vh(1.2)),
                                            ..default()
                                        },
                                    ));
                                    panel
                                        .spawn(Node {
                                            flex_direction: FlexDirection::Column,
                                            row_gap: Val::Vh(SLOT_GAP_VH),
                                            ..default()
                                        })
                                        .with_children(|grid| {
                                            for row in 0..GRID_ROWS {
                                                grid.spawn(Node {
                                                    flex_direction: FlexDirection::Row,
                                                    column_gap: Val::Vh(SLOT_GAP_VH),
                                                    ..default()
                                                })
                                                .with_children(|row_node| {
                                                    for col in 0..GRID_COLS {
                                                        let slot_ref = InventorySlotRef {
                                                            panel: InventoryPanel::Chest,
                                                            index: row * GRID_COLS + col,
                                                        };
                                                        row_node
                                                            .spawn((
                                                                Node {
                                                                    width: Val::Vh(SLOT_VH),
                                                                    height: Val::Vh(SLOT_VH),
                                                                    border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                                                                    justify_content: JustifyContent::Center,
                                                                    align_items: AlignItems::Center,
                                                                    overflow: Overflow::clip(),
                                                                    ..default()
                                                                },
                                                                BackgroundColor(slot_bg),
                                                                BorderColor::all(slot_border),
                                                                Interaction::default(),
                                                                slot_ref,
                                                            ))
                                                            .with_children(|slot| {
                                                                slot.spawn((
                                                                    Node {
                                                                        width: Val::Percent(100.0),
                                                                        height: Val::Percent(100.0),
                                                                        ..default()
                                                                    },
                                                                    ImageNode::default(),
                                                                    Visibility::Hidden,
                                                                    SlotIcon,
                                                                    slot_ref,
                                                                ));
                                                                slot.spawn((
                                                                    Text::new(""),
                                                                    TextFont {
                                                                        font: count_font.clone(),
                                                                        font_size: 8.0,
                                                                        ..default()
                                                                    },
                                                                    TextColor(Color::WHITE),
                                                                    Node {
                                                                        position_type: PositionType::Absolute,
                                                                        bottom: Val::Px(1.0),
                                                                        right: Val::Px(1.0),
                                                                        ..default()
                                                                    },
                                                                    Visibility::Hidden,
                                                                    StackCount,
                                                                    slot_ref,
                                                                ));
                                                            });
                                                    }
                                                });
                                            }
                                        });
                                });

                            // --- Player panel (always visible when overlay is open) ---
                            content
                                .spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Vh(SLOT_GAP_VH),
                                    ..default()
                                })
                                .with_children(|panel| {
                                    panel.spawn((
                                        Text::new("Player"),
                                        TextFont { font_size: 13.0, ..default() },
                                        TextColor(text_col),
                                        Node {
                                            margin: UiRect::bottom(Val::Vh(1.2)),
                                            ..default()
                                        },
                                    ));
                                    panel
                                        .spawn(Node {
                                            flex_direction: FlexDirection::Column,
                                            row_gap: Val::Vh(SLOT_GAP_VH),
                                            ..default()
                                        })
                                        .with_children(|grid| {
                                            for row in 0..GRID_ROWS {
                                                grid.spawn(Node {
                                                    flex_direction: FlexDirection::Row,
                                                    column_gap: Val::Vh(SLOT_GAP_VH),
                                                    ..default()
                                                })
                                                .with_children(|row_node| {
                                                    for col in 0..GRID_COLS {
                                                        let slot_ref = InventorySlotRef {
                                                            panel: InventoryPanel::Player,
                                                            index: row * GRID_COLS + col,
                                                        };
                                                        row_node
                                                            .spawn((
                                                                Node {
                                                                    width: Val::Vh(SLOT_VH),
                                                                    height: Val::Vh(SLOT_VH),
                                                                    border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                                                                    justify_content: JustifyContent::Center,
                                                                    align_items: AlignItems::Center,
                                                                    overflow: Overflow::clip(),
                                                                    ..default()
                                                                },
                                                                BackgroundColor(slot_bg),
                                                                BorderColor::all(slot_border),
                                                                Interaction::default(),
                                                                slot_ref,
                                                            ))
                                                            .with_children(|slot| {
                                                                slot.spawn((
                                                                    Node {
                                                                        width: Val::Percent(100.0),
                                                                        height: Val::Percent(100.0),
                                                                        ..default()
                                                                    },
                                                                    ImageNode::default(),
                                                                    Visibility::Hidden,
                                                                    SlotIcon,
                                                                    slot_ref,
                                                                ));
                                                                slot.spawn((
                                                                    Text::new(""),
                                                                    TextFont {
                                                                        font: count_font.clone(),
                                                                        font_size: 8.0,
                                                                        ..default()
                                                                    },
                                                                    TextColor(Color::WHITE),
                                                                    Node {
                                                                        position_type: PositionType::Absolute,
                                                                        bottom: Val::Px(1.0),
                                                                        right: Val::Px(1.0),
                                                                        ..default()
                                                                    },
                                                                    Visibility::Hidden,
                                                                    StackCount,
                                                                    slot_ref,
                                                                ));
                                                            });
                                                    }
                                                });
                                            }
                                        });
                                });
                        });
                });
        });

    // Floating cursor icon rendered above all UI (z=20). No Interaction component
    // so clicks pass through to the slots beneath.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Vh(SLOT_VH),
            height: Val::Vh(SLOT_VH),
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ImageNode::default(),
        Visibility::Hidden,
        GlobalZIndex(20),
        HeldItemCursor,
    ));
}

/// Spawns the always-visible hotbar anchored to the bottom-centre of the screen.
///
/// Each slot is a full inventory slot (player inventory indices [`HOTBAR_START`]..):
/// it carries [`InventorySlotRef`], [`Interaction`] for drag-drop, and a [`SlotIcon`]
/// child so the existing icon-sync system displays held items automatically.
fn spawn_hotbar(mut commands: Commands, asset_server: Res<AssetServer>) {
    let count_font: Handle<Font> = asset_server.load("fonts/RobotoMono-Bold.ttf");
    let slot_bg = Color::srgb(0.08, 0.07, 0.06);
    let slot_border = Color::srgb(0.32, 0.27, 0.22);
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.90);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Vh(HOTBAR_BOTTOM_MARGIN_VH),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Vh(SLOT_GAP_VH),
                    padding: UiRect::all(Val::Vh(HOTBAR_PADDING_VH)),
                    ..default()
                },
                BackgroundColor(panel_bg),
            ))
            .with_children(|hotbar| {
                for i in 0..HOTBAR_SLOTS {
                    let slot_ref = InventorySlotRef {
                        panel: InventoryPanel::Player,
                        index: HOTBAR_START + i,
                    };
                    // Column wrapper: slot on top, ammo sub-view floating below.
                    hotbar
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Vh(AMMO_SUBVIEW_GAP_VH),
                            ..default()
                        })
                        .with_children(|wrapper| {
                            // --- Slot ---
                            wrapper
                                .spawn((
                                    Node {
                                        width: Val::Vh(HOTBAR_SLOT_VH),
                                        height: Val::Vh(HOTBAR_SLOT_VH),
                                        border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        overflow: Overflow::clip(),
                                        ..default()
                                    },
                                    BackgroundColor(slot_bg),
                                    BorderColor::all(slot_border),
                                    Interaction::default(),
                                    HotbarSlot,
                                    slot_ref,
                                ))
                                .with_children(|slot| {
                                    slot.spawn((
                                        Node {
                                            width: Val::Percent(100.0),
                                            height: Val::Percent(100.0),
                                            ..default()
                                        },
                                        ImageNode::default(),
                                        Visibility::Hidden,
                                        SlotIcon,
                                        slot_ref,
                                    ));
                                    slot.spawn((
                                        Text::new(""),
                                        TextFont {
                                            font: count_font.clone(),
                                            font_size: 11.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        Node {
                                            position_type: PositionType::Absolute,
                                            bottom: Val::Px(1.0),
                                            right: Val::Px(1.0),
                                            ..default()
                                        },
                                        Visibility::Hidden,
                                        StackCount,
                                        slot_ref,
                                    ));
                                });

                            // --- Ammo sub-view (centered below slot) ---
                            wrapper
                                .spawn((
                                    Node {
                                        width: Val::Vh(HOTBAR_SLOT_VH),
                                        height: Val::Vh(AMMO_SUBVIEW_VH),
                                        flex_direction: FlexDirection::Row,
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::Center,
                                        column_gap: Val::Px(3.0),
                                        padding: UiRect::horizontal(Val::Px(3.0)),
                                        border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                                        ..default()
                                    },
                                    BackgroundColor(slot_bg),
                                    BorderColor::all(slot_border),
                                    Visibility::Hidden,
                                    AmmoSubView,
                                    slot_ref,
                                ))
                                .with_children(|sub| {
                                    sub.spawn((
                                        Node {
                                            height: Val::Percent(80.0),
                                            aspect_ratio: Some(1.0),
                                            ..default()
                                        },
                                        ImageNode::default(),
                                        AmmoIcon,
                                        slot_ref,
                                    ));
                                    sub.spawn((
                                        Text::new(""),
                                        TextFont {
                                            font: count_font.clone(),
                                            font_size: 9.0,
                                            ..default()
                                        },
                                        TextColor(Color::WHITE),
                                        AmmoCount,
                                        slot_ref,
                                    ));
                                });
                        });
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Opens the player-only inventory when I (or gamepad Y/North) is pressed.
///
/// - Playing → Inventory.
/// - Inventory (no active chest) → Playing (also returns any held item to the player).
/// - Paused / Settings → Inventory (switches the visible menu screen without closing).
fn toggle_inventory(
    action_input: Res<ActionInput>,
    mut input_mode: ResMut<InputMode>,
    active_chest: Res<ActiveChest>,
    mut held: ResMut<HeldItem>,
    mut held_source: ResMut<HeldItemSource>,
    mut player_inv: Query<&mut Inventory, (With<PlayerControlled>, Without<Chest>)>,
    mut chest_inv: Query<&mut Inventory, (With<Chest>, Without<PlayerControlled>)>,
) {
    if !action_input.just_pressed(GameAction::OpenInventory) {
        return;
    }
    match *input_mode {
        InputMode::Playing => *input_mode = InputMode::Inventory,
        InputMode::Inventory if active_chest.0.is_none() => {
            // Return any held item to its source slot, or to the first empty slot.
            if held.0.is_some() {
                if let Some(source) = held_source.0.take() {
                    swap_held_with_slot(
                        source,
                        &mut player_inv,
                        &active_chest,
                        &mut chest_inv,
                        &mut held,
                        None,
                    );
                } else if let Some(stack) = held.0.take() {
                    if let Ok(mut inv) = player_inv.single_mut() {
                        if !inv.insert_first_empty(stack.clone()) {
                            warn!("Inventory full — dropped '{}' on close.", stack.id);
                        }
                    }
                }
            }
            *input_mode = InputMode::Playing;
        }
        InputMode::Paused | InputMode::Settings => *input_mode = InputMode::Inventory,
        _ => {}
    }
}

/// Closes the inventory on Cancel (Escape / gamepad B) or X-button click.
///
/// If an item is currently held, Cancel returns it to its source slot (recorded in
/// [`HeldItemSource`]) rather than closing the inventory, giving the player a chance
/// to cancel a swap mid-navigation. A second Cancel (with nothing held) closes normally.
fn close_inventory(
    action_input: Res<ActionInput>,
    close_btn: Query<&Interaction, With<CloseInventoryButton>>,
    mut input_mode: ResMut<InputMode>,
    mut active_chest: ResMut<ActiveChest>,
    mut held: ResMut<HeldItem>,
    mut held_source: ResMut<HeldItemSource>,
    mut player_inv: Query<&mut Inventory, (With<PlayerControlled>, Without<Chest>)>,
    mut chest_inv: Query<&mut Inventory, (With<Chest>, Without<PlayerControlled>)>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }

    let escape = action_input.just_pressed(GameAction::Cancel);
    let x_clicked = close_btn.iter().any(|i| *i == Interaction::Pressed);

    if !escape && !x_clicked {
        return;
    }

    // If holding an item, return it to the source slot rather than closing.
    if held.0.is_some() {
        if let Some(source) = held_source.0.take() {
            swap_held_with_slot(
                source,
                &mut player_inv,
                &active_chest,
                &mut chest_inv,
                &mut held,
                None,
            );
        } else {
            // No source tracked (edge case); fall back to first empty slot.
            if let Some(stack) = held.0.take() {
                if let Ok(mut inv) = player_inv.single_mut() {
                    if !inv.insert_first_empty(stack.clone()) {
                        warn!("Inventory full — dropped '{}' on cancel.", stack.id);
                    }
                }
            }
        }
        return;
    }

    active_chest.0 = None;
    *input_mode = InputMode::Playing;
}

/// Shows or hides the dim overlay when [`InputMode`] changes.
fn sync_overlay_visibility(
    input_mode: Res<InputMode>,
    mut overlay: Query<&mut Visibility, With<InventoryOverlay>>,
) {
    if !input_mode.is_changed() {
        return;
    }
    for mut vis in &mut overlay {
        *vis = if *input_mode == InputMode::Inventory {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Shows or hides the chest panel when [`ActiveChest`] changes.
///
/// Uses `Display::None` (not just `Visibility::Hidden`) so the panel is fully
/// removed from the flex layout when no chest is active, keeping the player
/// panel centred on its own.
fn sync_chest_panel_visibility(
    active_chest: Res<ActiveChest>,
    mut panel: Query<(&mut Visibility, &mut Node), With<ChestPanel>>,
) {
    if !active_chest.is_changed() {
        return;
    }
    for (mut vis, mut node) in &mut panel {
        if active_chest.0.is_some() {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
}

/// Swaps [`HeldItem`] with the contents of `slot_ref` in the appropriate inventory.
///
/// Used by both mouse clicks ([`handle_slot_click`]) and keyboard/gamepad confirm
/// ([`confirm_inventory_slot`]) to avoid duplicating swap logic.
fn swap_held_with_slot(
    slot_ref: InventorySlotRef,
    player_inv: &mut Query<&mut Inventory, (With<PlayerControlled>, Without<Chest>)>,
    active_chest: &ActiveChest,
    chest_inv: &mut Query<&mut Inventory, (With<Chest>, Without<PlayerControlled>)>,
    held: &mut HeldItem,
    item_library: Option<&ItemLibrary>,
) {
    // Block non-equippable items (e.g. arrows) from being placed into hotbar slots.
    if slot_ref.index >= HOTBAR_START {
        if let (Some(library), Some(stack)) = (item_library, held.0.as_ref()) {
            if library.def(&stack.id).map_or(false, |d| !d.equippable) {
                return;
            }
        }
    }

    let old_slot = match slot_ref.panel {
        InventoryPanel::Player => {
            let Ok(mut inv) = player_inv.single_mut() else { return };
            let old = inv.take(slot_ref.index);
            inv.put(slot_ref.index, held.0.take()).ok();
            old
        }
        InventoryPanel::Chest => {
            let Some(chest_entity) = active_chest.0 else { return };
            let Ok(mut inv) = chest_inv.get_mut(chest_entity) else { return };
            let old = inv.take(slot_ref.index);
            inv.put(slot_ref.index, held.0.take()).ok();
            old
        }
    };

    held.0 = old_slot;
}

/// Handles slot clicks — swaps [`HeldItem`] with the clicked slot's contents.
///
/// Only active while [`InputMode::Inventory`] is set, preventing accidental hotbar
/// drain when the player clicks during normal gameplay.
///
/// Every click is a full swap: the held item goes into the slot and the slot's
/// previous contents become the new held item. Covers all four combinations:
/// pick up, place, swap two different items, and no-op (both sides empty).
fn handle_slot_click(
    input_mode: Res<InputMode>,
    slot_query: Query<(&Interaction, &InventorySlotRef), Changed<Interaction>>,
    mut player_inv: Query<&mut Inventory, (With<PlayerControlled>, Without<Chest>)>,
    active_chest: Res<ActiveChest>,
    mut chest_inv: Query<&mut Inventory, (With<Chest>, Without<PlayerControlled>)>,
    mut held: ResMut<HeldItem>,
    mut held_source: ResMut<HeldItemSource>,
    item_library: Option<Res<ItemLibrary>>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }

    for (interaction, slot_ref) in &slot_query {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let was_empty = held.0.is_none();
        swap_held_with_slot(
            *slot_ref,
            &mut player_inv,
            &active_chest,
            &mut chest_inv,
            &mut held,
            item_library.as_deref(),
        );
        if was_empty && held.0.is_some() {
            held_source.0 = Some(*slot_ref);
        } else if !was_empty {
            held_source.0 = if held.0.is_some() { Some(*slot_ref) } else { None };
        }
    }
}

/// Moves focus between inventory slots using WASD / D-pad (or left stick initial press).
///
/// Only active in [`InputMode::Inventory`]. Moving left from the Player panel's first column
/// (or right from the Chest panel's last column) switches panels when a chest is open.
/// Rows wrap vertically; columns wrap within a panel when only one panel is visible.
fn navigate_inventory(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    active_chest: Res<ActiveChest>,
    mut focus: ResMut<InventoryFocusSlot>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }

    let up    = action_input.just_pressed(GameAction::MoveNorth);
    let down  = action_input.just_pressed(GameAction::MoveSouth);
    let left  = action_input.just_pressed(GameAction::MoveWest);
    let right = action_input.just_pressed(GameAction::MoveEast);

    if !up && !down && !left && !right {
        return;
    }

    let chest_open = active_chest.0.is_some();

    if focus.in_hotbar {
        // Navigation while the hotbar row is focused.
        if up {
            // Return to the bottom row of the player grid.
            focus.in_hotbar = false;
            focus.panel = InventoryPanel::Player;
            focus.row = GRID_ROWS - 1;
        }
        // Down from hotbar: no row below, ignore.
        if left  { focus.col = (focus.col + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS; }
        if right { focus.col = (focus.col + 1) % HOTBAR_SLOTS; }
    } else {
        // Navigation within the inventory grid.
        if up {
            focus.row = (focus.row + GRID_ROWS - 1) % GRID_ROWS;
        }
        if down {
            if focus.panel == InventoryPanel::Player && focus.row == GRID_ROWS - 1 {
                // Enter the hotbar from the bottom row of the player grid.
                focus.in_hotbar = true;
            } else {
                focus.row = (focus.row + 1) % GRID_ROWS;
            }
        }
        if left {
            if focus.col == 0 && chest_open && focus.panel == InventoryPanel::Player {
                // Wrap left from Player panel into Chest panel.
                focus.panel = InventoryPanel::Chest;
                focus.col = GRID_COLS - 1;
            } else {
                focus.col = (focus.col + GRID_COLS - 1) % GRID_COLS;
            }
        }
        if right {
            if focus.col == GRID_COLS - 1 && chest_open && focus.panel == InventoryPanel::Chest {
                // Wrap right from Chest panel into Player panel.
                focus.panel = InventoryPanel::Player;
                focus.col = 0;
            } else {
                focus.col = (focus.col + 1) % GRID_COLS;
            }
        }
    }
}

/// Activates the focused inventory slot when Confirm (Space / gamepad A) is pressed.
///
/// Performs the same held-item swap as a mouse click on the focused slot.
/// Records [`HeldItemSource`] when picking up and clears it when placing.
/// Only fires in [`InputMode::Inventory`].
fn confirm_inventory_slot(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    focus: Res<InventoryFocusSlot>,
    mut player_inv: Query<&mut Inventory, (With<PlayerControlled>, Without<Chest>)>,
    active_chest: Res<ActiveChest>,
    mut chest_inv: Query<&mut Inventory, (With<Chest>, Without<PlayerControlled>)>,
    mut held: ResMut<HeldItem>,
    mut held_source: ResMut<HeldItemSource>,
    item_library: Option<Res<ItemLibrary>>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }
    if !action_input.just_pressed(GameAction::Confirm) {
        return;
    }

    let slot_ref = if focus.in_hotbar {
        InventorySlotRef { panel: InventoryPanel::Player, index: HOTBAR_START + focus.col }
    } else {
        InventorySlotRef { panel: focus.panel, index: focus.row * GRID_COLS + focus.col }
    };

    let was_empty = held.0.is_none();
    swap_held_with_slot(
        slot_ref,
        &mut player_inv,
        &active_chest,
        &mut chest_inv,
        &mut held,
        item_library.as_deref(),
    );

    if was_empty && held.0.is_some() {
        // Picked up: record the source slot.
        held_source.0 = Some(slot_ref);
    } else if !was_empty && held.0.is_none() {
        // Placed or swapped to empty: clear source.
        held_source.0 = None;
    } else if !was_empty && held.0.is_some() {
        // Swapped two items: new held item still came from this slot.
        held_source.0 = Some(slot_ref);
    }
}

/// Highlights the focused inventory slot with a golden border while [`InputMode::Inventory`] is active.
///
/// Manages borders for both inventory grid slots and hotbar slots so the two sets
/// are always consistent. When inventory closes, hotbar borders are restored
/// (equipped slot = white, others = normal) so [`sync_hotbar_borders`] does not need
/// to run again.
fn sync_inventory_focus_highlight(
    input_mode: Res<InputMode>,
    focus: Res<InventoryFocusSlot>,
    equipped: Res<EquippedHotbarSlot>,
    mut grid_slots: Query<(&InventorySlotRef, &mut BorderColor), Without<HotbarSlot>>,
    mut hotbar_slots: Query<(&InventorySlotRef, &mut BorderColor), With<HotbarSlot>>,
) {
    if !input_mode.is_changed() && !focus.is_changed() && !equipped.is_changed() {
        return;
    }

    let is_inventory = *input_mode == InputMode::Inventory;
    let focused_border = BorderColor::all(Color::srgb(0.85, 0.70, 0.20));
    let normal_border  = BorderColor::all(Color::srgb(0.32, 0.27, 0.22));
    let equipped_border = BorderColor::all(Color::WHITE);

    // Inventory grid slots.
    let grid_focused_idx = focus.row * GRID_COLS + focus.col;
    for (slot_ref, mut border) in &mut grid_slots {
        let is_focused = is_inventory
            && !focus.in_hotbar
            && slot_ref.panel == focus.panel
            && slot_ref.index == grid_focused_idx;
        *border = if is_focused { focused_border } else { normal_border };
    }

    // Hotbar slots — always sync so closing inventory restores the equipped highlight.
    for (slot_ref, mut border) in &mut hotbar_slots {
        let hotbar_idx = slot_ref.index.saturating_sub(HOTBAR_START);
        *border = if is_inventory && focus.in_hotbar && focus.col == hotbar_idx {
            focused_border
        } else if equipped.0 == Some(hotbar_idx) {
            equipped_border
        } else {
            normal_border
        };
    }
}

/// Resets focus to the Player panel when the chest closes, preventing a stale focus
/// pointing at a hidden panel.
fn reset_focus_on_chest_close(
    active_chest: Res<ActiveChest>,
    mut focus: ResMut<InventoryFocusSlot>,
) {
    if active_chest.is_changed() && active_chest.0.is_none() {
        focus.panel = InventoryPanel::Player;
    }
}

/// Updates all slot icon nodes to reflect current [`Inventory`] data.
///
/// Runs every frame. Occupied slots become visible with the correct item texture;
/// empty slots are hidden.
fn sync_slot_icons(
    item_library: Option<Res<ItemLibrary>>,
    player_inv: Query<&Inventory, (With<PlayerControlled>, Without<Chest>)>,
    active_chest: Res<ActiveChest>,
    chest_inv: Query<&Inventory, (With<Chest>, Without<PlayerControlled>)>,
    mut icons: Query<(&InventorySlotRef, &mut ImageNode, &mut Visibility), With<SlotIcon>>,
) {
    let Some(library) = item_library else { return };

    let player_inventory = player_inv.single().ok();
    let chest_inventory = active_chest.0.and_then(|e| chest_inv.get(e).ok());

    for (slot_ref, mut img, mut vis) in &mut icons {
        let inventory = match slot_ref.panel {
            InventoryPanel::Player => player_inventory,
            InventoryPanel::Chest => chest_inventory,
        };

        match inventory
            .and_then(|inv| inv.get(slot_ref.index))
            .and_then(|s| library.icon(&s.id))
        {
            Some(handle) => {
                img.image = handle.clone();
                *vis = Visibility::Inherited;
            }
            None => {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Moves the floating cursor icon to track the held item's position.
///
/// - Mouse: follows the cursor position directly.
/// - Gamepad: snaps to the top-left of the currently focused slot so the icon
///   visually occupies that slot while the player navigates to the drop target.
///
/// The cursor node has no [`Interaction`] so it never blocks clicks on slots beneath it.
fn update_held_cursor(
    windows: Query<&Window, With<PrimaryWindow>>,
    held: Res<HeldItem>,
    item_library: Option<Res<ItemLibrary>>,
    input_source: Res<InputSource>,
    input_mode: Res<InputMode>,
    focus: Res<InventoryFocusSlot>,
    slot_q: Query<(&InventorySlotRef, &UiGlobalTransform, &ComputedNode)>,
    mut cursor_q: Query<(&mut Node, &mut Visibility, &mut ImageNode), With<HeldItemCursor>>,
) {
    let Ok((mut node, mut vis, mut img)) = cursor_q.single_mut() else { return };

    let Some(stack) = &held.0 else {
        *vis = Visibility::Hidden;
        return;
    };

    match *input_source {
        InputSource::Gamepad if *input_mode == InputMode::Inventory => {
            // Find the focused slot and snap the icon to its top-left corner.
            let target_ref = if focus.in_hotbar {
                InventorySlotRef { panel: InventoryPanel::Player, index: HOTBAR_START + focus.col }
            } else {
                InventorySlotRef { panel: focus.panel, index: focus.row * GRID_COLS + focus.col }
            };

            let slot = slot_q.iter().find(|(sr, _, _)| **sr == target_ref);
            if let Some((_, ui_tf, computed)) = slot {
                let top_left_physical = ui_tf.affine().translation;
                let inv_scale = computed.inverse_scale_factor();
                let top_left = top_left_physical * inv_scale;
                node.left = Val::Px(top_left.x);
                node.top  = Val::Px(top_left.y);
            }
        }
        _ => {
            // Mouse: follow the cursor.
            if let Ok(window) = windows.single() {
                if let Some(pos) = window.cursor_position() {
                    node.left = Val::Px(pos.x);
                    node.top  = Val::Px(pos.y);
                }
            }
        }
    }

    if let Some(library) = item_library {
        if let Some(handle) = library.icon(&stack.id) {
            img.image = handle.clone();
        }
    }

    *vis = Visibility::Visible;
}

/// Selects a hotbar slot when the player presses 1–4 or Q/E (keyboard) or LB/RB (gamepad).
///
/// Keys 1–4 select a slot directly. Q/LB cycles to the previous slot; E/RB cycles to
/// the next. The selected slot is stored in [`EquippedHotbarSlot`] and used by combat
/// and item-use systems to determine the active item.
fn select_hotbar_slot(
    action_input: Res<ActionInput>,
    input_mode: Res<InputMode>,
    mut equipped: ResMut<EquippedHotbarSlot>,
) {
    if *input_mode != InputMode::Playing {
        return;
    }

    // Direct selection via keys 1–4.
    let direct = if action_input.just_pressed(GameAction::HotbarSlot1) {
        Some(0)
    } else if action_input.just_pressed(GameAction::HotbarSlot2) {
        Some(1)
    } else if action_input.just_pressed(GameAction::HotbarSlot3) {
        Some(2)
    } else if action_input.just_pressed(GameAction::HotbarSlot4) {
        Some(3)
    } else {
        None
    };

    if let Some(s) = direct {
        equipped.0 = Some(s);
        return;
    }

    // Cycle selection via Q/LB (prev) and E/RB (next).
    let prev = action_input.just_pressed(GameAction::HotbarPrev);
    let next = action_input.just_pressed(GameAction::HotbarNext);

    if prev || next {
        let current = equipped.0.unwrap_or(0);
        equipped.0 = Some(if next {
            (current + 1) % HOTBAR_SLOTS
        } else {
            (current + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS
        });
    }
}

/// Outlines the selected hotbar slot with a white border; all others use the normal border.
///
/// Skips while [`InputMode::Inventory`] is active — [`sync_inventory_focus_highlight`]
/// owns hotbar borders during that time to show the focus cursor. Runs only when
/// [`EquippedHotbarSlot`] changes to keep UI updates minimal.
fn sync_hotbar_borders(
    equipped: Res<EquippedHotbarSlot>,
    input_mode: Res<InputMode>,
    mut slots: Query<(&InventorySlotRef, &mut BorderColor), With<HotbarSlot>>,
) {
    if *input_mode == InputMode::Inventory {
        return;
    }
    if !equipped.is_changed() {
        return;
    }

    let selected_border = BorderColor::all(Color::WHITE);
    let normal_border = BorderColor::all(Color::srgb(0.32, 0.27, 0.22));

    for (slot_ref, mut border) in &mut slots {
        let hotbar_idx = slot_ref.index.saturating_sub(HOTBAR_START);
        *border = if equipped.0 == Some(hotbar_idx) {
            selected_border
        } else {
            normal_border
        };
    }
}

/// Updates the stack-count text overlay on each inventory slot.
///
/// Shows the count in the bottom-right corner only for stackable items
/// (`max_stack > 1`). Hidden when the slot is empty or holds a non-stackable item.
fn sync_stack_counts(
    item_library: Option<Res<ItemLibrary>>,
    player_inv: Query<&Inventory, (With<PlayerControlled>, Without<Chest>)>,
    active_chest: Res<ActiveChest>,
    chest_inv: Query<&Inventory, (With<Chest>, Without<PlayerControlled>)>,
    mut counts: Query<(&InventorySlotRef, &mut Text, &mut Visibility), With<StackCount>>,
) {
    let Some(library) = item_library else { return };

    let player_inventory = player_inv.single().ok();
    let chest_inventory = active_chest.0.and_then(|e| chest_inv.get(e).ok());

    for (slot_ref, mut text, mut vis) in &mut counts {
        let inventory = match slot_ref.panel {
            InventoryPanel::Player => player_inventory,
            InventoryPanel::Chest => chest_inventory,
        };

        let count_label = inventory
            .and_then(|inv| inv.get(slot_ref.index))
            .and_then(|stack| {
                let def = library.def(&stack.id)?;
                if def.max_stack > 1 { Some(stack.count.to_string()) } else { None }
            });

        match count_label {
            Some(label) => {
                **text = label;
                *vis = Visibility::Inherited;
            }
            None => {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Shows an ammo sub-view in the bottom-left corner of the equipped hotbar slot.
///
/// The sub-view displays the ammo item's icon and total count whenever the equipped
/// item has a non-`None` `ammo_id` and the player holds at least one of that ammo.
/// Hidden on all other slots and when ammo is exhausted.
fn sync_ammo_subview(
    equipped: Res<EquippedHotbarSlot>,
    item_library: Option<Res<ItemLibrary>>,
    player_inv: Query<&Inventory, With<PlayerControlled>>,
    mut subviews: Query<(&InventorySlotRef, &mut Visibility), With<AmmoSubView>>,
    mut ammo_icons: Query<(&InventorySlotRef, &mut ImageNode), With<AmmoIcon>>,
    mut ammo_counts: Query<(&InventorySlotRef, &mut Text), With<AmmoCount>>,
) {
    let Some(library) = item_library else { return };
    let Ok(inventory) = player_inv.single() else { return };

    for (slot_ref, mut vis) in &mut subviews {
        let hotbar_idx = slot_ref.index.saturating_sub(HOTBAR_START);

        // Only show on the currently equipped slot.
        let Some(equipped_idx) = equipped.0 else {
            *vis = Visibility::Hidden;
            continue;
        };
        if hotbar_idx != equipped_idx {
            *vis = Visibility::Hidden;
            continue;
        }

        // Resolve the ammo_id for the equipped item.
        let ammo_id = inventory
            .get(slot_ref.index)
            .and_then(|stack| library.def(&stack.id))
            .and_then(|def| def.ammo_id.clone());

        let Some(ammo_id) = ammo_id else {
            *vis = Visibility::Hidden;
            continue;
        };

        // Sum all ammo across the inventory.
        let total: u32 = (0..inventory.len())
            .filter_map(|i| inventory.get(i))
            .filter(|s| s.id == ammo_id)
            .map(|s| s.count)
            .sum();

        if total == 0 {
            *vis = Visibility::Hidden;
            continue;
        }

        *vis = Visibility::Inherited;

        // Update icon and count for this slot.
        if let Some(icon_handle) = library.icon(&ammo_id) {
            for (icon_ref, mut img) in &mut ammo_icons {
                if icon_ref.index == slot_ref.index {
                    img.image = icon_handle.clone();
                }
            }
        }
        for (count_ref, mut text) in &mut ammo_counts {
            if count_ref.index == slot_ref.index {
                **text = total.to_string();
            }
        }
    }
}
