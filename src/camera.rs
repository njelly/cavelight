use bevy::prelude::*;
use bevy_light_2d::prelude::*;

use crate::level::{LEVEL_HEIGHT, LEVEL_WIDTH};
use crate::player_input::PlayerControlled;
use crate::GRID_SIZE;

/// The number of world units the camera shows vertically, regardless of window height.
///
/// Scaling by window height means fullscreening does not reveal more of the world —
/// it only improves pixel density. One world unit = one pixel in the source atlas,
/// and one tile = [`GRID_SIZE`] world units.
const TARGET_VIEWPORT_HEIGHT: f32 = 96.0; // 12 tiles

/// Spawns the camera and follows the player each frame, clamped to the level boundary.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, follow_player);
    }
}

/// Spawns the primary 2D camera with 2D lighting enabled.
///
/// Ambient brightness is kept very low to simulate a dark cave; point lights placed
/// on the player, campfire, etc. provide the main illumination.
///
/// The orthographic scale is updated every frame by [`follow_player`] to maintain a
/// constant world-space viewport height as the window resizes.
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection::default_2d()),
        Light2d {
            ambient_light: AmbientLight2d {
                color: Color::WHITE,
                brightness: 0.1,
            },
        },
    ));
}

/// Moves the camera to track the player and clamps it so the viewport never shows
/// outside the level.
///
/// The orthographic scale is computed each frame so that the vertical world-space
/// extent equals [`TARGET_VIEWPORT_HEIGHT`], keeping the zoom level constant across
/// all window sizes and fullscreen modes.
///
/// After clamping, the camera translation is snapped to the nearest screen pixel.
/// This guarantees every atlas sprite is sampled at the centre of its texels rather
/// than exactly on a UV boundary, which prevents sub-pixel bleeding from adjacent
/// atlas frames.
fn follow_player(
    player_query: Query<&Transform, With<PlayerControlled>>,
    mut camera_query: Query<(&mut Transform, &mut Projection), (With<Camera2d>, Without<PlayerControlled>)>,
    window_query: Query<&Window>,
) {
    let Ok(player) = player_query.single() else { return; };
    let Ok((mut camera, mut projection)) = camera_query.single_mut() else { return; };
    let Ok(window) = window_query.single() else { return; };

    // Derive scale from window height so vertical world extent stays constant.
    let scale = TARGET_VIEWPORT_HEIGHT / window.height();
    if let Projection::Orthographic(ref mut ortho) = *projection {
        ortho.scale = scale;
    }

    // Half the viewport size in world units at the current scale.
    let half_vp = Vec2::new(
        window.width() * scale / 2.0,
        window.height() * scale / 2.0,
    );

    // The tilemap sprite is shifted by -GRID_SIZE/2 so tile centers align with the snap grid.
    let level_center = Vec2::splat(-GRID_SIZE / 2.0);
    let level_half_size = Vec2::new(
        LEVEL_WIDTH as f32 * GRID_SIZE / 2.0,
        LEVEL_HEIGHT as f32 * GRID_SIZE / 2.0,
    );
    let level_min = level_center - level_half_size;
    let level_max = level_center + level_half_size;

    let target = player.translation.truncate();

    // Compute clamp bounds. If the viewport is wider than the level, center on the level instead.
    let x_min = level_min.x + half_vp.x;
    let x_max = level_max.x - half_vp.x;
    let y_min = level_min.y + half_vp.y;
    let y_max = level_max.y - half_vp.y;

    let clamped_x = if x_max >= x_min { target.x.clamp(x_min, x_max) } else { level_center.x };
    let clamped_y = if y_max >= y_min { target.y.clamp(y_min, y_max) } else { level_center.y };

    // Snap to the nearest screen pixel so atlas sprites are always sampled at texel
    // centres rather than UV boundaries. Without this, sub-pixel camera positions cause
    // the GPU to sample the edge between adjacent atlas frames, producing 1-pixel colour
    // bleeds from neighbouring sprites.
    camera.translation.x = (clamped_x / scale).round() * scale;
    camera.translation.y = (clamped_y / scale).round() * scale;
}
