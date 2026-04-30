use bevy::prelude::*;

/// Spawns and configures the primary 2D camera.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
    }
}

/// Spawns the primary 2D camera at the world origin.
fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
