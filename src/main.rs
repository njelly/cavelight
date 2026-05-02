#![doc = include_str!("../README.md")]

mod camera;
mod campfire;
mod chest;
mod dialogue;
mod door;
mod fps_counter;
mod grid_mover;
mod interaction;
mod interaction_reticle;
mod inventory;
mod item;
mod ladder;
mod level;
mod npc;
mod player_input;
mod signpost;
mod sprite_animation;

use bevy::prelude::*;
use avian2d::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_light_2d::prelude::*;
use camera::CameraPlugin;
use campfire::CampfirePlugin;
use chest::ChestPlugin;
use dialogue::DialoguePlugin;
use door::DoorPlugin;
use fps_counter::FpsCounterPlugin;
use grid_mover::{GridMover, GridMoverPlugin};
use interaction::InteractionPlugin;
use interaction_reticle::InteractionReticlePlugin;
use inventory::InventoryPlugin;
use item::{Inventory, ItemPlugin, ItemStack};
use ladder::LadderPlugin;
use level::{LevelPlugin, PlayerSpawnPoint};
use player_input::{Facing, PlayerControlled, PlayerInput, PlayerInputPlugin};
use npc::NpcPlugin;
use signpost::SignpostPlugin;
use sprite_animation::{SpriteAnimation, SpriteAnimationPlugin};

// One grid cell = 8x8 pixels
pub const GRID_SIZE: f32 = 8.0;

/// Tracks whether the egui world inspector panel is visible.
///
/// Defaults to `false` (hidden). Toggle with F2.
#[derive(Resource, Default)]
struct WorldInspectorOpen(bool);

/// Marks the lantern light carried by the player.
///
/// Spawned as a child of the player entity so it inherits the player's transform.
/// Query this component to adjust lantern brightness, color, or radius at runtime
/// (e.g. when the player picks up oil, enters a dark zone, etc.).
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct PlayerLantern;

fn main() {
    App::new()
        .add_plugins((
            // Bevy core plugins
            DefaultPlugins
                // Enable pixel-perfect sprites.
                // TODO: Is this disabling anti-aliasing? That's fine for now, but I don't fully understand this behavior.
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Cavelight".to_string(),
                        resolution: bevy::window::WindowResolution::new(1200, 600),
                        // Enable vsync and prevent screen tearing.
                        present_mode: bevy::window::PresentMode::Fifo,
                        ..default()
                    }),
                    ..default()
                }),

            // Third-party plugins
            (
                PhysicsPlugins::default().with_length_unit(GRID_SIZE),
                PhysicsDebugPlugin::default(),
                EguiPlugin::default(),
                WorldInspectorPlugin::new().run_if(|open: Res<WorldInspectorOpen>| open.0),
                Light2dPlugin,
            ),

            // Internal plugins
            // Note: Nesting of plugin tuples is required eventually, see:
            // https://bevy-cheatbook.github.io/programming/systems.html
            // | Your function can have a maximum of 16 total parameters.
            // | If you need more, group them into tuples to work around the limit.
            // | Tuples can contain up to 16 members but can be nested indefinitely.
            (
                LevelPlugin,
                CameraPlugin,
                CampfirePlugin,
                ChestPlugin,
                DialoguePlugin,
                DoorPlugin,
                FpsCounterPlugin,
                GridMoverPlugin,
                InteractionPlugin,
            ),
            (
                InventoryPlugin,
                ItemPlugin,
                LadderPlugin,
                NpcPlugin,
                PlayerInputPlugin,
                SignpostPlugin,
                SpriteAnimationPlugin,
                InteractionReticlePlugin,
            ),
        ))
        .register_type::<PlayerLantern>()
        .insert_gizmo_config(PhysicsGizmos::default(), GizmoConfig { enabled: false, ..default() })
        .init_resource::<WorldInspectorOpen>()
        .add_systems(Startup, spawn_player)
        .add_systems(Update, (toggle_physics_debug, toggle_world_inspector))
        .run();
}

/// Toggles the egui world inspector panel on/off with F2.
fn toggle_world_inspector(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<WorldInspectorOpen>,
) {
    if keys.just_pressed(KeyCode::F2) {
        open.0 ^= true;
    }
}

/// Toggles avian2d collision shape debug rendering on/off with F1.
fn toggle_physics_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut store: ResMut<GizmoConfigStore>,
) {
    if keys.just_pressed(KeyCode::F1) {
        store.config_mut::<PhysicsGizmos>().0.enabled ^= true;
    }
}

/// Spawns the player entity with a lantern child light and a starting inventory.
///
/// The player begins with a dagger in inventory slot 0. The lantern follows
/// movement automatically via transform propagation.
fn spawn_player(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<PlayerSpawnPoint>,
) {
    // 512x512 atlas divided into 8x8 tiles = 64 columns, 64 rows
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    let mut inventory = Inventory::new(16);
    inventory.put(0, Some(ItemStack::new("dagger", 1))).ok();

    commands
        .spawn((
            Sprite::from_atlas_image(
                asset_server.load("atlas_8x8.png"),
                TextureAtlas {
                    layout: layout_handle,
                    index: 0,
                },
            ),
            Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
            SpriteAnimation::with_name("player_idle", true),
            GridMover::new(GRID_SIZE),
            RigidBody::Kinematic,
            Collider::rectangle(GRID_SIZE, GRID_SIZE),
            PlayerControlled,
            PlayerInput::default(),
            Facing::default(),
            inventory,
        ))
        .with_children(|parent| {
            // Lantern light carried by the player as a child entity so it follows movement.
            // TODO: Tune radius and intensity down to ~30 / 1.5 for normal gameplay.
            parent.spawn((
                PlayerLantern,
                Transform::default(),
                PointLight2d {
                    color: Color::srgb(0.5, 0.7, 1.0),
                    intensity: 0.6,
                    radius: 30.0,
                    falloff: 2.0,
                    cast_shadows: true,
                },
            ));
        });
}
