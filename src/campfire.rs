use bevy::prelude::*;
use bevy_light_2d::prelude::*;
use rand::{Rng, thread_rng};

use crate::level::CampfireSpawnPoint;
use crate::sprite_animation::SpriteAnimation;

/// Marks the campfire entity.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Campfire;

/// Drives the flickering intensity animation on the campfire's point light.
///
/// Attached to the campfire's light child entity. Each frame the current intensity
/// is lerped toward a randomly-chosen target; the target refreshes on a short
/// repeating timer to produce a natural, organic flame effect.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct CampfireFlicker {
    /// Baseline intensity around which the flame oscillates.
    pub base_intensity: f32,
    /// Maximum random deviation from the baseline per flicker step.
    pub variance: f32,
    /// Current interpolated intensity, written to [`PointLight2d::intensity`] every frame.
    pub current: f32,
    /// Target intensity the current value is lerping toward.
    pub target: f32,
    /// How often a new random target is chosen.
    pub timer: Timer,
}

impl CampfireFlicker {
    fn new(base_intensity: f32, variance: f32) -> Self {
        Self {
            base_intensity,
            variance,
            current: base_intensity,
            target: base_intensity,
            timer: Timer::from_seconds(0.08, TimerMode::Repeating),
        }
    }
}

/// Spawns the campfire entity and registers the flicker system.
pub struct CampfirePlugin;

impl Plugin for CampfirePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Campfire>()
            .register_type::<CampfireFlicker>()
            .add_systems(Startup, spawn_campfire)
            .add_systems(Update, flicker_campfire_light);
    }
}

/// Spawns the campfire sprite and its flickering point light child at the level's campfire position.
fn spawn_campfire(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<CampfireSpawnPoint>,
) {
    // Same 64x64 atlas grid used by all 8x8 sprites.
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands
        .spawn((
            Campfire,
            Sprite::from_atlas_image(
                asset_server.load("atlas_8x8.png"),
                TextureAtlas {
                    layout: layout_handle,
                    index: 75,
                },
            ),
            Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
            SpriteAnimation::with_name("campfire", true),
        ))
        .with_children(|parent| {
            // Warm orange-red fire light. Intensity and radius are the "calm" baseline;
            // CampfireFlicker adds organic variation each frame.
            parent.spawn((
                Transform::default(),
                PointLight2d {
                    color: Color::srgb(1.0, 0.55, 0.1),
                    intensity: 4.0,
                    radius: 80.0,
                    falloff: 3.0,
                    cast_shadows: true,
                },
                CampfireFlicker::new(4.0, 1.5),
            ));
        });
}

/// Each frame: advances the flicker timer, picks a new random intensity target when it
/// fires, then smoothly interpolates the light's intensity toward that target.
///
/// The lerp speed of 12 gives a lively flicker without jarring jumps.
fn flicker_campfire_light(
    time: Res<Time>,
    mut query: Query<(&mut PointLight2d, &mut CampfireFlicker)>,
) {
    let mut rng = thread_rng();
    for (mut light, mut flicker) in &mut query {
        flicker.timer.tick(time.delta());
        if flicker.timer.just_finished() {
            flicker.target =
                flicker.base_intensity + rng.gen_range(-flicker.variance..=flicker.variance);
        }
        flicker.current = flicker.current.lerp(flicker.target, 12.0 * time.delta_secs());
        light.intensity = flicker.current;
    }
}
