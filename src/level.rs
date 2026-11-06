use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::goal::{GoalClock, GoalZone};
use crate::parts::{pin_rope_end, spawn_part, spawn_rope_between, PartKind, PartSpec, SpawnPart};

pub const LANE_Z: f32 = 0.25;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Level {
    pub name: String,
    pub hint: String,
    pub goal: GoalDef,
    pub fixtures: Vec<FixtureDef>,
    pub inventory: Vec<InventoryDef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FixtureDef {
    pub kind: PartKind,
    /// Center of a normal part. Omitted for a rope, which is placed by [`Self::ends`].
    #[serde(default, skip_serializing_if = "is_zero_array")]
    pub position: [f32; 3],
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub rotation_x: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub rotation_y: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub rotation_z: f32,
    /// The two end-link positions of a rope. Length and direction follow this
    /// segment; `size.y` is still the rope diameter. Left at the origin when
    /// the part is not a rope.
    #[serde(default, skip_serializing_if = "is_zero_ends")]
    pub ends: [[f32; 3]; 2],
    pub size: [f32; 3],
    pub friction: f32,
    pub restitution: f32,
    #[serde(default = "default_sticky", skip_serializing_if = "is_default_sticky")]
    pub sticky: bool,
}

fn default_sticky() -> bool {
    true
}

fn is_zero_f32(value: &f32) -> bool {
    value.abs() < 1e-6
}

fn is_zero_array(value: &[f32; 3]) -> bool {
    value.iter().all(is_zero_f32)
}

fn is_zero_ends(value: &[[f32; 3]; 2]) -> bool {
    value.iter().all(is_zero_array)
}

fn is_default_sticky(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryDef {
    pub kind: PartKind,
    pub count: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub kind: PartKind,
    pub position: [f32; 3],
    pub rotation_x: f32,
    pub rotation_y: f32,
    pub rotation_z: f32,
    pub sticky: bool,
    /// The two end-link positions of a rope. Other parts leave this at the origin.
    pub ends: [[f32; 3]; 2],
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
    pub save: bool,
}

pub fn builtin_levels() -> Vec<Level> {
    vec![
        ron::from_str(include_str!("../assets/levels/rolling_start.ron"))
            .expect("rolling_start.ron"),
        ron::from_str(include_str!("../assets/levels/across_the_gap.ron"))
            .expect("across_the_gap.ron"),
        ron::from_str(include_str!("../assets/levels/bounce_the_gap.ron"))
            .expect("bounce_the_gap.ron"),
        ron::from_str(include_str!("../assets/levels/level1.ron")).expect("level1.ron"),
        ron::from_str(include_str!("../assets/levels/level2.ron")).expect("level2.ron"),
        ron::from_str(include_str!("../assets/levels/level3.ron")).expect("level3.ron"),
        ron::from_str(include_str!("../assets/levels/level4.ron")).expect("level4.ron"),
        ron::from_str(include_str!("../assets/levels/level5.ron")).expect("level5.ron"),
        ron::from_str(include_str!("../assets/levels/level6.ron")).expect("level6.ron"),
        ron::from_str(include_str!("../assets/levels/level_11.ron")).expect("level_11.ron"),
        ron::from_str(include_str!("../assets/levels/level_12.ron")).expect("level_12.ron"),
        ron::from_str(include_str!("../assets/levels/level_13.ron")).expect("level_13.ron"),
        ron::from_str(include_str!("../assets/levels/level_14.ron")).expect("level_14.ron"),
        ron::from_str(include_str!("../assets/levels/level_15.ron")).expect("level_15.ron"),
        ron::from_str(include_str!("../assets/levels/level_16.ron")).expect("level_16.ron"),
        ron::from_str(include_str!("../assets/levels/level_17.ron")).expect("level_17.ron"),
        ron::from_str(include_str!("../assets/levels/level_18.ron")).expect("level_18.ron"),
        ron::from_str(include_str!("../assets/levels/level_19.ron")).expect("level_19.ron"),
        ron::from_str(include_str!("../assets/levels/level_20.ron")).expect("level_20.ron"),
        ron::from_str(include_str!("../assets/levels/level_21.ron")).expect("level_21.ron"),
    ]
}

/// The level as it stands in edit mode: the original pieces, the parts you
/// placed, and the counts still on the shelf.
pub fn snapshot_level(
    level: &Level,
    remaining: &[u32],
    placed: impl IntoIterator<Item = FixtureDef>,
) -> Level {
    let mut fixtures = level.fixtures.clone();
    fixtures.extend(placed);
    let inventory = PartKind::ALL
        .into_iter()
        .filter(|kind| remaining.get(kind.slot()).copied().unwrap_or(0) > 0)
        .map(|kind| InventoryDef {
            kind,
            count: remaining[kind.slot()],
        })
        .collect();
    Level {
        name: level.name.clone(),
        hint: level.hint.clone(),
        goal: level.goal.clone(),
        fixtures,
        inventory,
    }
}

pub fn level_ron(level: &Level) -> Result<String, String> {
    ron::ser::to_string_pretty(level, ron::ser::PrettyConfig::default())
        .map_err(|err| err.to_string())
}

/// A shelf part written the same way a level file describes a fixture.
pub fn fixture_from_placement(
    kind: PartKind,
    sticky: bool,
    translation: Vec3,
    rotation: Quat,
    far_end: Option<Vec3>,
) -> FixtureDef {
    let spec = kind.spec();
    let (rotation_x, rotation_y, rotation_z) = rotation.to_euler(EulerRot::XYZ);
    let rope = kind == PartKind::Rope;
    let ends = match far_end {
        Some(far) if rope => [translation.to_array(), far.to_array()],
        _ => [[0.0; 3]; 2],
    };
    FixtureDef {
        kind,
        position: if rope && far_end.is_some() {
            [0.0; 3]
        } else {
            translation.to_array()
        },
        rotation_x: if rope && far_end.is_some() {
            0.0
        } else {
            rotation_x
        },
        rotation_y: if rope && far_end.is_some() {
            0.0
        } else {
            rotation_y
        },
        rotation_z: if rope && far_end.is_some() {
            0.0
        } else {
            rotation_z
        },
        ends,
        size: spec.size.to_array(),
        friction: spec.friction,
        restitution: spec.restitution,
        sticky,
    }
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
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: -28.648_f32.to_radians(),
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Spring,
            position: [-0.0366, 2.0580, z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: -17.189_f32.to_radians(),
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
    ]
}

/// Two sticky springs and two boards that solve "Level 2".
///
/// The first spring sits under the loose ball. The two boards lies in the gap, tilted
/// toward the landing. The last spring to reach the landing zone. Tilts are the inspector degrees converted to radians.
#[cfg(test)]
pub fn level2_placements(level: &Level) -> [Placement; 4] {
    let ball = level
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == PartKind::Ball)
        .expect("level 2 starts with a loose ball");
    let _z = ball.position[2];
    [
        Placement {
            kind: PartKind::Spring,
            position: [-2.5349, 2.6268, 0.3209],
            rotation_x: -10.762_f32.to_radians(),
            rotation_y: 2.743_f32.to_radians(),
            rotation_z: -28.527_f32.to_radians(),
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Board,
            position: [-0.0610, 2.0860, 0.8006],
            rotation_x: -5.730_f32.to_radians(),
            rotation_y: 0.0,
            rotation_z: -11.459_f32.to_radians(),
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Board,
            position: [1.1892, 1.9591, 0.8101],
            rotation_x: -5.730_f32.to_radians(),
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Spring,
            position: [2.3281, 1.88, 0.34],
            rotation_x: 22.556_f32.to_radians(),
            rotation_y: 2.817_f32.to_radians(),
            rotation_z: 16.944_f32.to_radians(),
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
    ]
}

#[cfg(test)]
pub fn level3_placements(level: &Level) -> [Placement; 2] {
    let ball = level
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == PartKind::Ball)
        .expect("level 3 starts with a loose ball");
    let z = ball.position[2];
    [
        Placement {
            kind: PartKind::Spring,
            position: [-2.5138, 2.6984, z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: -0.2,
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Spring,
            position: [0.1527, 1.8398, z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
    ]
}

/// The rope that ties the high anchor to the loose weight in "Level 4".
#[cfg(test)]
pub fn level4_placements(level: &Level) -> [Placement; 2] {
    let ball = level
        .fixtures
        .iter()
        .find(|fixture| fixture.kind == PartKind::Ball)
        .expect("level 4 starts with a loose ball");
    let z = ball.position[2];
    [
        Placement {
            kind: PartKind::Weight,
            position: [-3.2014, 4.900, z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: false,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Rope,
            position: [0.0, 0.0, z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: false,
            ends: [[-2.5151, 5.3935, z], [-3.2014, 4.900, z]],
        },
    ]
}

/// Board and ball that solve "A Rolling Start", derived from that level's ramp and landing.
#[cfg(test)]
pub fn rolling_start_placements(level: &Level) -> [Placement; 2] {
    let ramp = level
        .fixtures
        .iter()
        .max_by(|left, right| left.rotation_z.abs().total_cmp(&right.rotation_z.abs()))
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
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: board_angle,
            sticky: true,
            ends: [[0.0; 3]; 2],
        },
        Placement {
            kind: PartKind::Ball,
            position: [ball_center.x, ball_center.y, ramp.position[2]],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: false,
            ends: [[0.0; 3]; 2],
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
    let mut hosts = Vec::new();
    let mut ropes = Vec::new();
    for fixture in &level.fixtures {
        let spec = spec_for_fixture(fixture);
        if fixture.kind == PartKind::Rope && fixture.ends != [[0.0; 3]; 2] {
            let near = Vec3::from_array(fixture.ends[0]);
            let far = Vec3::from_array(fixture.ends[1]);
            let (root, far_link) =
                spawn_rope_between(commands, meshes, palette, &spec, near, far, false);
            ropes.push(AuthoredRope {
                root,
                far_link,
                near,
                far,
            });
            continue;
        }
        let entity = spawn_part(
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
        hosts.push(AnchorHost {
            entity,
            position: Vec3::from_array(fixture.position),
            rotation: Quat::from_euler(
                EulerRot::XYZ,
                fixture.rotation_x,
                fixture.rotation_y,
                fixture.rotation_z,
            ),
            half: Vec3::from_array(fixture.size) * 0.5,
            ball: fixture.kind.round(),
        });
    }
    for rope in ropes {
        pin_authored_end(commands, rope.root, rope.root, rope.near, &hosts);
        pin_authored_end(commands, rope.root, rope.far_link, rope.far, &hosts);
    }

    let half = Vec3::from_array(level.goal.half_extents);
    commands.spawn((
        LevelObject,
        GoalZone { half_extents: half },
        Mesh3d(meshes.add(Cuboid::new(half.x * 2.0, half.y * 2.0, half.z * 2.0))),
        MeshMaterial3d(palette.goal.clone()),
        Transform::from_translation(Vec3::from_array(level.goal.center)),
    ));
}

struct AnchorHost {
    entity: Entity,
    position: Vec3,
    rotation: Quat,
    half: Vec3,
    ball: bool,
}

struct AuthoredRope {
    root: Entity,
    far_link: Entity,
    near: Vec3,
    far: Vec3,
}

fn pin_authored_end(
    commands: &mut Commands,
    root: Entity,
    end: Entity,
    point: Vec3,
    hosts: &[AnchorHost],
) {
    let Some(host) = hosts
        .iter()
        .filter(|host| fixture_contains(host, point))
        .min_by(|left, right| {
            left.position
                .distance_squared(point)
                .partial_cmp(&right.position.distance_squared(point))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    else {
        return;
    };
    let local = host.rotation.inverse() * (point - host.position);
    pin_rope_end(commands, root, end, host.entity, local);
}

fn fixture_contains(host: &AnchorHost, point: Vec3) -> bool {
    let local = host.rotation.inverse() * (point - host.position);
    let slop = 0.02;
    if host.ball {
        local.length() <= host.half.x + slop
    } else {
        local.x.abs() <= host.half.x + slop
            && local.y.abs() <= host.half.y + slop
            && local.z.abs() <= host.half.z + slop
    }
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
    if placement.kind == PartKind::Rope && placement.ends != [[0.0; 3]; 2] {
        let near = Vec3::from_array(placement.ends[0]);
        let far = Vec3::from_array(placement.ends[1]);
        let (root, _) = spawn_rope_between(commands, meshes, palette, &spec, near, far, true);
        return root;
    }
    spawn_part(
        commands,
        meshes,
        materials,
        palette,
        &SpawnPart {
            spec: &spec,
            position: Vec3::from_array(placement.position),
            rotation_x: placement.rotation_x,
            rotation_y: placement.rotation_y,
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
    fn a_saved_level_keeps_the_goal_the_shelf_and_the_parts_you_placed() {
        let level = &builtin_levels()[0];
        let placed = fixture_from_placement(
            PartKind::Spring,
            true,
            Vec3::new(1.25, 2.5, 0.25),
            Quat::from_rotation_z(-0.2),
            None,
        );
        let rope = fixture_from_placement(
            PartKind::Rope,
            false,
            Vec3::new(-1.0, 4.0, 0.25),
            Quat::IDENTITY,
            Some(Vec3::new(1.0, 3.0, 0.25)),
        );
        let mut remaining = empty_shelf();
        remaining[PartKind::Board.slot()] = 2;
        let snapshot = snapshot_level(level, &remaining, [placed, rope]);
        let text = level_ron(&snapshot).expect("ron");
        let parsed: Level = ron::from_str(&text).expect("saved level should load");
        assert_eq!(parsed.name, level.name);
        assert_eq!(parsed.goal.center, level.goal.center);
        assert_eq!(parsed.fixtures.len(), level.fixtures.len() + 2);
        let spring = parsed
            .fixtures
            .iter()
            .find(|fixture| {
                fixture.kind == PartKind::Spring && (fixture.position[0] - 1.25).abs() < 1e-3
            })
            .expect("placed spring");
        assert!((spring.position[0] - 1.25).abs() < 1e-4);
        assert!(spring.sticky);
        assert!((spring.size[0] - PartKind::Spring.spec().size.x).abs() < 1e-4);
        let saved_rope = parsed
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Rope)
            .expect("placed rope");
        assert_eq!(saved_rope.position, [0.0; 3]);
        assert!((saved_rope.ends[0][0] + 1.0).abs() < 1e-4);
        assert!((saved_rope.ends[1][0] - 1.0).abs() < 1e-4);
        assert!(!saved_rope.sticky);
        assert!(parsed
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Board && item.count == 2));
    }

    #[test]
    fn both_level_files_parse() {
        let levels = builtin_levels();
        assert_eq!(levels.len(), 9);
        assert_eq!(levels[0].name, "A Rolling Start");
        assert_eq!(levels[1].name, "Across the Gap");
        assert_eq!(levels[2].name, "Bounce the Gap");

        // Level 1
        assert_eq!(levels[3].name, "Level 1");
        assert_eq!(levels[3].goal.kind, PartKind::Ball);
        assert!(!levels[3]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Ball));
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
        assert!(!levels[4]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Ball));
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
        let pitched = levels[4]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Block && (item.position[0] + 1.5).abs() < 0.01)
            .expect("level 2 has a block at x=-1.5");
        assert!((pitched.rotation_x - 0.1).abs() < 1e-4);
        assert!(pitched.rotation_y.abs() < 1e-4);

        // Level 3
        assert_eq!(levels[5].name, "Level 3");
        assert_eq!(levels[5].goal.kind, PartKind::Ball);
        assert!(!levels[5]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Ball));
        let ball = levels[5]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Ball)
            .expect("level 3 starts with a ball");
        assert!(!ball.sticky);
        let plank = levels[5]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Plank)
            .expect("level 3 has a loose plank");
        assert!(!plank.sticky);
        assert!((plank.rotation_z + 0.3).abs() < 1e-4);
        assert!(levels[5]
            .fixtures
            .iter()
            .filter(|item| {
                item.kind != PartKind::Ball
                    && item.kind != PartKind::Rope
                    && item.kind != PartKind::Plank
            })
            .all(|item| item.sticky));
        let rope = levels[5]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Rope)
            .expect("level 3 starts with a rope");
        assert_eq!(rope.ends[0], [-2.5151, 5.3935, 0.25]);
        assert_eq!(rope.ends[1], [-3.7, 4.6268, 0.25]);
        assert_eq!(rope.rotation_x, 0.0);
        assert_eq!(rope.rotation_y, 0.0);
        assert_eq!(rope.rotation_z, 0.0);

        // Level 4
        assert_eq!(levels[6].name, "Level 4");
        assert_eq!(levels[6].goal.kind, PartKind::Ball);
        let ball = levels[6]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Ball)
            .expect("level 4 starts with a ball");
        assert!(!ball.sticky);
        assert!(levels[6]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Weight));
        assert!(levels[6]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Rope));
        assert!(levels[6]
            .fixtures
            .iter()
            .filter(|item| item.kind != PartKind::Ball)
            .all(|item| item.sticky));

        // Level 5
        assert_eq!(levels[7].name, "Level 5");
        assert_eq!(levels[7].goal.kind, PartKind::Ball);
        let weight = levels[7]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Weight)
            .expect("level 5 starts with a weight");
        assert!(!weight.sticky);
        assert!(weight.kind.round());
        let rope = levels[7]
            .fixtures
            .iter()
            .find(|item| item.kind == PartKind::Rope)
            .expect("level 5 starts with a rope");
        assert_eq!(rope.ends[0], [-2.5151, 5.3935, 0.25]);
        assert_eq!(rope.ends[1], [-3.2014, 4.900, 0.25]);

        // Level 6
        assert_eq!(levels[8].name, "Level 6");
        assert!(levels[8]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Spring && item.count == 3));

        // Bounce the Gap
        assert!(levels[2]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Spring));
        assert!(levels[0].goal.hold_secs >= 4.0);
        assert_eq!(levels[0].goal.kind, PartKind::Ball);
        assert_eq!(levels[1].goal.kind, PartKind::Ball);
        assert!(levels[0]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Board));
        assert!(levels[1]
            .inventory
            .iter()
            .any(|item| item.kind == PartKind::Ball));
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

    #[test]
    fn level_2_solution_puts_the_spring_uder_the_ball_and_the_spring_and_boards_across_the_gap() {
        let level = &builtin_levels()[4];
        let [under, first_board, second_board, landing_spring] = level2_placements(level);
        let ball = level
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Ball)
            .expect("level 2 starts with a loose ball");
        assert_eq!(under.kind, PartKind::Spring);
        assert!(under.sticky);
        assert_eq!(first_board.kind, PartKind::Board);
        assert!(first_board.sticky);
        assert_eq!(second_board.kind, PartKind::Board);
        assert!(second_board.sticky);
        assert_eq!(landing_spring.kind, PartKind::Spring);
        assert!(landing_spring.sticky);

        let axis = Quat::from_euler(
            EulerRot::XYZ,
            under.rotation_x,
            under.rotation_y,
            under.rotation_z,
        ) * Vec3::X;
        let along = (Vec3::from_array(ball.position) - Vec3::from_array(under.position)).dot(axis);
        let half = PartKind::Spring.spec().size.x * 0.5;
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
            .expect("level 2 needs a high platform");
        let high_right = high.position[0] + high.size[0] * 0.5;
        let landing_left = level
            .fixtures
            .iter()
            .filter(|fixture| fixture.position[0] > high_right && fixture.size[0] > fixture.size[1])
            .map(|fixture| fixture.position[0] - fixture.size[0] * 0.5)
            .min_by(|left, right| left.total_cmp(right))
            .expect("level 2 needs a landing across the gap");
        assert!(
            first_board.position[0] > high_right && first_board.position[0] < landing_left,
            "the first board at x={} is not in the gap {high_right}..{landing_left}",
            first_board.position[0]
        );
        assert!(second_board.position[0] > first_board.position[0]);
        assert!(landing_spring.position[0] > second_board.position[0]);
        assert!(
            landing_spring.position[0] > landing_left,
            "the last spring at x={} does not reach the landing",
            landing_spring.position[0]
        );
    }

    #[test]
    fn level_3_solution_puts_the_spring_under_the_ball_and_the_spring_across_the_gap() {
        let level = &builtin_levels()[5];
        let [under, across] = level3_placements(level);
        let ball = level
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Ball)
            .expect("level 3 starts with a loose ball");
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
            .filter(|fixture| {
                fixture.kind != PartKind::Ball
                    && fixture.kind != PartKind::Rope
                    && fixture.kind != PartKind::Spring
                    && fixture.size[0] > fixture.size[1]
                    && ball.position[0] >= fixture.position[0] - fixture.size[0] * 0.5
                    && ball.position[0] <= fixture.position[0] + fixture.size[0] * 0.5
            })
            .max_by(|left, right| {
                let left_top = left.position[1] + left.size[1] * 0.5;
                let right_top = right.position[1] + right.size[1] * 0.5;
                left_top.total_cmp(&right_top)
            })
            .expect("level 3 needs a high platform");
        let high_right = high.position[0] + high.size[0] * 0.5;
        let landing_left = level
            .fixtures
            .iter()
            .filter(|fixture| {
                fixture.kind != PartKind::Rope
                    && fixture.position[0] > high_right
                    && fixture.size[0] > fixture.size[1]
            })
            .map(|fixture| fixture.position[0] - fixture.size[0] * 0.5)
            .min_by(|left, right| left.total_cmp(right))
            .expect("level 3 needs a landing across the gap");
        assert!(
            across.position[0] > high_right && across.position[0] < landing_left,
            "the spring at x={} is not in the gap {high_right}..{landing_left}",
            across.position[0]
        );
    }

    #[test]
    fn level_4_solution_ties_the_rope_between_the_anchor_and_the_weight() {
        let level = &builtin_levels()[6];
        let [weight, rope] = level4_placements(level);
        assert_eq!(weight.kind, PartKind::Weight);
        assert!(!weight.sticky);
        assert_eq!(rope.kind, PartKind::Rope);
        assert!(!rope.sticky);
        let anchor = level
            .fixtures
            .iter()
            .find(|fixture| {
                fixture.kind == PartKind::Block && fixture.size[0] < 0.2 && fixture.size[1] < 0.2
            })
            .expect("level 4 has a small anchor block");
        assert_eq!(rope.ends[0], anchor.position);
        assert_eq!(rope.ends[1], weight.position);
    }
}
