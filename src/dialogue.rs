use std::collections::HashMap;

use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::{Deserialize, Serialize};

use crate::interaction::{InteractEvent, InteractionSet};
use crate::inventory::{InputMode, HOTBAR_HEIGHT_VH};

// ---------------------------------------------------------------------------
// Dialogue definitions (loaded from RON)
// ---------------------------------------------------------------------------

/// A single dialogue entry loaded from `dialogues.ron`.
///
/// Each definition contains an `id` for lookup and an ordered list of `pages`.
/// The player advances through pages one at a time; the panel closes after the last page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueDef {
    /// Unique key used to reference this dialogue from a [`DialogueSource`] component.
    pub id: String,
    /// Ordered text pages shown in the dialogue panel. Must not be empty.
    pub pages: Vec<String>,
}

/// Raw list of [`DialogueDef`]s deserialized from `dialogues.ron`.
///
/// Loaded as a Bevy asset; [`DialoguePlugin`] converts it into the runtime
/// [`DialogueLibrary`] resource once loading completes.
#[derive(Asset, TypePath, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DialogueDefList(pub Vec<DialogueDef>);

/// In-flight asset handle stored until [`DialogueLibrary`] is built.
#[derive(Resource)]
struct DialogueDefListHandle(Handle<DialogueDefList>);

// ---------------------------------------------------------------------------
// Runtime dialogue registry
// ---------------------------------------------------------------------------

/// Runtime registry built from [`DialogueDefList`] once the RON asset finishes loading.
///
/// Provides O(1) lookup of dialogue definitions by id. Available as a [`Resource`]
/// after the first frame on which the asset finishes loading.
#[derive(Resource)]
pub struct DialogueLibrary {
    defs: HashMap<String, DialogueDef>,
}

impl DialogueLibrary {
    /// Returns the definition for `id`, or `None` if unknown.
    pub fn get(&self, id: &str) -> Option<&DialogueDef> {
        self.defs.get(id)
    }
}

// ---------------------------------------------------------------------------
// DialogueSource component
// ---------------------------------------------------------------------------

/// Marks an entity as having dialogue the player can read by interacting with it.
///
/// Works with any [`Interactable`](crate::interaction::Interactable) entity — signposts,
/// NPCs, notice boards, etc. When the player interacts with this entity the
/// [`DialoguePlugin`] opens the dialogue panel and loads the referenced definition
/// from [`DialogueLibrary`].
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct DialogueSource {
    /// Name shown in the dialogue panel header (e.g. `"Sign"`, `"Merchant"`).
    pub display_name: String,
    /// Key into [`DialogueLibrary`] selecting which pages to display.
    pub dialogue_id: String,
}

// ---------------------------------------------------------------------------
// Active dialogue state
// ---------------------------------------------------------------------------

/// Internal state for a currently-open dialogue session.
struct ActiveDialogueState {
    /// Name displayed in the panel header.
    source_name: String,
    /// Ordered pages to show. Never empty.
    pages: Vec<String>,
    /// Index of the currently displayed page.
    current_page: usize,
    /// `true` for one frame after opening to prevent the Space press that triggered
    /// the interaction from also advancing the first page.
    just_opened: bool,
}

impl ActiveDialogueState {
    /// Clears the just-opened guard. Returns `true` if the flag was set, meaning
    /// the caller should skip processing the current Space press.
    fn consume_just_opened(&mut self) -> bool {
        let was = self.just_opened;
        self.just_opened = false;
        was
    }

    /// Advances to the next page. Returns `true` if more pages follow, `false`
    /// if this was the last page (caller should close the dialogue).
    fn advance_page(&mut self) -> bool {
        if self.current_page + 1 < self.pages.len() {
            self.current_page += 1;
            true
        } else {
            false
        }
    }
}

/// Holds the currently-open dialogue session, if any.
///
/// `None` when the dialogue panel is closed. Set by [`on_interact_with_dialogue_source`]
/// when the player interacts with a [`DialogueSource`] entity, and cleared by
/// [`advance_dialogue`] when the last page is dismissed.
///
/// External systems (e.g. door interactions) can open a dialogue directly via [`ActiveDialogue::open`]
/// without needing access to the private [`ActiveDialogueState`] internals.
#[derive(Resource, Default)]
pub struct ActiveDialogue(Option<ActiveDialogueState>);

impl ActiveDialogue {
    /// Opens a dialogue session from raw parts, bypassing [`DialogueLibrary`] lookup.
    ///
    /// Use this when the content and source name are determined at runtime (e.g. a locked
    /// door that shows different text depending on inventory state). The `just_opened` guard
    /// is set so the Space press that triggered the interaction does not also advance the page.
    pub fn open(
        &mut self,
        source_name: impl Into<String>,
        pages: Vec<String>,
        input_mode: &mut InputMode,
    ) {
        self.0 = Some(ActiveDialogueState {
            source_name: source_name.into(),
            pages,
            current_page: 0,
            just_opened: true,
        });
        *input_mode = InputMode::Dialogue;
    }
}

