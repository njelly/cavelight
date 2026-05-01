use avian2d::prelude::*;
use bevy::prelude::*;

use crate::grid_mover::GridMoverSet;
use crate::player_input::{Facing, PlayerControlled};
use crate::GRID_SIZE;

/// Marker component for entities the player can interact with by pressing Space.
///
/// Any entity with this component and a [`Collider`] will receive an [`InteractEvent`]
/// when the player faces its tile and presses Space.
#[derive(Component)]
pub struct Interactable;

/// Triggered when the player presses Space while facing a tile that contains an [`Interactable`] entity.
///
/// Fired via [`Commands::trigger`] so any [`Observer`] registered for this event type
/// will run immediately (within the same frame, after the triggering system completes).
/// The `entity` field identifies the interactable that was targeted.
#[derive(Event)]
pub struct InteractEvent {
    /// The entity being interacted with.
    pub entity: Entity,
}

/// System set for the interaction dispatch system.
///
/// [`InteractEvent`]s are triggered during this set. Observers that respond to
/// interactions (e.g. opening a chest) run as part of observer dispatch, which
/// occurs when [`Commands`] are flushed — typically at the end of the frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct InteractionSet;

/// Registers the system that fires interaction triggers.
pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            fire_interact_events
                .in_set(InteractionSet)
                .after(GridMoverSet),
        );
    }
}

/// On Space press, tests the tile directly in front of the player for [`Interactable`] entities
/// and triggers an [`InteractEvent`] for each one found.
///
/// Uses avian2d [`SpatialQuery::point_intersections`] to find colliders at the target position,
/// then filters for those carrying the [`Interactable`] marker before triggering.
fn fire_interact_events(
    keys: Res<ButtonInput<KeyCode>>,
    player_query: Query<(&Transform, &Facing), With<PlayerControlled>>,
    interactable_query: Query<(), With<Interactable>>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    let Ok((transform, facing)) = player_query.single() else {
        return;
    };

    let target = transform.translation.truncate() + facing.offset() * GRID_SIZE;

    for entity in spatial_query.point_intersections(target, &SpatialQueryFilter::default()) {
        if interactable_query.contains(entity) {
            commands.trigger(InteractEvent { entity });
        }
    }
}
