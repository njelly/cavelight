use bevy::prelude::*;

/// Inventory grid dimensions.
const GRID_COLS: usize = 4;
const GRID_ROWS: usize = 4;

/// Hotbar slot count — matches the inventory column width.
const HOTBAR_SLOTS: usize = 4;

// All sizes below are expressed as a percentage of the viewport height (vh units) so the
// layout scales proportionally with the window, like Unity's "Scale With Screen Size".

/// Slot size as a percentage of viewport height — shared by both the inventory grid and hotbar.
const SLOT_VH: f32 = 10.0;
const SLOT_GAP_VH: f32 = 2.4;
const GRID_PADDING_VH: f32 = 1.2;

const HOTBAR_SLOT_VH: f32 = SLOT_VH;
const HOTBAR_PADDING_VH: f32 = 0.8;
const HOTBAR_BOTTOM_MARGIN_VH: f32 = 1.5;

/// Total vh the hotbar takes from the bottom: slot + top/bottom padding + bottom margin.
const HOTBAR_HEIGHT_VH: f32 = HOTBAR_SLOT_VH + 2.0 * HOTBAR_PADDING_VH + HOTBAR_BOTTOM_MARGIN_VH;

/// Border stays in pixels — a 2 px line looks fine at any resolution.
const SLOT_BORDER_PX: f32 = 2.0;

/// Whether the inventory panel is currently visible. Toggle with I.
#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
pub struct InventoryOpen(pub bool);

/// Marker for the inventory overlay root node.
///
/// Setting [`Visibility::Hidden`] on this entity hides both the dim background
/// and the grid panel in a single operation.
#[derive(Component)]
struct InventoryOverlay;

/// Marker for an inventory grid slot (the 4×4 panel).
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct InventorySlot;

/// Marker for a hotbar slot (always-visible quick-access bar).
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct HotbarSlot;

/// Spawns the inventory UI and hotbar; wires up the I-key toggle.
pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryOpen>()
            .register_type::<InventoryOpen>()
            .register_type::<InventorySlot>()
            .register_type::<HotbarSlot>()
            .add_systems(Startup, spawn_inventory_ui)
            .add_systems(Update, (toggle_inventory, sync_inventory_visibility).chain());
    }
}

/// Spawns the dim overlay + inventory grid (hidden by default) and the always-visible hotbar.
fn spawn_inventory_ui(mut commands: Commands) {
    let slot_bg = Color::srgb(0.08, 0.07, 0.06);
    let slot_border_col = Color::srgb(0.32, 0.27, 0.22);
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.90);
    let dim = Color::srgba(0.0, 0.0, 0.0, 0.65);

    // Overlay root — hidden by default. Both the dim layer and the grid panel are
    // children, so toggling Visibility on this node shows or hides everything at once.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Vw(100.0),
                height: Val::Vh(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                // Push the grid upward so it sits above the hotbar.
                padding: UiRect::bottom(Val::Vh(HOTBAR_HEIGHT_VH)),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(5),
            BackgroundColor(dim),
            InventoryOverlay,
        ))
        .with_children(|root| {
            // Grid panel.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Vh(SLOT_GAP_VH),
                    padding: UiRect::all(Val::Vh(GRID_PADDING_VH)),
                    ..default()
                },
                BackgroundColor(panel_bg),
            ))
            .with_children(|panel| {
                for _ in 0..GRID_ROWS {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Vh(SLOT_GAP_VH),
                            ..default()
                        })
                        .with_children(|row| {
                            for _ in 0..GRID_COLS {
                                row.spawn((
                                    Node {
                                        width: Val::Vh(SLOT_VH),
                                        height: Val::Vh(SLOT_VH),
                                        border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                                        ..default()
                                    },
                                    BackgroundColor(slot_bg),
                                    BorderColor::all(slot_border_col),
                                    InventorySlot,
                                ));
                            }
                        });
                }
            });
        });

    // Hotbar — always visible, sits above the overlay, anchored to the bottom center.
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
                for _ in 0..HOTBAR_SLOTS {
                    hotbar.spawn((
                        Node {
                            width: Val::Vh(HOTBAR_SLOT_VH),
                            height: Val::Vh(HOTBAR_SLOT_VH),
                            border: UiRect::all(Val::Px(SLOT_BORDER_PX)),
                            ..default()
                        },
                        BackgroundColor(slot_bg),
                        BorderColor::all(slot_border_col),
                        HotbarSlot,
                    ));
                }
            });
        });
}

/// Toggles [`InventoryOpen`] when the player presses I.
fn toggle_inventory(keys: Res<ButtonInput<KeyCode>>, mut open: ResMut<InventoryOpen>) {
    if keys.just_pressed(KeyCode::KeyI) {
        open.0 = !open.0;
    }
}

/// Propagates [`InventoryOpen`] state to the overlay's [`Visibility`] component.
///
/// Runs only when the resource actually changes to avoid unnecessary change detection noise.
fn sync_inventory_visibility(
    open: Res<InventoryOpen>,
    mut overlay_query: Query<&mut Visibility, With<InventoryOverlay>>,
) {
    if !open.is_changed() {
        return;
    }
    for mut vis in &mut overlay_query {
        *vis = if open.0 { Visibility::Visible } else { Visibility::Hidden };
    }
}
