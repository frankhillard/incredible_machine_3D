use avian3d::prelude::*;
use bevy::prelude::*;

use crate::goal::{watch_goal, GameState, GoalClock};
use crate::level::{
    configure_goal, shelf_from, spawn_level_contents, Campaign, LevelObject, LevelOps,
};
use crate::parts::Part;
use crate::scene::Palette;

#[derive(Resource, Default)]
pub struct PlaySnapshot {
    pub poses: Option<Vec<(Entity, Transform)>>,
}

#[derive(Resource, Default)]
pub struct Selection(pub Option<Entity>);

#[derive(Resource)]
pub struct Shelf {
    pub remaining: [u32; crate::parts::PartKind::ALL.len()],
    pub armed: Option<crate::parts::PartKind>,
    pub tilt: f32,
}

impl Default for Shelf {
    fn default() -> Self {
        Self {
            remaining: [0; crate::parts::PartKind::ALL.len()],
            armed: None,
            tilt: 0.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct UiPointer {
    pub blocks: bool,
    pub keys: bool,
}

pub fn pause_physics(mut time: ResMut<Time<Physics>>) {
    time.pause();
}

pub fn load_first_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<Palette>,
    mut clock: ResMut<GoalClock>,
    mut shelf: ResMut<Shelf>,
) {
    let levels = crate::level::builtin_levels();
    let level = levels[0].clone();
    configure_goal(&mut clock, &level);
    shelf.remaining = shelf_from(&level);
    shelf.armed = None;
    shelf.tilt = 0.0;
    spawn_level_contents(&mut commands, &mut meshes, &mut materials, &palette, &level);
    commands.insert_resource(Campaign { levels, index: 0 });
}

pub fn apply_level_ops(
    mut commands: Commands,
    mut ops: ResMut<LevelOps>,
    mut campaign: ResMut<Campaign>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<Palette>,
    mut clock: ResMut<GoalClock>,
    mut shelf: ResMut<Shelf>,
    mut selection: ResMut<Selection>,
    mut snapshot: ResMut<PlaySnapshot>,
    mut physics: ResMut<Time<Physics>>,
    mut next: ResMut<NextState<GameState>>,
    objects: Query<Entity, With<LevelObject>>,
) {
    let go_to = ops.go_to.take();
    let reload = std::mem::take(&mut ops.reload);
    let Some(index) = go_to.or(reload.then_some(campaign.index)) else {
        return;
    };
    let index = index % campaign.levels.len();
    campaign.index = index;
    snapshot.poses = None;
    selection.0 = None;
    for entity in &objects {
        commands.entity(entity).despawn();
    }
    let level = campaign.levels[index].clone();
    configure_goal(&mut clock, &level);
    shelf.remaining = shelf_from(&level);
    shelf.armed = None;
    shelf.tilt = 0.0;
    spawn_level_contents(&mut commands, &mut meshes, &mut materials, &palette, &level);
    physics.pause();
    next.set(GameState::Editing);
}

pub fn enter_editing(
    mut physics: ResMut<Time<Physics>>,
    mut snapshot: ResMut<PlaySnapshot>,
    mut clock: ResMut<GoalClock>,
    mut parts: Query<
        (
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
            Option<&mut LinearVelocity>,
            Option<&mut AngularVelocity>,
        ),
        Or<(With<Part>, With<crate::parts::ChainLink>)>,
    >,
) {
    physics.pause();
    clock.held = 0.0;
    let Some(saved) = snapshot.poses.take() else {
        return;
    };
    for (entity, pose) in saved {
        let Ok((mut transform, position, rotation, linear, angular)) = parts.get_mut(entity) else {
            continue;
        };
        write_pose(&mut transform, position, rotation, pose);
        if let Some(mut linear) = linear {
            *linear = LinearVelocity::ZERO;
        }
        if let Some(mut angular) = angular {
            *angular = AngularVelocity::ZERO;
        }
    }
}

pub fn enter_playing(
    mut commands: Commands,
    mut physics: ResMut<Time<Physics>>,
    mut snapshot: ResMut<PlaySnapshot>,
    mut clock: ResMut<GoalClock>,
    mut parts: Query<
        (
            Entity,
            &Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Or<(With<Part>, With<crate::parts::ChainLink>)>,
    >,
) {
    physics.unpause();
    clock.held = 0.0;
    let mut poses = Vec::new();
    for (entity, transform, mut position, mut rotation) in &mut parts {
        if let Some(position) = position.as_mut() {
            position.0 = transform.translation;
        }
        if let Some(rotation) = rotation.as_mut() {
            rotation.0 = transform.rotation;
        }
        commands.entity(entity).remove::<Sleeping>();
        poses.push((entity, *transform));
    }
    snapshot.poses = Some(poses);
}

pub fn write_pose(
    transform: &mut Transform,
    mut position: Option<Mut<Position>>,
    mut rotation: Option<Mut<Rotation>>,
    pose: Transform,
) {
    *transform = pose;
    if let Some(position) = position.as_mut() {
        position.0 = pose.translation;
    }
    if let Some(rotation) = rotation.as_mut() {
        rotation.0 = pose.rotation;
    }
}

pub fn add_sim_systems(app: &mut App) {
    app.init_state::<GameState>()
        .insert_resource(Gravity(Vec3::NEG_Y * 9.81))
        // A rope is a chain of joints. The default six substeps let the links
        // drift apart; twelve keeps the sockets and the end distances tight.
        .insert_resource(SubstepCount(12))
        .init_resource::<PlaySnapshot>()
        .init_resource::<Selection>()
        .init_resource::<Shelf>()
        .init_resource::<GoalClock>()
        .init_resource::<LevelOps>()
        .init_resource::<UiPointer>()
        .add_systems(
            Startup,
            (
                crate::scene::setup_palette,
                crate::scene::spawn_frame,
                load_first_level,
                pause_physics,
            )
                .chain(),
        )
        .add_systems(OnEnter(GameState::Editing), enter_editing)
        .add_systems(OnEnter(GameState::Playing), enter_playing)
        .add_systems(
            Update,
            (
                apply_level_ops,
                watch_goal.run_if(in_state(GameState::Playing)),
            ),
        );
}

/// Removes one player part and gives it back to the shelf. Used by the property panel.
pub fn refund(shelf: &mut Shelf, part: &Part) {
    shelf.remaining[part.kind.slot()] += 1;
}
