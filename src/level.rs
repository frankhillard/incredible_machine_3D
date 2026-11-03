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
    pub rotation_x: f32,
    #[serde(default)]
    pub rotation_y: f32,
    #[serde(default)]
    pub rotation_z: f32,
    pub size: [f32; 3],
    pub friction: f32,
    pub restitution: f32,
    #[serde(default = "default_sticky")]
    pub sticky: bool,
}

fn default_sticky() -> bool {
    true
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
        ron::from_str(include_str!("../assets/levels/level1.ron"))
            .expect("level1.ron"),
        ron::from_str(include_str!("../assets/levels/level2.ron"))
            .expect("level2.ron"),
        
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

/// Two sticky springs that solve "Level 1".
///
/// The first sits under the loose ball. The second lies in the gap, tilted
/// down toward the landing. Tilts are the inspector degrees converted to radians.
#[cfg(test)]
pub fn level1_placements(level: &Level) -> [Placement; 2] {
    let ball = level
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == PartKind::Ball)
        .expect("level 1 starts with a loose ball");
    let z = ball.position[2];
    [
        Placement {
            kind: PartKind::Spring,
            position: [-2.7422, 2.7208, z],
            rotation_z: -28.648_f32.to_radians(),
            sticky: true,
        },
        Placement {
            kind: PartKind::Spring,
            position: [-0.0366, 2.0580, z],
            rotation_z: -17.189_f32.to_radians(),
            sticky: true,
        },
    ]
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
                rotation_x: fixture.rotation_x,
                rotation_y: fixture.rotation_y,
                rotation_z: fixture.rotation_z,
                sticky: fixture.sticky,
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
            rotation_x: 0.0,
            rotation_y: 0.0,
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
        assert_eq!(levels.len(), 5);
        assert_eq!(levels[0].name, "A Rolling Start");
        assert_eq!(levels[1].name, "Across the Gap");
        assert_eq!(levels[2].name, "Bounce the Gap");
        
        // Level 1
        assert_eq!(levels[3].name, "Level 1");
        assert_eq!(levels[3].goal.kind, PartKind::Ball);
        assert!(!levels[3].inventory.iter().any(|item| item.kind == PartKind::Ball));
        let ball = levels[3]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Ball)
            .expect("level 1 starts with a ball");
        assert!(!ball.sticky);
        assert!(levels[3]
            .fixtures
            .iter()
            .filter(|item| item.kind != PartKind::Ball)
            .all(|item| item.sticky));

        // Level 2
        assert_eq!(levels[4].name, "Level 2");
        assert_eq!(levels[4].goal.kind, PartKind::Ball);
        assert!(!levels[4].inventory.iter().any(|item| item.kind == PartKind::Ball));
        let ball = levels[4]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Ball)
            .expect("level 2 starts with a ball");
        assert!(!ball.sticky);
        assert!(levels[4]
            .fixtures
            .iter()
            .filter(|item| item.kind != PartKind::Ball)
            .all(|item| item.sticky));
        let yawed = levels[4]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Block && (item.position[0] + 1.5).abs() < 0.01)
            .expect("level 2 has a block at x=-1.5");
        assert!((yawed.rotation_y - 0.4).abs() < 1e-4);
        assert!(yawed.rotation_x.abs() < 1e-4);

        // Bounce the Gap
        assert!(levels[2].inventory.iter().any(|item| item.kind == PartKind::Spring));
        assert!(levels[0].goal.hold_secs >= 4.0);
        assert_eq!(levels[0].goal.kind, PartKind::Ball);
        assert_eq!(levels[1].goal.kind, PartKind::Ball);
        assert!(levels[0].inventory.iter().any(|item| item.kind == PartKind::Board));
        assert!(levels[1].inventory.iter().any(|item| item.kind == PartKind::Ball));
        assert!(levels[..3]
            .iter()
            .flat_map(|level| &level.fixtures)
            .all(|fixture| fixture.sticky));
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

    #[test]
    fn level_1_solution_puts_the_spring_uder_the_ball_and_the_spring_across_the_gap() {
        let level = &builtin_levels()[3];
        let [under, across] = level1_placements(level);
        let ball = level
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Ball)
            .expect("level 1 starts with a loose ball");
        assert_eq!(under.kind, PartKind::Spring);
        assert!(under.sticky);
        assert_eq!(across.kind, PartKind::Spring);
        assert!(across.sticky);

        let half = PartKind::Spring.spec().size.x * 0.5;
        let dir = Vec2::new(under.rotation_z.cos(), under.rotation_z.sin());
        let along = Vec2::new(
            ball.position[0] - under.position[0],
            ball.position[1] - under.position[1],
        )
        .dot(dir);
        assert!(
            along.abs() <= half,
            "the first spring is not under the ball, offset {along}"
        );
        assert!(under.position[1] < ball.position[1]);

        let high = level
            .fixtures
            .iter()
            .filter(|fixture| fixture.kind != PartKind::Ball)
            .max_by(|left, right| {
                let left_top = left.position[1] + left.size[1] * 0.5;
                let right_top = right.position[1] + right.size[1] * 0.5;
                left_top.total_cmp(&right_top)
            })
            .expect("level 1 needs a high platform");
        let high_right = high.position[0] + high.size[0] * 0.5;
        let landing_left = level
            .fixtures
            .iter()
            .filter(|fixture| fixture.position[0] > high_right && fixture.size[0] > fixture.size[1])
            .map(|fixture| fixture.position[0] - fixture.size[0] * 0.5)
            .min_by(|left, right| left.total_cmp(right))
            .expect("level 1 needs a landing across the gap");
        assert!(
            across.position[0] > high_right && across.position[0] < landing_left,
            "the second spring at x={} is not in the gap {high_right}..{landing_left}",
            across.position[0]
        );
    }
}
