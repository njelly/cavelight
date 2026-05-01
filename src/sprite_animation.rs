use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Frame indices and playback speed for a single named animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationData {
    /// Atlas frame indices to play in sequence.
    pub frames: Vec<usize>,
    /// Playback speed in frames per second.
    pub fps: f32,
}

/// The full animation library loaded from `sprite_animations.ron`.
///
/// Maps animation name strings to their [`AnimationData`]. Marked `#[serde(transparent)]`
/// so the RON file is a plain map rather than a newtype wrapper.
#[derive(Asset, TypePath, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpriteAnimationLibrary(pub HashMap<String, AnimationData>);

/// Handle to the loaded [`SpriteAnimationLibrary`] asset.
#[derive(Resource)]
pub struct AnimationLibraryHandle(pub Handle<SpriteAnimationLibrary>);

/// Drives frame-by-frame sprite animation using a named entry from [`SpriteAnimationLibrary`].
///
/// Attach alongside a [`Sprite`] that has a [`TextureAtlas`]. The animation name is resolved
/// at runtime from the loaded RON asset.
///
/// # Example
/// ```rust,ignore
/// commands.spawn((sprite, SpriteAnimation::with_name("player_idle", true)));
/// ```
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct SpriteAnimation {
    /// Key into [`SpriteAnimationLibrary`].
    pub name: String,
    /// When `true`, the animation loops indefinitely and `is_complete` is never set.
    pub looping: bool,
    /// Set to `true` after a non-looping animation plays through its last frame.
    pub is_complete: bool,
    /// Current position within the animation's frame list.
    frame_index: usize,
    /// Timer controlling when to advance to the next frame.
    timer: Timer,
}

impl SpriteAnimation {
    /// Creates a new animation by name.
    ///
    /// The timer starts with a placeholder duration; [`animate_sprites`] will sync it
    /// to the library's configured fps as soon as the asset is loaded.
    pub fn with_name(name: impl Into<String>, looping: bool) -> Self {
        Self {
            name: name.into(),
            looping,
            is_complete: false,
            frame_index: 0,
            timer: Timer::from_seconds(1.0 / 8.0, TimerMode::Repeating),
        }
    }


}

/// Loads `sprite_animations.ron` and drives [`SpriteAnimation`] components each frame.
pub struct SpriteAnimationPlugin;

impl Plugin for SpriteAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SpriteAnimation>()
            .add_plugins(RonAssetPlugin::<SpriteAnimationLibrary>::new(&["ron"]))
            .add_systems(Startup, load_animation_library)
            .add_systems(Update, animate_sprites);
    }
}

/// Loads `sprite_animations.ron` and stores its handle as a resource.
fn load_animation_library(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handle = asset_server.load("sprite_animations.ron");
    commands.insert_resource(AnimationLibraryHandle(handle));
}

/// Ticks all active [`SpriteAnimation`] components and updates the [`TextureAtlas`] index.
fn animate_sprites(
    time: Res<Time>,
    library_handle: Res<AnimationLibraryHandle>,
    libraries: Res<Assets<SpriteAnimationLibrary>>,
    mut query: Query<(&mut SpriteAnimation, &mut Sprite)>,
) {
    let Some(library) = libraries.get(&library_handle.0) else {
        return;
    };

    for (mut animation, mut sprite) in &mut query {
        if animation.is_complete {
            continue;
        }

        let Some(data) = library.0.get(&animation.name) else {
            warn!("SpriteAnimation: '{}' not found in library", animation.name);
            continue;
        };

        // Sync duration to library fps so hot-reload changes propagate immediately.
        let frame_duration = std::time::Duration::from_secs_f32(1.0 / data.fps);
        if animation.timer.duration() != frame_duration {
            animation.timer.set_duration(frame_duration);
        }

        animation.timer.tick(time.delta());
        if !animation.timer.just_finished() {
            continue;
        }

        let (next_index, complete) =
            advance_frame(animation.frame_index, data.frames.len(), animation.looping);
        animation.frame_index = next_index;
        animation.is_complete = complete;

        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = data.frames[animation.frame_index];
        }
    }
}

/// Returns `(next_frame_index, is_complete)` for one animation tick.
///
/// Looping animations wrap back to 0; non-looping animations stop on the last frame.
fn advance_frame(current: usize, total: usize, looping: bool) -> (usize, bool) {
    if looping {
        ((current + 1) % total, false)
    } else if current + 1 < total {
        (current + 1, false)
    } else {
        (current, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SpriteAnimation::with_name ---

    #[test]
    fn with_name_looping_initializes_correctly() {
        let anim = SpriteAnimation::with_name("player_idle", true);
        assert_eq!(anim.name, "player_idle");
        assert!(anim.looping);
        assert!(!anim.is_complete);
        assert_eq!(anim.frame_index, 0);
    }

    #[test]
    fn with_name_non_looping_initializes_correctly() {
        let anim = SpriteAnimation::with_name("chest_open", false);
        assert_eq!(anim.name, "chest_open");
        assert!(!anim.looping);
        assert!(!anim.is_complete);
        assert_eq!(anim.frame_index, 0);
    }

    // --- advance_frame ---

    #[test]
    fn advance_frame_looping_mid_sequence() {
        assert_eq!(advance_frame(0, 3, true), (1, false));
    }

    #[test]
    fn advance_frame_looping_wraps_at_end() {
        assert_eq!(advance_frame(1, 2, true), (0, false));
    }

    #[test]
    fn advance_frame_looping_single_frame_stays() {
        assert_eq!(advance_frame(0, 1, true), (0, false));
    }

    #[test]
    fn advance_frame_non_looping_advances() {
        assert_eq!(advance_frame(0, 3, false), (1, false));
    }

    #[test]
    fn advance_frame_non_looping_completes_at_last_frame() {
        assert_eq!(advance_frame(2, 3, false), (2, true));
    }

    #[test]
    fn advance_frame_non_looping_single_frame_completes_immediately() {
        assert_eq!(advance_frame(0, 1, false), (0, true));
    }

    // --- SpriteAnimationLibrary deserialization ---

    #[test]
    fn library_deserializes_from_ron() {
        let src = r#"{
            "player_idle":  (frames: [0, 1], fps: 6.0),
            "chest_closed": (frames: [3],    fps: 1.0),
            "chest_open":   (frames: [4],    fps: 1.0),
        }"#;
        let lib: SpriteAnimationLibrary = ron::from_str(src).unwrap();

        let idle = lib.0.get("player_idle").expect("player_idle missing");
        assert_eq!(idle.frames, vec![0, 1]);
        assert_eq!(idle.fps, 6.0);

        let closed = lib.0.get("chest_closed").expect("chest_closed missing");
        assert_eq!(closed.frames, vec![3]);

        let open = lib.0.get("chest_open").expect("chest_open missing");
        assert_eq!(open.frames, vec![4]);
    }

    #[test]
    fn library_with_multiple_frames_preserves_order() {
        let src = r#"{ "walk": (frames: [5, 6, 7, 8], fps: 12.0) }"#;
        let lib: SpriteAnimationLibrary = ron::from_str(src).unwrap();
        assert_eq!(lib.0["walk"].frames, vec![5, 6, 7, 8]);
    }

    #[test]
    fn library_rejects_invalid_ron() {
        let bad = r#"{ "broken": oops }"#;
        assert!(ron::from_str::<SpriteAnimationLibrary>(bad).is_err());
    }
}