// ---------------------------------------------------------------------------
// UI marker components
// ---------------------------------------------------------------------------

/// Marker for the dialogue panel root node.
#[derive(Component)]
struct DialoguePanel;

/// Marker for the speaker-name [`Text`] node inside the dialogue header.
#[derive(Component)]
struct DialogueNameText;

/// Marker for the page-body [`Text`] node inside the dialogue panel.
#[derive(Component)]
struct DialogueBodyText;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Manages dialogue data loading, panel UI, and Space-to-advance interaction.
pub struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<DialogueDefList>::new(&["ron"]))
            .register_type::<DialogueSource>()
            .init_resource::<ActiveDialogue>()
            .add_systems(Startup, (load_dialogue_defs, spawn_dialogue_ui))
            .add_systems(
                Update,
                (
                    build_dialogue_library,
                    // advance_dialogue must run after fire_interact_events so that closing
                    // the dialogue on the last page does not immediately re-trigger an interaction.
                    // sync_dialogue_ui is chained after so it always sees the final state.
                    (advance_dialogue, sync_dialogue_ui).chain().after(InteractionSet),
                ),
            )
            .add_observer(on_interact_with_dialogue_source);
    }
}

// ---------------------------------------------------------------------------
// Startup systems
// ---------------------------------------------------------------------------

/// Starts the async load of `dialogues.ron` and stores the handle.
fn load_dialogue_defs(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handle = asset_server.load("dialogues.ron");
    commands.insert_resource(DialogueDefListHandle(handle));
}

/// Builds and inserts [`DialogueLibrary`] once the asset finishes loading.
fn build_dialogue_library(
    mut commands: Commands,
    handle: Option<Res<DialogueDefListHandle>>,
    list_assets: Res<Assets<DialogueDefList>>,
    library: Option<Res<DialogueLibrary>>,
) {
    if library.is_some() {
        return;
    }
    let Some(handle) = handle else { return };
    let Some(list) = list_assets.get(&handle.0) else { return };

    let defs = list.0.iter().cloned().map(|d| (d.id.clone(), d)).collect();
    commands.insert_resource(DialogueLibrary { defs });
}

/// Spawns the dialogue panel anchored above the hotbar.
///
/// The panel starts hidden (`Display::None`) and is shown by [`sync_dialogue_ui`]
/// when [`ActiveDialogue`] becomes `Some`.
fn spawn_dialogue_ui(mut commands: Commands) {
    let panel_bg = Color::srgba(0.06, 0.05, 0.04, 0.50);
    let header_bg = Color::srgba(0.04, 0.03, 0.02, 0.75);
    let text_col = Color::srgb(0.85, 0.80, 0.70);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Vh(HOTBAR_HEIGHT_VH),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            BackgroundColor(panel_bg),
            GlobalZIndex(8),
            DialoguePanel,
        ))
        .with_children(|panel| {
            // Speaker name header
            panel
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(header_bg),
                ))
                .with_children(|header| {
                    header.spawn((
                        Text::new(""),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(text_col),
                        DialogueNameText,
                    ));
                });

            // Page body text
            panel
                .spawn(Node {
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(12.0)),
                    ..default()
                })
                .with_children(|body| {
                    body.spawn((
                        Text::new(""),
                        TextFont { font_size: 15.0, ..default() },
                        TextColor(text_col),
                        DialogueBodyText,
                    ));
                });
        });
}

// ---------------------------------------------------------------------------
// Observer
// ---------------------------------------------------------------------------

/// Opens the dialogue panel when the player interacts with a [`DialogueSource`] entity.
///
/// Fires for every [`InteractEvent`]; entities without a [`DialogueSource`] component
/// are silently ignored. The `just_opened` flag is set to `true` so that the Space
/// press that triggered the interaction is not forwarded to [`advance_dialogue`].
fn on_interact_with_dialogue_source(
    on: On<InteractEvent>,
    sources: Query<&DialogueSource>,
    library: Option<Res<DialogueLibrary>>,
    mut active: ResMut<ActiveDialogue>,
    mut input_mode: ResMut<InputMode>,
) {
    let Ok(source) = sources.get(on.event().entity) else { return };
    let Some(library) = library else { return };
    let Some(def) = library.get(&source.dialogue_id) else {
        warn!("DialogueSource references unknown dialogue id '{}'", source.dialogue_id);
        return;
    };
    if def.pages.is_empty() {
        warn!("Dialogue '{}' has no pages — ignoring interaction.", source.dialogue_id);
        return;
    }

    active.0 = Some(ActiveDialogueState {
        source_name: source.display_name.clone(),
        pages: def.pages.clone(),
        current_page: 0,
        just_opened: true,
    });
    *input_mode = InputMode::Dialogue;
}

