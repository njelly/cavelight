use avian2d::prelude::*;
use bevy::prelude::*;

use crate::dialogue::DialogueSource;
use crate::goap::{GoapAgent, GoapSet};
use crate::grid_mover::{GridMover, GridMoverSet};
use crate::interaction::{InteractEvent, Interactable};
use crate::inventory::InputMode;
use crate::level::NpcSpawnPoint;
use crate::player_input::PlayerControlled;
use crate::sprite_animation::SpriteAnimation;
use crate::GRID_SIZE;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks the female NPC entity.
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct Npc;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Spawns the NPC and registers its type.
///
/// The NPC wanders autonomously via [`GoapPlugin`](crate::goap::GoapPlugin) and is
/// interactable — pressing Space while facing her opens the `"npc_greeting"` dialogue.
/// During dialogue she faces the player and movement is suppressed.
pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Npc>()
            .add_systems(Startup, spawn_npc)
            .add_systems(
                Update,
                hold_npc_facing_during_dialogue
                    .after(GoapSet)
                    .before(GridMoverSet),
            )
            .add_observer(on_npc_interacted);
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// Spawns the female NPC at [`NpcSpawnPoint`] with a GOAP wander agent and a grid mover.
///
/// A kinematic rigid body and collider make the NPC solid so the player and other
/// entities cannot walk through her, and so she registers in spatial queries for
/// both collision and interaction detection.
fn spawn_npc(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn_point: Res<NpcSpawnPoint>,
) {
    let layout = TextureAtlasLayout::from_grid(UVec2::splat(8), 64, 64, None, None);
    let layout_handle = layouts.add(layout);

    commands.spawn((
        Npc,
        Interactable,
        DialogueSource {
            display_name: "Stranger".to_string(),
            dialogue_id: "npc_greeting".to_string(),
        },
        Sprite::from_atlas_image(
            asset_server.load("atlas_8x8.png"),
            TextureAtlas {
                layout: layout_handle,
                index: 64,
            },
        ),
        Transform::from_xyz(spawn_point.0.x, spawn_point.0.y, 0.0),
        SpriteAnimation::with_name("npc_female", true),
        GridMover::new(GRID_SIZE).with_walk_speed(16.0),
        RigidBody::Kinematic,
        Collider::rectangle(GRID_SIZE, GRID_SIZE),
        GoapAgent::wander(6, 10, 1.0, 3.0),
    ));
}

// ---------------------------------------------------------------------------
// Observer
// ---------------------------------------------------------------------------

/// Flips the NPC's sprite to face the player the moment an interaction begins.
///
/// Runs before [`crate::dialogue::DialoguePlugin`]'s observer opens the dialogue panel,
/// so the NPC is already oriented correctly when the first page appears.
fn on_npc_interacted(
    on: On<InteractEvent>,
    mut npc_query: Query<(&Transform, &mut Sprite), With<Npc>>,
    player_query: Query<&Transform, With<PlayerControlled>>,
) {
    let Ok((npc_tf, mut npc_sprite)) = npc_query.get_mut(on.event().entity) else { return };
    let Ok(player_tf) = player_query.single() else { return };

    npc_sprite.flip_x = player_tf.translation.x < npc_tf.translation.x;
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Suppresses NPC movement and keeps her facing the player while a dialogue is active.
///
/// Runs after [`GoapSet`] (which may set a new movement direction or sprite flip) and
/// before [`GridMoverSet`] (which consumes the direction), so GOAP cannot override the
/// facing or cause movement during the conversation.
fn hold_npc_facing_during_dialogue(
    input_mode: Res<InputMode>,
    mut npc_query: Query<(&Transform, &mut GridMover, &mut Sprite), With<Npc>>,
    player_query: Query<&Transform, With<PlayerControlled>>,
) {
    if *input_mode != InputMode::Dialogue {
        return;
    }
    let Ok(player_tf) = player_query.single() else { return };

    for (npc_tf, mut mover, mut sprite) in &mut npc_query {
        mover.direction = None;
        sprite.flip_x = player_tf.translation.x < npc_tf.translation.x;
    }
}
