//! Data-driven entity definitions loaded from `entity_definitions.ron`.
//!
//! Mirrors the [`crate::item`] pattern: a RON file describes static spawn-time
//! properties of every entity type, the asset is loaded asynchronously, and an
//! [`EntityLibrary`] resource is built once loading completes.
//!
//! Today the library carries only damageable info ([`EntityDef::display_name`],
//! [`EntityDef::toughness`]). It exists as a small foothold so spawn helpers like
//! [`crate::skeleton::spawn_skeleton_entity`] can begin pulling values from this
//! single source of truth — atlas index, walk speed, animation name and the rest
//! will migrate here incrementally as more enemy types are added.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_common_assets::ron::RonAssetPlugin;
use serde::{Deserialize, Serialize};

use crate::damageable::Damageable;

// ---------------------------------------------------------------------------
// Entity definitions (loaded from RON)
// ---------------------------------------------------------------------------

/// Static spawn-time properties of an entity type, loaded from `entity_definitions.ron`.
///
/// [`EntityDef`]s are read-only and shared across all instances of an entity type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    /// Unique string key referencing this entity type. E.g. `"skeleton"`.
    pub id: String,
    /// Player-facing name shown on the floating health-bar label.
    pub display_name: String,
    /// Base hit points before death — copied into [`Damageable::toughness`] at spawn.
    pub toughness: u32,
}

/// Raw list of [`EntityDef`]s deserialized directly from `entity_definitions.ron`.
///
/// Bevy loads this as an [`Asset`]; [`EntityPlugin`] converts it into the runtime
/// [`EntityLibrary`] resource once loading completes.
#[derive(Asset, TypePath, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityDefList(pub Vec<EntityDef>);

/// In-flight asset handle stored until [`EntityLibrary`] is built.
#[derive(Resource)]
struct EntityDefListHandle(Handle<EntityDefList>);

// ---------------------------------------------------------------------------
// Runtime registry
// ---------------------------------------------------------------------------

/// Runtime entity-def registry built from [`EntityDefList`] after the RON asset loads.
///
/// Available as a [`Resource`] once [`EntityPlugin`] finishes initialisation.
#[derive(Resource)]
pub struct EntityLibrary {
    pub defs: HashMap<String, EntityDef>,
}

impl EntityLibrary {
    /// Returns the [`EntityDef`] for `id`, or `None` if unknown.
    pub fn def(&self, id: &str) -> Option<&EntityDef> {
        self.defs.get(id)
    }

    /// Builds a fresh [`Damageable`] component from the def for `id`. Returns
    /// `None` if the id is unknown so callers can fall back to a hardcoded default.
    pub fn damageable(&self, id: &str) -> Option<Damageable> {
        let def = self.def(id)?;
        Some(Damageable::new(def.toughness, def.display_name.clone()))
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Loads `entity_definitions.ron` and provides the [`EntityLibrary`] resource.
pub struct EntityPlugin;

impl Plugin for EntityPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RonAssetPlugin::<EntityDefList>::new(&["ron"]))
            .add_systems(Startup, load_entity_defs)
            .add_systems(Update, build_entity_library);
    }
}

/// Kicks off the async load of `entity_definitions.ron`.
fn load_entity_defs(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handle = asset_server.load("entity_definitions.ron");
    commands.insert_resource(EntityDefListHandle(handle));
}

/// Builds and inserts [`EntityLibrary`] once the [`EntityDefList`] asset finishes loading.
///
/// Runs every frame until the library is ready, then exits early.
fn build_entity_library(
    mut commands: Commands,
    handle: Option<Res<EntityDefListHandle>>,
    list_assets: Res<Assets<EntityDefList>>,
    library: Option<Res<EntityLibrary>>,
) {
    if library.is_some() {
        return;
    }
    let Some(handle) = handle else { return };
    let Some(list) = list_assets.get(&handle.0) else { return };

    let mut defs = HashMap::new();
    for def in &list.0 {
        defs.insert(def.id.clone(), def.clone());
    }
    commands.insert_resource(EntityLibrary { defs });
}
