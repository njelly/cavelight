#![doc = include_str!("../README.md")]

mod camera;

use bevy::prelude::*;
use avian2d::prelude::*;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use camera::CameraPlugin;

// One grid cell = 8x8 pixels
pub const GRID_SIZE: f32 = 8.0;

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
                WorldInspectorPlugin::new()
            ),

            // Internal plugins
            // Note: Nesting of plugin tuples is required eventually, see:
            // https://bevy-cheatbook.github.io/programming/systems.html
            // | Your function can have a maximum of 16 total parameters.
            // | If you need more, group them into tuples to work around the limit.
            // | Tuples can contain up to 16 members but can be nested indefinitely.
            (
                CameraPlugin,
            ),
        ))
        .add_systems(Startup, spawn_sprite)
        .run();
}

/// Spawns the 0th sprite from the atlas as a visual sanity check.
fn spawn_sprite(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // 512x512 atlas divided into 8x8 tiles = 64 columns, 64 rows
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);
    commands.spawn(Sprite::from_atlas_image(
        asset_server.load("atlas_8x8.png"),
        TextureAtlas {
            layout: layout_handle,
            index: 0,
        },
    ));
}
