use bevy::prelude::*;
use serde::Deserialize;

use crate::goal::{GoalClock, GoalZone};
use crate::parts::{spawn_part, PartKind, PartSpec, SpawnPart};

pub const LANE_Z: f32 = 0.25;

#[derive(Clone, Debug, Deserialize)]
pub struct Level {
    pub name: String,
    pub hint: String,
    pub goal: GoalDef,
    pub fixtures: Vec<FixtureDef>,
    pub inventory: Vec<InventoryDef>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GoalDef {
    pub kind: PartKind,
    #[serde(default = "default_hold")]
    pub hold_secs: f32,
    #[serde(default = "default_speed")]
    pub max_speed: f32,
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
}

fn default_hold() -> f32 {
    4.0
}

fn default_speed() -> f32 {
    0.8
}

#[derive(Clone, Debug, Deserialize)]
pub struct FixtureDef {
    pub kind: PartKind,
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation_z: f32,
    pub size: [f32; 3],
    pub friction: f32,
    pub restitution: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InventoryDef {
    pub kind: PartKind,
    pub count: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub kind: PartKind,
    pub position: [f32; 3],
    pub rotation_z: f32,
    pub sticky: bool,
}

#[derive(Component)]
pub struct LevelObject;

#[derive(Resource)]
pub struct Campaign {
    pub levels: Vec<Level>,
    pub index: usize,
}

impl Campaign {
    pub fn current(&self) -> &Level {
        &self.levels[self.index]
    }
}

#[derive(Resource, Default)]
pub struct LevelOps {
    pub reload: bool,
    pub go_to: Option<usize>,
}

pub fn builtin_levels() -> Vec<Level> {
    vec![
        ron::from_str(include_str!("../assets/levels/rolling_start.ron"))
            .expect("rolling_start.ron"),
        ron::from_str(include_str!("../assets/levels/across_the_gap.ron"))
            .expect("across_the_gap.ron"),
        ron::from_str(include_str!("../assets/levels/bounce_the_gap.ron"))
            .expect("bounce_the_gap.ron"),
    ]
}

pub fn empty_shelf() -> [u32; PartKind::ALL.len()] {
    [0; PartKind::ALL.len()]
}

pub fn shelf_from(level: &Level) -> [u32; PartKind::ALL.len()] {
    let mut remaining = empty_shelf();
    for item in &level.inventory {
        remaining[item.kind.slot()] += item.count;
    }
    remaining
}

/// Board and ball that solve "A Rolling Start", derived from that level's ramp and landing.
#[cfg(test)]
pub fn rolling_start_placements(level: &Level) -> [Placement; 2] {
    let ramp = level
        .fixtures
        .iter()
        .max_by(|left, right| {
            left.rotation_z
                .abs()
                .total_cmp(&right.rotation_z.abs())
        })
        .expect("rolling start needs a tilted ramp");
    let platform = level
        .fixtures
        .iter()
        .filter(|fixture| fixture.rotation_z.abs() < 0.01 && fixture.size[0] > fixture.size[1])
        .max_by(|left, right| {
            let left_top = left.position[1] + left.size[1] * 0.5;
            let right_top = right.position[1] + right.size[1] * 0.5;
            left_top.total_cmp(&right_top)
        })
        .expect("rolling start needs a landing");

    let ramp_size = ramp.size;
    let platform_size = platform.size;
    let angle = ramp.rotation_z;
    let along = Vec2::new(angle.cos(), angle.sin());
    let normal = Vec2::new(-angle.sin(), angle.cos());
    let half_len = ramp_size[0] * 0.5;
    let half_h = ramp_size[1] * 0.5;
    let center = Vec2::new(ramp.position[0], ramp.position[1]);
    let downhill = center + along * half_len;
    let uphill = center - along * half_len;
    let ramp_lip = downhill + normal * half_h;
    let landing = Vec2::new(
        platform.position[0] - platform_size[0] * 0.5 + 0.2,
        platform.position[1] + platform_size[1] * 0.5,
    );
    let delta = landing - ramp_lip;
    let chord = delta.length();
    let dir = delta / chord;
    let board = PartKind::Board.spec();
    let board_len = board.size.x;
    let overlap = 0.08;
    assert!(
        board_len - overlap > chord + 0.12,
        "the shelf board ({board_len} m) cannot bridge this gap ({chord} m)"
    );
    let board_angle = dir.y.atan2(dir.x);
    let board_normal = Vec2::new(-board_angle.sin(), board_angle.cos());
    let along_board = board_len * 0.5 - overlap;
    let top_center = ramp_lip + dir * along_board;
    let board_center = top_center - board_normal * (board.size.y * 0.5);
    let ball = PartKind::Ball.spec();
    let radius = ball.size.x * 0.5;
    let ball_center = uphill + normal * (half_h + radius) + along * 0.4;

    [
        Placement {
            kind: PartKind::Board,
            position: [board_center.x, board_center.y, ramp.position[2]],
            rotation_z: board_angle,
            sticky: true,
        },
        Placement {
            kind: PartKind::Ball,
            position: [ball_center.x, ball_center.y, ramp.position[2]],
            rotation_z: 0.0,
            sticky: false,
        },
    ]
}

pub fn configure_goal(clock: &mut GoalClock, level: &Level) {
    clock.held = 0.0;
    clock.required = level.goal.hold_secs;
    clock.max_speed = level.goal.max_speed;
    clock.kind = level.goal.kind;
}

pub fn spawn_level_contents(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    palette: &crate::scene::Palette,
    level: &Level,
) {
    for fixture in &level.fixtures {
        let spec = spec_for_fixture(fixture);
        spawn_part(
            commands,
            meshes,
            materials,
            palette,
            &SpawnPart {
                spec: &spec,
                position: Vec3::from_array(fixture.position),
                rotation_z: fixture.rotation_z,
                sticky: true,
                player: false,
                ghost: false,
            },
        );
    }

    let half = Vec3::from_array(level.goal.half_extents);
    commands.spawn((
        LevelObject,
        GoalZone {
            half_extents: half,
        },
        Mesh3d(meshes.add(Cuboid::new(half.x * 2.0, half.y * 2.0, half.z * 2.0))),
        MeshMaterial3d(palette.goal.clone()),
        Transform::from_translation(Vec3::from_array(level.goal.center)),
    ));
}

fn spec_for_fixture(fixture: &FixtureDef) -> PartSpec {
    let mut spec = fixture.kind.spec();
    spec.size = Vec3::from_array(fixture.size);
    spec.friction = fixture.friction;
    spec.restitution = fixture.restitution;
    spec
}

pub fn spawn_player_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    palette: &crate::scene::Palette,
    placement: Placement,
) -> Entity {
    let spec = placement.kind.spec();
    spawn_part(
        commands,
        meshes,
        materials,
        palette,
        &SpawnPart {
            spec: &spec,
            position: Vec3::from_array(placement.position),
            rotation_z: placement.rotation_z,
            sticky: placement.sticky,
            player: true,
            ghost: false,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_level_files_parse() {
        let levels = builtin_levels();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].name, "A Rolling Start");
        assert_eq!(levels[1].name, "Across the Gap");
        assert_eq!(levels[2].name, "Bounce the Gap");
        assert!(levels[2].inventory.iter().any(|item| item.kind == PartKind::Spring));
        assert!(levels[0].goal.hold_secs >= 4.0);
        assert_eq!(levels[0].goal.kind, PartKind::Ball);
        assert_eq!(levels[1].goal.kind, PartKind::Ball);
        assert!(levels[0].inventory.iter().any(|item| item.kind == PartKind::Board));
        assert!(levels[1].inventory.iter().any(|item| item.kind == PartKind::Ball));
    }

    #[test]
    fn rolling_start_solution_puts_the_ball_on_the_ramp_and_the_board_across_the_gap() {
        let level = &builtin_levels()[0];
        let [board, ball] = rolling_start_placements(level);
        assert_eq!(board.kind, PartKind::Board);
        assert!(board.sticky);
        assert_eq!(ball.kind, PartKind::Ball);
        assert!(!ball.sticky);
        assert!(ball.position[0] < -1.0, "ball x {}", ball.position[0]);
        assert!(ball.position[1] > 2.5, "ball y {}", ball.position[1]);
        assert!(board.position[0] > ball.position[0]);
        let basket_left = 2.2 - 1.3 * 0.5;
        let board_reach = {
            let dir = Vec2::new(board.rotation_z.cos(), board.rotation_z.sin());
            let half = PartKind::Board.spec().size.x * 0.5;
            board.position[0] + dir.x * half
        };
        assert!(
            board_reach < basket_left,
            "board reaches x={board_reach}, basket starts at {basket_left}"
        );
    }
}
