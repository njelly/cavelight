# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

We are developing a video game, *Cavelight*, in Rust using Bevy.

## Project structure

Agents should update this section as new files are added, discovered, or modified.
File descriptions should be succinct and useful.

```
cavelight/
├── assets/                         # Resources loaded at runtime (textures, audio, fonts, etc.)
│   ├── atlas_8x8.png               # 512x512 sprite atlas of 8x8 sprites for characters and dynamic objects.
│   ├── sprite_animations.ron       # Animation library: maps animation names to frame index lists and fps.
│   └── fonts/                      # RobotoMono font family (all weights/styles)
├── src/
│   ├── main.rs                     # Entry point; app setup and plugin registration.
│   ├── camera.rs                   # CameraPlugin — spawns the primary 2D camera.
│   ├── grid_mover.rs               # GridMoverPlugin — smooth grid-locked movement (Pokémon-style). GridMover component; exposes GridMoverSet for system ordering.
│   ├── player_input.rs             # PlayerInputPlugin — keyboard input, sprite flipping. PlayerControlled + PlayerInput components; bridges to GridMover.
│   └── sprite_animation.rs         # SpriteAnimationPlugin — loads sprite_animations.ron and drives SpriteAnimation components.
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

## Documentation

Use the local, built in documentation when researching Bevy and other crate APIs rather than looking for online documentation.