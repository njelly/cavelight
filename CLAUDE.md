# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

We are developing a video game, *Cavelight*, in Rust using Bevy 0.18.1

## Project structure

Agents should update this section as new files are added, discovered, or modified.
File descriptions should be succinct and useful.

```
cavelight/
├── assets/                         # Resources loaded at runtime (textures, audio, fonts, etc.)
│   ├── atlas_8x8.png               # 512x512 sprite atlas of 8x8 sprites for characters and dynamic objects.
│   ├── sprite_animations.ron       # Animation library: maps animation names to frame index lists and fps.
│   ├── item_definitions.ron        # Item definitions: id, display_name, icon_path, max_stack per item type.
│   ├── entity_definitions.ron      # Entity definitions: id, display_name, toughness per entity type.
│   ├── dialogues.ron               # Dialogue definitions: id and ordered pages array per dialogue entry.
│   ├── item_icons/                 # PNG icons for item types (Dagger_01.png, Arrow_01.png, Bow_01.png).
│   └── fonts/                      # RobotoMono font family (all weights/styles)
├── src/
│   ├── main.rs                     # Entry point; app setup and plugin registration. Spawns player with PlayerLantern child light.
│   ├── aim.rs                      # AimPlugin — bow aim indicator, charge fill, and bow overlay; writes AimState resource (active, charge, direction, origin) for the shooting system.
│   ├── arrow.rs                    # ArrowPlugin — Shoot action consumes ammo and spawns Arrow projectiles (speed/range/damage scale with charge); arrows fly until they hit a wall/entity, applying damage to a Damageable target, then land and can be picked up by walking onto them.
│   ├── camera.rs                   # CameraPlugin — spawns the primary 2D camera with Light2d ambient lighting.
│   ├── campfire.rs                 # CampfirePlugin — campfire sprite+animation at CampfireSpawnPoint; CampfireFlicker drives flickering PointLight2d child.
│   ├── chest.rs                    # ChestPlugin — two chests: WeaponChest (bow+arrows) at WeaponChestSpawnPoint, KeyChest (key) at KeyChestSpawnPoint. Shared observer opens inventory UI on interaction.
│   ├── damageable.rs               # DamageablePlugin — Damageable component (toughness, damage, display_name). Spawns floating health-bar + name-label children that appear once damaged. Despawns dead entities with a death PulseFx.
│   ├── dialogue.rs                 # DialoguePlugin — RON-driven dialogue system. DialogueSource component, DialogueLibrary resource, bottom-of-screen panel UI, Space-to-advance page model. ActiveDialogue::open() for runtime dialogue without a DialogueSource.
│   ├── door.rs                     # DoorPlugin — LockedDoor entity at LockedDoorSpawnPoint. Interaction checks player inventory for a key: consumes it and opens the door, or shows a "locked" dialogue if no key found.
│   ├── entity.rs                   # EntityPlugin — EntityDef/EntityDefList (RON asset), EntityLibrary resource. Loads entity_definitions.ron; provides per-id Damageable construction for spawn helpers.
│   ├── grid_mover.rs               # GridMoverPlugin — smooth grid-locked movement (Pokémon-style). GridMover component; exposes GridMoverSet for system ordering.
│   ├── interaction.rs              # InteractionPlugin — Interactable marker, InteractEvent trigger, InteractionSet system set. Space press fires InteractEvent; gated on InputMode::Playing.
│   ├── interaction_reticle.rs      # InteractionReticlePlugin — tile-highlight square that shows the player's facing tile. Space fades it in; it fades out 1s after last press. Orbits to new facing on direction change.
│   ├── inventory.rs                # InventoryPlugin — dual-panel (Chest/Player) 4x4 inventory UI + hotbar. InputMode (Playing/Inventory/Dialogue/Paused/Settings) gates player input. HeldItem + slot-swap drag model. ActiveChest tracks open chest.
│   ├── menu.rs                     # MenuPlugin — Tab-cycling menu system: Pause screen (Continue/Save&Quit, WASD nav), Settings screen (developer toggles for physics debug + world inspector). WorldInspectorOpen resource. Shared CloseMenuButton (X) and dim overlay. Escape closes to Playing.
│   ├── item.rs                     # ItemPlugin — ItemDef/ItemDefList (RON asset), ItemStack, Inventory component, ItemLibrary resource. Loads item_definitions.ron and pre-loads icon handles.
│   ├── ladder.rs                   # LadderPlugin — solid inert ladder sprite at LadderSpawnPoint (atlas frame 15, "ladder_down"). No interaction yet; floor-transition logic is a future feature.
│   ├── goap.rs                     # GoapPlugin — Goal-Oriented Action Planning. GoapAgent component; WorldState, Goal, Action types; plan_for_goal(); execute_navigate and execute_idle systems; 0.5s achievability replan timer.
│   ├── npc.rs                      # NpcPlugin — female NPC at NpcSpawnPoint; uses GoapAgent(Goal::Wander) with GridMover for A*-planned movement and idle pauses.
│   ├── player_input.rs             # PlayerInputPlugin — keyboard input, Facing component, sprite flipping. PlayerControlled + PlayerInput + Facing; bridges to GridMover. Gated on InputMode::Playing.
│   ├── save.rs                     # SavePlugin — single-slot save/load to <data_dir>/cavelight/saves/<slot>/save.ron (slot 0 today; path is slot-indexed for future multi-slot). PreStartup loads and inserts LoadedSave (skipping level gen on hit), PostStartup applies snapshot to player/chests/door/NPC and spawns skeletons + landed arrows. Save & Quit emits SaveAndExitRequested; process_save_and_exit serializes RON, then writes AppExit.
│   ├── signpost.rs                 # SignpostPlugin — static Interactable signpost at SignpostSpawnPoint; RigidBody + Collider; DialogueSource wired to "signpost_welcome" dialogue.
│   ├── skeleton.rs                 # SkeletonPlugin — Skeleton enemy; observer on SpawnRequested spawns skeleton with GoapAgent(Goal::Wander)+GridMover, a Damageable (pulled from EntityLibrary), and a PulseFx entity for spawn-in effect.
│   ├── spawner.rs                  # SpawnerPlugin — Spawner component (interval-based, capacity-capped); SpawnRequested trigger; SpawnedBy tag; SpawnerSpin spin-on-spawn effect; PulseFx despawn system.
│   ├── sprite_animation.rs         # SpriteAnimationPlugin — loads sprite_animations.ron and drives SpriteAnimation components.
│   ├── wander.rs                   # Pathfinding utilities: astar(), cardinal_neighbors(), pick_random_walkable_in_radius(). No plugin — pure functions used by goap.rs and spawner.rs.
│   └── level/                      # LevelPlugin — graph-based procedural cave generation and tile spawning.
│       ├── mod.rs                  # LevelPlugin (64×64 map); single-texture tilemap; wall LightOccluder2d entities; exports all spawn point resources and DoorOrientation.
│       ├── generator.rs            # generate_level1(): places 4 rooms (Start, WeaponChest, KeyChest, End) in fixed zones with ±jitter, carves L-shaped corridors, applies CA smoothing, enforces 1-tile door bottleneck, flood-fills for connectivity. MapData includes spawner_pos in key room.
│       └── tile.rs                 # TileType enum (Wall/Floor) with per-type render colors. Tile marker component.
├── Cargo.toml
├── Cargo.lock
├── CLAUDE.md
└── README.md                       # Game overview, including game mechanics, theme, vibe, and lore info.
```

