use avian3d::prelude::*;
use bevy::prelude::*;

use crate::level::LevelObject;
use crate::scene::Palette;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Board,
    Block,
    Plank,
    Ball,
    Spring,
}

impl PartKind {
    pub const ALL: [PartKind; 5] = [
        PartKind::Board,
        PartKind::Block,
        PartKind::Plank,
        PartKind::Ball,
        PartKind::Spring,
    ];

    pub fn slot(self) -> usize {
        match self {
            PartKind::Board => 0,
            PartKind::Block => 1,
            PartKind::Plank => 2,
            PartKind::Ball => 3,
            PartKind::Spring => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PartKind::Board => "Board",
            PartKind::Block => "Block",
            PartKind::Plank => "Plank",
            PartKind::Ball => "Ball",
            PartKind::Spring => "Spring",
        }
    }

    pub fn spec(self) -> PartSpec {
        match self {
            PartKind::Board => PartSpec {
                kind: self,
                size: Vec3::new(1.55, 0.12, 0.5),
                friction: 0.18,
                restitution: 0.02,
                mass: 2.2,
                sticky_default: true,
                linear_damping: 0.05,
                angular_damping: 0.4,
            },
            PartKind::Block => PartSpec {
                kind: self,
                size: Vec3::new(0.55, 0.55, 0.5),
                friction: 0.55,
                restitution: 0.02,
                mass: 2.0,
                sticky_default: true,
                linear_damping: 0.08,
                angular_damping: 0.5,
            },
            PartKind::Plank => PartSpec {
                kind: self,
                size: Vec3::new(1.4, 0.08, 0.42),
                friction: 0.4,
                restitution: 0.04,
                mass: 1.1,
                sticky_default: false,
                linear_damping: 0.08,
                angular_damping: 0.35,
            },
            PartKind::Ball => PartSpec {
                kind: self,
                size: Vec3::splat(0.32),
                friction: 0.16,
                restitution: 0.05,
                mass: 0.7,
                sticky_default: false,
                linear_damping: 0.04,
                angular_damping: 0.2,
            },
            PartKind::Spring => PartSpec {
                kind: self,
                size: Vec3::new(0.5, 0.12, 0.5),
                friction: 0.2,
                restitution: 0.92,
                mass: 2.0,
                sticky_default: true,
                linear_damping: 0.08,
                angular_damping: 0.5,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct PartSpec {
    pub kind: PartKind,
    /// Full box size. For a ball, every component is the diameter.
    pub size: Vec3,
    pub friction: f32,
    pub restitution: f32,
    pub mass: f32,
    pub sticky_default: bool,
    pub linear_damping: f32,
    pub angular_damping: f32,
}

#[derive(Component, Clone, Copy)]
pub struct Part {
    pub kind: PartKind,
    pub sticky: bool,
}

#[derive(Component)]
pub struct PlayerPart;

#[derive(Component)]
pub struct Fixture;

#[derive(Component)]
pub struct Ghost;

pub struct SpawnPart<'a> {
    pub spec: &'a PartSpec,
    pub position: Vec3,
    pub rotation_x: f32,
    pub rotation_y: f32,
    pub rotation_z: f32,
    pub sticky: bool,
    pub player: bool,
    pub ghost: bool,
}

pub fn spawn_part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    _materials: &mut Assets<StandardMaterial>,
    palette: &Palette,
    spawn: &SpawnPart,
) -> Entity {
    let rotation = Quat::from_euler(
        EulerRot::XYZ,
        spawn.rotation_x,
        spawn.rotation_y,
        spawn.rotation_z,
    );
    let transform = Transform {
        translation: spawn.position,
        rotation,
        scale: Vec3::ONE,
    };
    let mesh = mesh_for(meshes, spawn.spec);
    let material = if spawn.ghost {
        palette.ghost.clone()
    } else {
        palette.for_kind(spawn.spec.kind)
    };

    let mut entity = commands.spawn((
        LevelObject,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        transform,
    ));

    if spawn.ghost {
        entity.insert(Ghost);
        return entity.id();
    }

    entity.insert(Part {
        kind: spawn.spec.kind,
        sticky: spawn.sticky,
    });
    if spawn.player {
        entity.insert(PlayerPart);
    } else {
        entity.insert(Fixture);
    }

    let body = if spawn.sticky {
        RigidBody::Static
    } else {
        RigidBody::Dynamic
    };
    entity.insert((
        body,
        collider_for(spawn.spec),
        Position(spawn.position),
        Rotation(rotation),
        Friction::new(spawn.spec.friction),
        restitution_for(spawn.spec),
        Mass(spawn.spec.mass),
        LinearVelocity::ZERO,
        AngularVelocity::ZERO,
        LinearDamping(spawn.spec.linear_damping),
        AngularDamping(spawn.spec.angular_damping),
    ));
    entity.id()
}

fn mesh_for(meshes: &mut Assets<Mesh>, spec: &PartSpec) -> Handle<Mesh> {
    if spec.kind == PartKind::Ball {
        meshes.add(Sphere::new(spec.size.x * 0.5))
    } else {
        meshes.add(Cuboid::new(spec.size.x, spec.size.y, spec.size.z))
    }
}

fn restitution_for(spec: &PartSpec) -> Restitution {
    let restitution = Restitution::new(spec.restitution);
    if spec.kind == PartKind::Spring {
        // A ball's own restitution is nearly zero. Max keeps the spring's
        // bounce instead of averaging it away.
        restitution.with_combine_rule(CoefficientCombine::Max)
    } else {
        restitution
    }
}

fn collider_for(spec: &PartSpec) -> Collider {
    if spec.kind == PartKind::Ball {
        Collider::sphere(spec.size.x * 0.5)
    } else {
        Collider::cuboid(spec.size.x, spec.size.y, spec.size.z)
    }
}
