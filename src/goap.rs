use std::collections::VecDeque;
use std::time::Duration;

use avian2d::prelude::*;
use bevy::prelude::*;
use rand::{Rng, thread_rng};

use crate::grid_mover::{GridMover, GridMoverSet, snap_to_grid};
use crate::level::LevelTiles;
use crate::wander::{astar, pick_random_walkable_in_radius};

// ---------------------------------------------------------------------------
// World state
// ---------------------------------------------------------------------------

/// The agent's typed snapshot of relevant world facts.
///
/// Each field represents one boolean fact the planner reasons over.
/// Add new fields here as new goals and actions require new facts.
#[derive(Debug, Clone, PartialEq, Eq, Reflect, Default)]
pub struct WorldState {
    /// True when the agent has arrived at its current navigation destination.
    pub at_destination: bool,
}

// ---------------------------------------------------------------------------
// Goals
// ---------------------------------------------------------------------------

/// High-level objective an agent pursues.
///
/// The planner maps a goal + current world state to a sequence of [`Action`]s.
/// Add variants here as new behaviours are introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Goal {
    /// Roam the level: navigate to random positions with idle pauses between steps.
    Wander,
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Atomic behaviour an agent can execute.
///
/// Each action has implicit preconditions (the world state it requires to start)
/// and effects (how it changes world state when complete). These are encoded in
/// [`plan_for_goal`] and the `complete_*` methods on [`GoapAgent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Action {
    /// Pick a random nearby walkable tile and path to it via A*.
    NavigateToRandomPosition,
    /// Pause movement for a random duration before acting again.
    Idle,
}

// ---------------------------------------------------------------------------
// Agent component
// ---------------------------------------------------------------------------

/// GOAP agent component — drives goal-oriented behaviour for any entity.
///
/// Attach alongside [`GridMover`] and [`Sprite`]. The agent maintains a current
/// [`Goal`], a typed [`WorldState`], and a queue of [`Action`]s to execute.
///
/// # Replanning
/// Plans are regenerated in two cases:
/// 1. The current action completes — the next action is started immediately.
/// 2. A repeating timer fires — the goal is checked for achievability and a
///    stale plan is flushed so a fresh one can be generated next tick.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GoapAgent {
    /// The goal the agent is currently pursuing.
    pub goal: Goal,
    /// The agent's current understanding of the world.
    pub world_state: WorldState,
    /// The action currently being executed, if any.
    pub current_action: Option<Action>,
    /// Remaining actions in the current plan.
    #[reflect(ignore)]
    pub plan: VecDeque<Action>,
    /// Fires periodically to check that the current goal is still achievable.
    #[reflect(ignore)]
    replan_timer: Timer,
    /// Tile radius (Euclidean) for picking navigation destinations.
    pub nav_radius: usize,
    /// Maximum A* path steps — prevents routing far around walls.
    pub nav_max_path_steps: usize,
    /// Minimum idle duration between navigations (seconds).
    pub idle_min_secs: f32,
    /// Maximum idle duration between navigations (seconds).
    pub idle_max_secs: f32,
    // --- NavigateToRandomPosition action state ---
    #[reflect(ignore)]
    nav_path: VecDeque<Vec2>,
    nav_destination: Option<(usize, usize)>,
    // --- Idle action state ---
    #[reflect(ignore)]
    idle_timer: Option<Timer>,
}

impl GoapAgent {
    /// Creates a [`Goal::Wander`] agent with the given navigation and idle parameters.
    pub fn wander(
        nav_radius: usize,
        nav_max_path_steps: usize,
        idle_min_secs: f32,
        idle_max_secs: f32,
    ) -> Self {
        Self {
            goal: Goal::Wander,
            world_state: WorldState::default(),
            current_action: None,
            plan: VecDeque::new(),
            replan_timer: Timer::new(Duration::from_millis(500), TimerMode::Repeating),
            nav_radius,
            nav_max_path_steps,
            idle_min_secs,
            idle_max_secs,
            nav_path: VecDeque::new(),
            nav_destination: None,
            idle_timer: None,
        }
    }

    /// Starts `action`, initialising its execution state.
    fn start_action(&mut self, action: Action, rng: &mut impl Rng) {
        self.current_action = Some(action);
        match action {
            Action::NavigateToRandomPosition => {
                self.nav_path.clear();
                self.nav_destination = None;
            }
            Action::Idle => {
                let secs = rng.gen_range(self.idle_min_secs..=self.idle_max_secs);
                self.idle_timer = Some(Timer::from_seconds(secs, TimerMode::Once));
            }
        }
    }

