use bevy::prelude::*;

/// Tracks the in-progress frame count and elapsed time for the current second.
#[derive(Resource, Default)]
struct FpsState {
    /// Frames counted since the last display update.
    frames: u32,
    /// Seconds accumulated since the last display update.
    elapsed: f32,
}

/// Marker for the FPS counter text node.
#[derive(Component)]
struct FpsText;

/// Spawns the FPS counter and updates it once per second.
pub struct FpsCounterPlugin;

impl Plugin for FpsCounterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FpsState>()
            .add_systems(Startup, spawn_fps_counter)
            .add_systems(Update, update_fps_counter);
    }
}

/// Spawns the FPS text node anchored to the bottom-right corner of the screen.
fn spawn_fps_counter(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/RobotoMono-Regular.ttf");

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
        GlobalZIndex(20),
        Text::new("-- fps"),
        TextFont {
            font,
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
        FpsText,
    ));
}

/// Counts frames each tick; flushes the display once a full second has elapsed.
fn update_fps_counter(
    time: Res<Time>,
    mut state: ResMut<FpsState>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    state.frames += 1;
    state.elapsed += time.delta_secs();

    if state.elapsed >= 1.0 {
        let fps = state.frames;
        state.frames = 0;
        state.elapsed = 0.0;

        for mut text in &mut query {
            *text = Text::new(format!("{fps} fps"));
        }
    }
}
