use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn fps_unset() -> f32 { 0.0 }

/// Frame indices and playback speed for a single named animation.
///
/// `fps` may be omitted in the RON file for single-frame (static) entries.
/// When omitted it defaults to `0.0`, which the animation system treats as
/// "no playback speed required." Multi-frame entries must supply a positive `fps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationData {
    /// Atlas frame indices to play in sequence.
    pub frames: Vec<usize>,
    /// Playback speed in frames per second. Omit for single-frame sprites.
    #[serde(default = "fps_unset")]
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

    /// Switches to a different animation, resetting playback to the first frame.
    ///
    /// Use this to change an entity's animation at runtime (e.g. open ↔ closed door).
    pub fn switch_to(&mut self, name: impl Into<String>) {
        self.name = name.into();
        self.frame_index = 0;
        self.is_complete = false;
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

        // Single-frame entries are static sprites: apply the frame immediately without
        // a timer. This ensures atlas switches (e.g. door open/close, chest open) are
        // instantaneous rather than delayed by the fps interval.
        if data.frames.len() == 1 {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = data.frames[0];
            }
            if !animation.looping {
                animation.is_complete = true;
            }
            continue;
        }

        // Multi-frame animation: advance on a timer driven by the library fps.
        if data.fps <= 0.0 {
            warn!("SpriteAnimation: '{}' has multiple frames but no fps set", animation.name);
            continue;
        }
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

    #[test]
    fn switch_to_resets_name_and_frame() {
        let mut anim = SpriteAnimation::with_name("door_northsouth_closed", false);
        // Simulate some playback progress.
        anim.frame_index = 2;
        anim.is_complete = true;
        anim.switch_to("door_northsouth_open");
        assert_eq!(anim.name, "door_northsouth_open");
        assert_eq!(anim.frame_index, 0);
        assert!(!anim.is_complete);
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

    #[test]
    fn single_frame_animation_is_identified_by_frame_count() {
        // Verify that the frame-count check used by animate_sprites to skip the timer
        // correctly distinguishes static sprites (1 frame) from animated ones (>1 frame).
        let src = r#"{
            "door_closed": (frames: [146]),
            "player_idle": (frames: [0, 1], fps: 2.0)
        }"#;
        let lib: SpriteAnimationLibrary = ron::from_str(src).unwrap();
        assert_eq!(lib.0["door_closed"].frames.len(), 1, "door_closed should be static");
        assert_eq!(lib.0["player_idle"].frames.len(), 2, "player_idle should be animated");
    }

    // --- SpriteAnimationLibrary deserialization ---

    #[test]
    fn library_deserializes_from_ron() {
        let src = r#"{
            "player_idle":  (frames: [0, 1], fps: 6.0),
            "chest_closed": (frames: [3]),
            "chest_open":   (frames: [4]),
        }"#;
        let lib: SpriteAnimationLibrary = ron::from_str(src).unwrap();

        let idle = lib.0.get("player_idle").expect("player_idle missing");
        assert_eq!(idle.frames, vec![0, 1]);
        assert_eq!(idle.fps, 6.0);

        // Single-frame entries omit fps; the default sentinel is 0.0.
        let closed = lib.0.get("chest_closed").expect("chest_closed missing");
        assert_eq!(closed.frames, vec![3]);
        assert_eq!(closed.fps, 0.0);

        let open = lib.0.get("chest_open").expect("chest_open missing");
        assert_eq!(open.frames, vec![4]);
        assert_eq!(open.fps, 0.0);
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
