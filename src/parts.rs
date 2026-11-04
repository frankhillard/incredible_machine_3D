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
    Rope,
    Weight,
}

impl PartKind {
    pub const ALL: [PartKind; 7] = [
        PartKind::Board,
        PartKind::Block,
        PartKind::Plank,
        PartKind::Ball,
        PartKind::Spring,
        PartKind::Rope,
        PartKind::Weight,
    ];

    pub fn slot(self) -> usize {
        match self {
            PartKind::Board => 0,
            PartKind::Block => 1,
            PartKind::Plank => 2,
            PartKind::Ball => 3,
            PartKind::Spring => 4,
            PartKind::Rope => 5,
            PartKind::Weight => 6,
        }
    }

    /// A ball and a weight are both spheres. `size.x` is the diameter.
    pub fn round(self) -> bool {
        matches!(self, PartKind::Ball | PartKind::Weight)
    }

    pub fn label(self) -> &'static str {
        match self {
            PartKind::Board => "Board",
            PartKind::Block => "Block",
            PartKind::Plank => "Plank",
            PartKind::Ball => "Ball",
            PartKind::Spring => "Spring",
            PartKind::Rope => "Rope",
            PartKind::Weight => "Weight",
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
            PartKind::Weight => PartSpec {
                kind: self,
                size: Vec3::splat(0.32),
                friction: 0.16,
                restitution: 0.05,
                mass: 5.0,
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
            PartKind::Rope => PartSpec {
                kind: self,
                size: Vec3::new(1.4, 0.06, 0.06),
                friction: 0.45,
                restitution: 0.02,
                mass: 0.45,
                sticky_default: false,
                linear_damping: 0.2,
                angular_damping: 0.45,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct PartSpec {
    pub kind: PartKind,
    /// Full box size. For a ball or a weight, every component is the diameter.
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

/// One moving piece of a rope. The root link is the [`Part`]; the others hang off it.
#[derive(Component)]
pub struct ChainLink {
    pub root: Entity,
    /// Position along the rope. The root is 0 and is not a [`ChainLink`].
    pub index: u8,
    /// The link at the far end. The root is the near end.
    pub end: bool,
}

/// A joint that belongs to a rope, so deleting the rope removes it too.
#[derive(Component, Clone, Copy)]
pub struct RopeJoint {
    pub root: Entity,
    /// Set when this joint pins a rope end onto another part.
    pub pinned_end: Option<Entity>,
    pub host: Option<Entity>,
}

/// Joint rest length relative to the drawn gap. 1 keeps a pinned rope from
/// storing a stretch that launches the parts it is tied to.
pub const ROPE_SLACK: f32 = 1.0;

/// Meters of give per newton. A hard rope tied to a heavy part kicks it around.
const ROPE_COMPLIANCE: f32 = 0.00015;

/// How many capsules make one rope. Interior sockets are spherical; the two
/// end connections are distance joints.
const ROPE_LINKS: usize = 8;

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
    if spawn.spec.kind == PartKind::Rope {
        return spawn_rope(commands, meshes, palette, spawn);
    }
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
    if spec.kind.round() {
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

fn spawn_rope(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    palette: &Palette,
    spawn: &SpawnPart,
) -> Entity {
    let rotation = Quat::from_euler(
        EulerRot::XYZ,
        spawn.rotation_x,
        spawn.rotation_y,
        spawn.rotation_z,
    );
    let length = spawn.spec.size.x;
    let pitch = length / ROPE_LINKS as f32;
    let axis = rotation * Vec3::X;
    let origin = spawn.position - axis * (length * 0.5);
    let near = origin + axis * (pitch * 0.5);
    let far = origin + axis * (pitch * ((ROPE_LINKS - 1) as f32 + 0.5));
    // Capsules are built along Y. A negative quarter-turn about Z lays that
    // axis along the rope, so local +Y points toward the next link.
    let link_rotation = rotation * Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
    assemble_rope(
        commands,
        meshes,
        palette,
        spawn.spec,
        near,
        far,
        link_rotation,
        spawn.player,
        spawn.ghost,
    )
    .0
}

/// Spawns a rope whose end links sit exactly on `near` and `far`.
///
/// The returned entities are the near link (the rope [`Part`]) and the far link.
pub fn spawn_rope_between(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    palette: &Palette,
    spec: &PartSpec,
    near: Vec3,
    far: Vec3,
    player: bool,
) -> (Entity, Entity) {
    let dir = (far - near).try_normalize().unwrap_or(Vec3::X);
    assemble_rope(
        commands,
        meshes,
        palette,
        spec,
        near,
        far,
        Quat::from_rotation_arc(Vec3::Y, dir),
        player,
        false,
    )
}

fn assemble_rope(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    palette: &Palette,
    spec: &PartSpec,
    near: Vec3,
    far: Vec3,
    link_rotation: Quat,
    player: bool,
    ghost: bool,
) -> (Entity, Entity) {
    let radius = spec.size.y * 0.5;
    let spacing = near.distance(far) / (ROPE_LINKS - 1) as f32;
    let cylinder = (spacing - 2.0 * radius).max(radius);
    let mesh = meshes.add(Capsule3d::new(radius, cylinder));
    let material = if ghost {
        palette.ghost.clone()
    } else {
        palette.for_kind(PartKind::Rope)
    };
    let mut links = Vec::with_capacity(ROPE_LINKS);
    for index in 0..ROPE_LINKS {
        let center = near.lerp(far, index as f32 / (ROPE_LINKS - 1) as f32);
        let mut entity = commands.spawn((
            LevelObject,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: center,
                rotation: link_rotation,
                scale: Vec3::ONE,
            },
        ));
        if ghost {
            entity.insert(Ghost);
        }
        links.push(entity.id());
    }
    let root = links[0];
    let far_link = links[ROPE_LINKS - 1];
    if ghost {
        return (root, far_link);
    }

    let link_mass = spec.mass / ROPE_LINKS as f32;
    let collider = Collider::capsule(radius, cylinder);
    // Rope links must not collide with each other, or the sockets fight the contacts.
    let layers = CollisionLayers::from_bits(0b10, !0b10);
    for (index, link) in links.iter().copied().enumerate() {
        // Ends stay loose until a gizmo or a level pin parks them on a part.
        let center = near.lerp(far, index as f32 / (ROPE_LINKS - 1) as f32);
        let mut entity = commands.entity(link);
        entity.insert((
            RigidBody::Dynamic,
            collider.clone(),
            layers,
            Position(center),
            Rotation(link_rotation),
            Friction::new(spec.friction),
            Restitution::new(spec.restitution),
            Mass(link_mass),
            ActiveCollisionHooks::FILTER_PAIRS,
            LinearVelocity::ZERO,
            AngularVelocity::ZERO,
            LinearDamping(spec.linear_damping),
            AngularDamping(spec.angular_damping),
        ));
        if index == 0 {
            entity.insert(Part {
                kind: PartKind::Rope,
                sticky: false,
            });
            if player {
                entity.insert(PlayerPart);
            } else {
                entity.insert(Fixture);
            }
        } else {
            entity.insert(ChainLink {
                root,
                index: index as u8,
                end: index + 1 == ROPE_LINKS,
            });
        }
    }

    // Sockets meet at the drawn gap. End joints may fold shorter, but they
    // cannot stretch or push the parts they are pinned to.
    let slack = spacing * ROPE_SLACK;
    let tip = Vec3::Y * (slack * 0.5);
    for index in 0..ROPE_LINKS - 1 {
        let end_joint = index == 0 || index + 2 == ROPE_LINKS;
        if end_joint {
            commands.spawn((
                LevelObject,
                RopeJoint {
                    root,
                    pinned_end: None,
                    host: None,
                },
                DistanceJoint::new(links[index], links[index + 1])
                    .with_local_anchor1(Vec3::ZERO)
                    .with_local_anchor2(Vec3::ZERO)
                    .with_limits(0.0, spacing)
                    .with_compliance(ROPE_COMPLIANCE),
            ));
        } else {
            commands.spawn((
                LevelObject,
                RopeJoint {
                    root,
                    pinned_end: None,
                    host: None,
                },
                SphericalJoint::new(links[index], links[index + 1])
                    .with_local_anchor1(tip)
                    .with_local_anchor2(-tip)
                    .with_twist_axis(Vec3::Y)
                    .with_point_compliance(ROPE_COMPLIANCE),
            ));
        }
    }
    (root, far_link)
}

/// Pins a rope end to `host` so the end follows that body, including a loose one.
pub fn pin_rope_end(
    commands: &mut Commands,
    root: Entity,
    end: Entity,
    host: Entity,
    local_on_host: Vec3,
) {
    commands.spawn((
        LevelObject,
        RopeJoint {
            root,
            pinned_end: Some(end),
            host: Some(host),
        },
        SphericalJoint::new(end, host)
            .with_local_anchor1(Vec3::ZERO)
            .with_local_anchor2(local_on_host)
            .with_point_compliance(ROPE_COMPLIANCE),
    ));
    // The pin holds the end. A contact with the host would fight that joint.
    commands.entity(end).insert(Sensor);
}

/// Keeps a rope from bumping the parts its ends are tied to. The links run
/// through those parts, and that contact is what throws them around.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RopeContactHooks<'w, 's> {
    links: Query<'w, 's, &'static ChainLink>,
    ropes: Query<'w, 's, &'static Part>,
    pins: Query<'w, 's, &'static RopeJoint>,
}

impl RopeContactHooks<'_, '_> {
    fn rope_root(&self, entity: Entity) -> Option<Entity> {
        if let Ok(link) = self.links.get(entity) {
            return Some(link.root);
        }
        self.ropes
            .get(entity)
            .ok()
            .filter(|part| part.kind == PartKind::Rope)
            .map(|_| entity)
    }

    fn tied_to(&self, rope: Entity, other: Entity) -> bool {
        self.pins
            .iter()
            .any(|joint| joint.root == rope && joint.host == Some(other))
    }
}

impl CollisionHooks for RopeContactHooks<'_, '_> {
    fn filter_pairs(&self, collider1: Entity, collider2: Entity, _commands: &mut Commands) -> bool {
        if let Some(root) = self.rope_root(collider1) {
            if self.tied_to(root, collider2) {
                return false;
            }
        }
        if let Some(root) = self.rope_root(collider2) {
            if self.tied_to(root, collider1) {
                return false;
            }
        }
        true
    }
}

fn collider_for(spec: &PartSpec) -> Collider {
    if spec.kind.round() {
        Collider::sphere(spec.size.x * 0.5)
    } else {
        Collider::cuboid(spec.size.x, spec.size.y, spec.size.z)
    }
}
