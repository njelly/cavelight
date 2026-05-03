#![doc = include_str!("../README.md")]

mod aim;
mod arrow;
mod camera;
mod campfire;
mod chest;
mod damageable;
mod dialogue;
mod door;
mod entity;
mod fps_counter;
mod goap;
mod grid_mover;
mod input;
mod interaction;
mod interaction_reticle;
mod inventory;
mod item;
mod ladder;
mod level;
mod menu;
mod npc;
mod player_input;
mod save;
mod signpost;
mod skeleton;
mod spawner;
mod sprite_animation;
mod wander;

use std::collections::HashSet;

use bevy::asset::AssetId;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::text::FontAtlasSet;

use avian2d::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_light_2d::prelude::*;
use aim::AimPlugin;
use arrow::ArrowPlugin;
use camera::CameraPlugin;
use campfire::CampfirePlugin;
use chest::ChestPlugin;
use damageable::DamageablePlugin;
use dialogue::DialoguePlugin;
use door::DoorPlugin;
use entity::EntityPlugin;
use fps_counter::FpsCounterPlugin;
use grid_mover::{GridMover, GridMoverPlugin};
use input::InputPlugin;
use interaction::InteractionPlugin;
use interaction_reticle::InteractionReticlePlugin;
use inventory::InventoryPlugin;
use item::{Inventory, ItemPlugin, ItemStack};
use ladder::LadderPlugin;
use level::{LevelPlugin, PlayerSpawnPoint};
use menu::{MenuPlugin, WorldInspectorOpen};
use player_input::{Facing, PlayerControlled, PlayerInput, PlayerInputPlugin};
use npc::NpcPlugin;
use save::SavePlugin;
use signpost::SignpostPlugin;
use goap::GoapPlugin;
use skeleton::SkeletonPlugin;
use spawner::SpawnerPlugin;
use sprite_animation::{SpriteAnimation, SpriteAnimationPlugin};

// One grid cell = 8x8 pixels
pub const GRID_SIZE: f32 = 8.0;

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
                SavePlugin,
                LevelPlugin,
                CameraPlugin,
                CampfirePlugin,
                ChestPlugin,
                DamageablePlugin,
                DialoguePlugin,
                DoorPlugin,
                EntityPlugin,
                FpsCounterPlugin,
                GridMoverPlugin,
                InteractionPlugin,
            ),
            (
                AimPlugin,
                ArrowPlugin,
                GoapPlugin,
                InputPlugin,
                InventoryPlugin,
                ItemPlugin,
                LadderPlugin,
                MenuPlugin,
                NpcPlugin,
                PlayerInputPlugin,
                SignpostPlugin,
                SkeletonPlugin,
                SpawnerPlugin,
                SpriteAnimationPlugin,
                InteractionReticlePlugin,
            ),
        ))
        .register_type::<PlayerLantern>()
        .insert_gizmo_config(PhysicsGizmos::default(), GizmoConfig { enabled: false, ..default() })
        .add_systems(Startup, spawn_player)
        .add_systems(Update, (toggle_physics_debug, toggle_world_inspector, ensure_font_atlas_linear))
        .run();
}

/// Switches each newly-rasterised font-atlas texture from nearest to linear sampling.
///
/// The project uses [`bevy::image::ImagePlugin::default_nearest`] for crisp pixel-art
/// sprites, but that same default applies to font atlases — making world-space
/// [`Text2d`] glyphs (e.g. the [`crate::damageable::NameLabel`] above damaged enemies)
/// staircase visibly when the camera scales them. Glyphs are AA-rasterised into the
/// atlas at logical font size; linear filtering gives smooth scaling. UI text renders
/// at 1:1 pixel size, so the change has no visible effect there.
///
/// Font atlases that opt into [`bevy::text::FontSmoothing::None`] explicitly set
/// nearest themselves at creation, which we leave untouched (their `sampler` is
/// `Descriptor(...)`, not `Default`).
fn ensure_font_atlas_linear(
    font_atlas_set: Res<FontAtlasSet>,
    mut images: ResMut<Assets<Image>>,
    mut patched: Local<HashSet<AssetId<Image>>>,
) {
    for atlases in font_atlas_set.values() {
        for atlas in atlases {
            let id = atlas.texture.id();
            if !patched.insert(id) {
                continue;
            }
            if let Some(image) = images.get_mut(&atlas.texture) {
                if matches!(image.sampler, ImageSampler::Default) {
                    image.sampler = ImageSampler::linear();
                }
            }
        }
    }
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

    // 16 main slots (4×4 grid) + 4 hotbar slots = 20 total.
    let mut inventory = Inventory::new(20);
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
