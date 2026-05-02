use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::chest::Chest;
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
/// Total vh the hotbar occupies from the bottom edge of the screen.
///
/// Used by sibling modules (e.g. dialogue) to anchor panels directly above the hotbar.
pub const HOTBAR_HEIGHT_VH: f32 = HOTBAR_SLOT_VH + 2.0 * HOTBAR_PADDING_VH + HOTBAR_BOTTOM_MARGIN_VH;

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
#[derive(Component, Clone, Copy)]
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
                    sync_slot_icons,
                    update_held_cursor,
                    sync_overlay_visibility,
                    sync_chest_panel_visibility,
                    select_hotbar_slot,
                    sync_hotbar_borders,
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
fn spawn_inventory_ui(mut commands: Commands) {
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
fn spawn_hotbar(mut commands: Commands) {
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
                    hotbar
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
                        });
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Opens the player-only inventory when I is pressed during [`InputMode::Playing`].
/// Closes it if I is pressed again and no chest is active (use Escape or X otherwise).
fn toggle_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    mut input_mode: ResMut<InputMode>,
    active_chest: Res<ActiveChest>,
) {
    if !keys.just_pressed(KeyCode::KeyI) {
        return;
    }
    match *input_mode {
        InputMode::Playing => *input_mode = InputMode::Inventory,
        InputMode::Inventory if active_chest.0.is_none() => *input_mode = InputMode::Playing,
        _ => {}
    }
}

/// Closes the inventory on Escape or X-button click.
///
/// Any held item is returned to the player's first available slot. If the
/// inventory is full the item is dropped with a warning log.
fn close_inventory(
    keys: Res<ButtonInput<KeyCode>>,
    close_btn: Query<&Interaction, With<CloseInventoryButton>>,
    mut input_mode: ResMut<InputMode>,
    mut active_chest: ResMut<ActiveChest>,
    mut held: ResMut<HeldItem>,
    mut player_inv: Query<&mut Inventory, With<PlayerControlled>>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }

    let escape = keys.just_pressed(KeyCode::Escape);
    let x_clicked = close_btn.iter().any(|i| *i == Interaction::Pressed);

    if !escape && !x_clicked {
        return;
    }

    if let Some(stack) = held.0.take() {
        if let Ok(mut inv) = player_inv.single_mut() {
            if !inv.insert_first_empty(stack.clone()) {
                warn!("Inventory full — dropped '{}' on inventory close.", stack.id);
            }
        }
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
    item_library: Option<Res<ItemLibrary>>,
) {
    if *input_mode != InputMode::Inventory {
        return;
    }

    for (interaction, slot_ref) in &slot_query {
        if *interaction != Interaction::Pressed {
            continue;
        }

        // Block non-equippable items (e.g. arrows) from being placed into hotbar slots.
        if slot_ref.index >= HOTBAR_START {
            if let (Some(library), Some(stack)) = (item_library.as_ref(), held.0.as_ref()) {
                if library.def(&stack.id).map_or(false, |d| !d.equippable) {
                    continue;
                }
            }
        }

        let old_slot = match slot_ref.panel {
            InventoryPanel::Player => {
                let Ok(mut inv) = player_inv.single_mut() else { continue };
                let old = inv.take(slot_ref.index);
                inv.put(slot_ref.index, held.0.take()).ok();
                old
            }
            InventoryPanel::Chest => {
                let Some(chest_entity) = active_chest.0 else { continue };
                let Ok(mut inv) = chest_inv.get_mut(chest_entity) else { continue };
                let old = inv.take(slot_ref.index);
                inv.put(slot_ref.index, held.0.take()).ok();
                old
            }
        };

        held.0 = old_slot;
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

/// Moves the floating cursor icon to the mouse position and shows the held item texture.
///
/// The cursor node has no [`Interaction`] so it never blocks clicks on slots beneath it.
fn update_held_cursor(
    windows: Query<&Window, With<PrimaryWindow>>,
    held: Res<HeldItem>,
    item_library: Option<Res<ItemLibrary>>,
    mut cursor_q: Query<(&mut Node, &mut Visibility, &mut ImageNode), With<HeldItemCursor>>,
) {
    let Ok((mut node, mut vis, mut img)) = cursor_q.single_mut() else { return };

    let Some(stack) = &held.0 else {
        *vis = Visibility::Hidden;
        return;
    };

    if let Ok(window) = windows.single() {
        if let Some(pos) = window.cursor_position() {
            node.left = Val::Px(pos.x);
            node.top = Val::Px(pos.y);
        }
    }

    if let Some(library) = item_library {
        if let Some(handle) = library.icon(&stack.id) {
            img.image = handle.clone();
        }
    }

    *vis = Visibility::Visible;
}

/// Selects a hotbar slot when the player presses 1–4 during [`InputMode::Playing`].
///
/// The selected slot is stored in [`EquippedHotbarSlot`] and used by combat and
/// item-use systems to determine the active item. Pressing the same key again
/// keeps that slot selected (no toggle — use deselect logic when needed).
fn select_hotbar_slot(
    keys: Res<ButtonInput<KeyCode>>,
    input_mode: Res<InputMode>,
    mut equipped: ResMut<EquippedHotbarSlot>,
) {
    if *input_mode != InputMode::Playing {
        return;
    }

    let slot = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };

    if let Some(s) = slot {
        equipped.0 = Some(s);
    }
}

/// Outlines the selected hotbar slot with a white border; all others use the normal border.
///
/// Runs only when [`EquippedHotbarSlot`] changes to keep UI updates minimal.
fn sync_hotbar_borders(
    equipped: Res<EquippedHotbarSlot>,
    mut slots: Query<(&InventorySlotRef, &mut BorderColor), With<HotbarSlot>>,
) {
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