// ---------------------------------------------------------------------------
// Update systems
// ---------------------------------------------------------------------------

/// Advances dialogue pages on Space press and closes the panel after the last page.
///
/// Runs after [`InteractionSet`] to ensure that closing dialogue on the final page
/// cannot immediately re-trigger an interaction in the same frame.
///
/// The `just_opened` guard absorbs the Space press that originally opened the dialogue,
/// preventing it from also advancing to page two.
fn advance_dialogue(
    keys: Res<ButtonInput<KeyCode>>,
    mut active: ResMut<ActiveDialogue>,
    mut input_mode: ResMut<InputMode>,
) {
    if *input_mode != InputMode::Dialogue {
        return;
    }
    let Some(state) = &mut active.0 else { return };

    // Absorb the Space press used to open the dialogue — do not advance yet.
    if state.consume_just_opened() {
        return;
    }

    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    if !state.advance_page() {
        // Last page dismissed — close the dialogue.
        active.0 = None;
        *input_mode = InputMode::Playing;
    }
}

/// Keeps the dialogue panel in sync with [`ActiveDialogue`].
///
/// Shows or hides the panel and updates the speaker name and body text.
/// Runs every frame (after [`advance_dialogue`]) so state changes are always reflected.
fn sync_dialogue_ui(
    active: Res<ActiveDialogue>,
    mut panel: Query<&mut Node, With<DialoguePanel>>,
    mut name_text: Query<&mut Text, (With<DialogueNameText>, Without<DialogueBodyText>)>,
    mut body_text: Query<&mut Text, (With<DialogueBodyText>, Without<DialogueNameText>)>,
) {
    let Ok(mut panel_node) = panel.single_mut() else { return };

    match &active.0 {
        None => {
            panel_node.display = Display::None;
        }
        Some(state) => {
            panel_node.display = Display::Flex;
            if let Ok(mut text) = name_text.single_mut() {
                *text = Text::new(state.source_name.clone());
            }
            if let Ok(mut text) = body_text.single_mut() {
                *text = Text::new(state.pages[state.current_page].clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(pages: &[&str]) -> ActiveDialogueState {
        ActiveDialogueState {
            source_name: "Sign".to_string(),
            pages: pages.iter().map(|s| s.to_string()).collect(),
            current_page: 0,
            just_opened: false,
        }
    }

    #[test]
    fn consume_just_opened_clears_flag() {
        let mut state = make_state(&["p1"]);
        state.just_opened = true;

        assert!(state.consume_just_opened(), "should return true when flag was set");
        assert!(!state.just_opened, "flag should be cleared");
        assert!(!state.consume_just_opened(), "subsequent call returns false");
    }

    #[test]
    fn advance_page_progresses_through_pages() {
        let mut state = make_state(&["p1", "p2", "p3"]);

        assert!(state.advance_page(), "p1→p2: more pages remain");
        assert_eq!(state.current_page, 1);

        assert!(state.advance_page(), "p2→p3: more pages remain");
        assert_eq!(state.current_page, 2);

        assert!(!state.advance_page(), "p3 is last: returns false");
        assert_eq!(state.current_page, 2, "page index does not go out of bounds");
    }

    #[test]
    fn advance_page_single_page_is_immediately_last() {
        let mut state = make_state(&["only page"]);
        assert!(!state.advance_page(), "single page is immediately at end");
    }

    #[test]
    fn dialogue_def_list_deserializes_from_ron() {
        let ron_str = r#"[
            (id: "test_dialogue", pages: ["Hello", "World"]),
        ]"#;
        let list: DialogueDefList = ron::from_str(ron_str).unwrap();
        assert_eq!(list.0.len(), 1);
        assert_eq!(list.0[0].id, "test_dialogue");
        assert_eq!(list.0[0].pages, vec!["Hello".to_string(), "World".to_string()]);
    }

    #[test]
    fn dialogue_library_get_returns_correct_def() {
        let mut defs = HashMap::new();
        defs.insert(
            "signpost_welcome".to_string(),
            DialogueDef {
                id: "signpost_welcome".to_string(),
                pages: vec!["Welcome, adventurer.".to_string()],
            },
        );
        let lib = DialogueLibrary { defs };

        let def = lib.get("signpost_welcome").unwrap();
        assert_eq!(def.id, "signpost_welcome");
        assert_eq!(def.pages.len(), 1);
        assert!(lib.get("nonexistent").is_none());
    }
}