    /// Called when [`Action::NavigateToRandomPosition`] finishes. Applies effects to world state.
    fn complete_navigate(&mut self) {
        self.world_state.at_destination = true;
        self.current_action = None;
        self.nav_path.clear();
        self.nav_destination = None;
    }

    /// Called when [`Action::Idle`] finishes. Applies effects to world state.
    fn complete_idle(&mut self) {
        self.world_state.at_destination = false;
        self.current_action = None;
        self.idle_timer = None;
    }
}

// ---------------------------------------------------------------------------
// System set
// ---------------------------------------------------------------------------

/// All GOAP systems run in this set, ordered before [`GridMoverSet`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GoapSet;

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers the GOAP component types and wires up the planning/execution loop.
pub struct GoapPlugin;

impl Plugin for GoapPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GoapAgent>()
            .register_type::<WorldState>()
            .register_type::<Goal>()
            .register_type::<Action>()
            .configure_sets(Update, GoapSet.before(GridMoverSet))
            .add_systems(
                Update,
                (
                    replan_timer_tick,
                    (execute_navigate, execute_idle),
                    advance_plan,
                )
                    .chain()
                    .in_set(GoapSet),
            );
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Returns a plan (action sequence) that satisfies `goal` given `state`.
///
/// For [`Goal::Wander`] the plan cycles naturally:
/// - If not yet at a destination, navigate first, then idle.
/// - If already at a destination (e.g. replanning mid-idle), idle first to
///   complete that rest phase before navigating again.
fn plan_for_goal(goal: Goal, state: &WorldState) -> VecDeque<Action> {
    match goal {
        Goal::Wander => {
            let mut plan = VecDeque::new();
            if state.at_destination {
                plan.push_back(Action::Idle);
            }
            plan.push_back(Action::NavigateToRandomPosition);
            plan.push_back(Action::Idle);
            plan
        }
    }
}

