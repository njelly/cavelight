use bevy::prelude::*;
use bevy_light_2d::prelude::*;

/// Spawns and configures the primary 2D camera.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
    }
}

/// Pixels per world unit. Lower values zoom in; higher values zoom out.
const CAMERA_SCALE: f32 = 0.5;

/// Spawns the primary 2D camera at the world origin with 2D lighting enabled.
///
/// [`Light2d`] activates the `bevy_light_2d` render pass for this camera.
/// Ambient brightness is kept very low to simulate a dark cave; point lights
/// placed on torches, the player, etc. provide the main illumination.
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
        Light2d {
            ambient_light: AmbientLight2d {
                color: Color::WHITE,
                brightness: 0.1,
            },
        },
    ));
}
