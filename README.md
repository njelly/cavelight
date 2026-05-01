# Cavelight

A 2D, top-down, procedurally generated, dungeon-crawling, rougelike and colony sim.

## Overview

You've awakened in a mysterious cave that collapsed shut behind you. 
You have only a campfire that is slowly dying--and you will too if you don't figure out something quick! 
You're able to keep it alive for now with the scraps in your pocket, but your survival depends on going one direction: 
deeper. 
deeeeper. 
deeeeeeeeeprrrr....

Cavelight is a survival rougelike: when you die, you respawn at your last campfire with your belongings left (like Dark Souls). 
Cavelight is also an RPG: you can fight enemies, craft items, and level up to increase your stats as you gain experience. 

Along your travels you may run into interesting and skilled allies who will battle along side you or give you access to new mechanics and abilities. 
Cavelight is also a colony sim: your allies can go about their routines in the cave system when they aren't fighting enemies by your side. 
They may explore on their own for rare ores to craft into weapons, farm plants and herbs that grow in the *cavelight* that can be turned into food or medicine, or fulfill other roles that will establish your syndicate as a real community.

The *cavelight* is a mysterious source of light and energy that makes life inside the procedurally generated cave system possible.
In certain areas, the cavelight is bright enough for plants to grow, and this is a key early discovery for the player as they learn to plant crops like wheat, potatoes, tomatoes, etc.

## Controls

WASD: Move

Space: Interact

## Tech stack

Built with Bevy + Rust.

## Physics

Cavelight uses [avian2d](https://github.com/Jondolf/avian) for physics and spatial queries.

### Movement

Player and NPC movement is grid-locked in the style of classic Pokémon (Game Boy) — entities move exactly one tile per step with smooth linear interpolation between cells, driven by the `GridMover` component. Movement is not physics-simulated; `GridMover` sets the transform directly each frame.

### Collision

Wall tiles are registered as `RigidBody::Static` + `Collider::rectangle` entities in avian2d's physics world. Before `GridMover` commits to a step, it uses `SpatialQuery::point_intersections` to test the target tile center against all colliders. If any collider contains that point, the move is blocked.

This design deliberately uses one collider per wall tile rather than a merged compound shape, because it makes Minecraft-style block placement trivial: adding a wall spawns a new entity and removing one removes it — no compound shape rebuild required.

### Future directions

- **Block placement** — the per-tile collider design is already shaped to support it.
- **Projectile physics** — avian2d raycasts or shape casts for arrows, thrown items, etc.
- **Push-back / forces** — entities with `RigidBody::Dynamic` can receive impulses from attacks.
- **Area-of-effect queries** — `shape_intersections` for splash damage, detection radii, etc.