/// Returns `true` if `goal` can currently be satisfied from `pos` on `level`.
fn is_goal_achievable(goal: Goal, level: &LevelTiles, pos: Vec2) -> bool {
    match goal {
        // Wander requires the agent to be standing on a walkable tile.
        Goal::Wander => level
            .world_to_tile(pos)
            .map(|(x, y)| level.is_walkable(x, y))
            .unwrap_or(false),
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Ticks the replan timer and flushes the plan if the goal is no longer achievable.
///
/// Clearing the plan (without aborting the current action) lets [`advance_plan`]
/// generate a fresh plan from the updated world state on the next tick.
fn replan_timer_tick(
    time: Res<Time>,
    level: Option<Res<LevelTiles>>,
    mut query: Query<(&mut GoapAgent, &Transform)>,
) {
    let Some(level) = level else { return };

    for (mut agent, transform) in &mut query {
        agent.replan_timer.tick(time.delta());
        if !agent.replan_timer.just_finished() {
            continue;
        }

        let pos = transform.translation.truncate();
        if !is_goal_achievable(agent.goal, &level, pos) {
            agent.plan.clear();
        }
    }
}

/// Drives the [`Action::NavigateToRandomPosition`] action for all active agents.
///
/// On the first tick of this action the agent picks a random walkable tile within
/// [`GoapAgent::nav_radius`] and computes an A* path. Each subsequent tick it
/// steps toward the next waypoint via [`GridMover`]. When the path empties the
/// action completes and world state is updated (`at_destination = true`).
///
/// Dynamic obstacles (e.g. another entity blocking the next tile) trigger
/// immediate path replanning with that tile marked as extra-blocked.
fn execute_navigate(
    level: Option<Res<LevelTiles>>,
    spatial_query: SpatialQuery,
    mut query: Query<(&mut GoapAgent, &mut GridMover, &mut Sprite, &Transform)>,
) {
    let Some(level) = level else { return };
    let mut rng = thread_rng();

    for (mut agent, mut mover, mut sprite, transform) in &mut query {
        if agent.current_action != Some(Action::NavigateToRandomPosition) {
            continue;
        }

        // Wander navigation always uses walk speed.
        mover.walk();

        let world_pos = transform.translation.truncate();
        let snapped = snap_to_grid(world_pos, mover.grid_size);
        let nav_radius = agent.nav_radius;
        let nav_max_steps = agent.nav_max_path_steps;

        // Pop waypoints that have been reached.
        while let Some(&next) = agent.nav_path.front() {
            if (snapped - next).length_squared() < 0.5 * 0.5 {
                agent.nav_path.pop_front();
            } else {
                break;
            }
        }

        if agent.nav_path.is_empty() {
            if agent.nav_destination.is_some() {
                // Path exhausted after traveling — arrived at destination, action complete.
                agent.complete_navigate();
                mover.direction = None;
                continue;
            }

            // First tick of this action: pick a destination and compute a path to it.
            if let Some(start_tile) = level.world_to_tile(world_pos) {
                if let Some(dest) =
                    pick_random_walkable_in_radius(&level, start_tile, nav_radius, &mut rng)
                {
                    if let Some(tile_path) = astar(&level, start_tile, dest, &[]) {
                        for tile in tile_path.into_iter().take(nav_max_steps) {
                            agent.nav_path.push_back(level.tile_to_world(tile.0, tile.1));
                        }
                        agent.nav_destination = Some(dest);
                    }
                }
            }

            // No reachable destination found — complete action immediately.
            if agent.nav_path.is_empty() {
                agent.complete_navigate();
                mover.direction = None;
                continue;
            }
        }

        // Replan if the next step is dynamically blocked.
        if let Some(&next_wp) = agent.nav_path.front() {
            let filter = SpatialQueryFilter::default();
            if !spatial_query.point_intersections(next_wp, &filter).is_empty() {
                if let (Some(start_tile), Some(dest)) =
                    (level.world_to_tile(world_pos), agent.nav_destination)
                {
                    let blocked = level.world_to_tile(next_wp);
                    agent.nav_path.clear();
                    let tile_path = if let Some(b) = blocked {
                        astar(&level, start_tile, dest, &[b])
                    } else {
                        astar(&level, start_tile, dest, &[])
                    };
                    if let Some(tp) = tile_path {
                        for tile in tp.into_iter().take(nav_max_steps) {
                            agent.nav_path.push_back(level.tile_to_world(tile.0, tile.1));
                        }
                    }
                }
            }
        }

        // Path emptied after replanning — stuck, action complete.
        if agent.nav_path.is_empty() {
            agent.complete_navigate();
            mover.direction = None;
            continue;
        }

        // Drive GridMover toward the next waypoint.
        let direction = agent.nav_path.front().map(|&next| {
            let delta = next - snapped;
            if delta.x.abs() >= delta.y.abs() {
                if delta.x > 0.0 { IVec2::X } else { IVec2::NEG_X }
            } else if delta.y > 0.0 {
                IVec2::Y
            } else {
                IVec2::NEG_Y
            }
        });

        match direction {
            Some(d) if d == IVec2::NEG_X => sprite.flip_x = true,
            Some(d) if d == IVec2::X => sprite.flip_x = false,
            _ => {}
        }

        mover.direction = direction;
    }
}

/// Drives the [`Action::Idle`] action for all active agents.
///
/// Stops the agent's [`GridMover`] and counts down the idle timer. When the
/// timer expires the action completes (`at_destination = false`) and the agent
/// is ready to navigate again.
fn execute_idle(
    time: Res<Time>,
    mut query: Query<(&mut GoapAgent, &mut GridMover)>,
) {
    for (mut agent, mut mover) in &mut query {
        if agent.current_action != Some(Action::Idle) {
            continue;
        }

        mover.direction = None;

        let finished = agent
            .idle_timer
            .as_mut()
            .map(|t| { t.tick(time.delta()); t.just_finished() })
            .unwrap_or(true);

        if finished {
            agent.complete_idle();
        }
    }
}

/// Advances the plan when no action is running.
///
/// If the plan is empty, generates a fresh one from the current goal and world
/// state. Then pops the next action and initialises its execution state so
/// [`execute_navigate`] or [`execute_idle`] can begin work next frame.
fn advance_plan(mut query: Query<&mut GoapAgent>) {
    let mut rng = thread_rng();

    for mut agent in &mut query {
        if agent.current_action.is_some() {
            continue;
        }

        if agent.plan.is_empty() {
            agent.plan = plan_for_goal(agent.goal, &agent.world_state);
        }

        if let Some(next) = agent.plan.pop_front() {
            agent.start_action(next, &mut rng);
        }
    }
}