## Art style

Characters and interactable objects can be sourced from ./assets/atlas_8x8.png.
Some 8x8 tiles are frames in an animation or the same entity in different states.
For example, frames 0 and 1 are the player idle, and frames 3 and 4 are the chest closed and chest open states.
Environment tiles can be generated at runtime as tinted 8x8 squares(solid blocks of color).
Environment colors can animate and can use a pallette of variant colors to show visual interest.
For example, water tiles can lerp between pleasing blue hues to create a pixelated wave effect, and generated dungeon walls can occasionally have dark and lighter spots to show erosion and natural detail.

## Code guidelines

Agents should write accurate, useful comments where necessary when adding or modifying code.
Comments should always be written *above* relevant code and *never* inline with code.
Always create doc comments for components and systems and important functions, structs, and enums.
The goal is to create comprehensive documentation for Cavelight's systems that can be browsed and understood by humans.

Rust modules should be organized by feature and strive to be independent of sibling modules.
A new Bevy `Plugin` likely warrants its own module.
For example, `GridMoverPlugin`, `GridMover: Component`, `fn update_grid_mover()` would all be in the `grid_mover` mod, which can all exist in `grid_mover.rs`.

Agents should always attempt to reuse logic when possible and write components and systems that are reusable. 
For example, `GridMover` can be used by any entity that moves along the 2d grid, which is most characters, including the player.
Do not duplicate or repeat the same logic in different locations in the codebase.

Agents should always fix any warnings or errors or broken tests when they are encountered, including those that are pre-existing.

## Tools

A `rust-analyzer` MCP server is available. Prefer it over `grep`/`find` for symbol-level questions:

- `hover(file, line, char)` — rendered rustdoc + signature for any symbol, including Bevy and other third-party crates. Use this instead of searching online docs.
- `references(file, line, char)` — every call site of a symbol across the workspace. Use before renames or refactors.
- `diagnostics(file)` — per-file errors/warnings/hints. Faster than `cargo check` for quick pre-flight on a file you just edited.
- `symbols(file)` — map of an unfamiliar file at a glance.
- `definition` / `completion` / `code_actions` / `format` are also available.

Call `set_workspace("/Users/nathaniel/Documents/cavelight")` once per session before the first query.

Skip `workspace_diagnostics` — it's broken in the wrapper. Loop `diagnostics(file)` over the files you care about instead.