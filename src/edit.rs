use avian3d::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::goal::GameState;
use crate::level::{self, Placement, LANE_Z};
use crate::parts::{
    pin_rope_end, ChainLink, Fixture, Part, PartKind, PlayerPart, RopeJoint, ROPE_SLACK,
};
use crate::scene::{OrbitCamera, Palette};
use crate::sim::{write_pose, Selection, Shelf, UiPointer};

const AXIS_GAP: f32 = 0.05;
const AXIS_LENGTH: f32 = 0.72;
const AXIS_PICK_GAP: f32 = 0.16;
const AXIS_PICK_PIXELS: f32 = 18.0;
const SLIDE_PER_NOTCH: f32 = 0.08;
const SPIN_PER_NOTCH: f32 = 0.1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn local(self) -> Vec3 {
        match self {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
}

/// Which end of a selected rope the arrow gizmo belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RopeEndSide {
    Near,
    Far,
}

/// The axis arrow the player has picked on the selected shelf part.
#[derive(Resource, Default)]
pub struct AxisGizmo {
    pub axis: Option<Axis>,
    /// Set while the picked arrows sit on a rope end.
    pub rope_end: Option<RopeEndSide>,
}

impl AxisGizmo {
    pub fn clear(&mut self) {
        self.axis = None;
        self.rope_end = None;
    }
}

pub fn configure_gizmos(mut store: ResMut<bevy::gizmos::config::GizmoConfigStore>) {
    let (config, _) = store.config_mut::<bevy::gizmos::config::DefaultGizmoConfigGroup>();
    config.line.width = 6.0;
    config.depth_bias = -0.8;
}

pub fn clear_axis_gizmo(mut gizmo: ResMut<AxisGizmo>) {
    gizmo.clear();
}

/// Slides `pose` along one of its own axes.
pub fn slide(pose: &mut Transform, axis: Axis, distance: f32) {
    pose.translation += pose.rotation * axis.local() * distance;
}

/// Turns `pose` around one of its own axes. The pivot stays put.
pub fn spin(pose: &mut Transform, axis: Axis, angle: f32) {
    let direction = pose.rotation * axis.local();
    pose.rotation = (Quat::from_axis_angle(direction, angle) * pose.rotation).normalize();
}

/// How far a loose part's center can be from a host surface and still be resting on it.
pub fn support_reach(kind: PartKind) -> f32 {
    let size = kind.spec().size;
    if kind.round() {
        size.x * 0.5
    } else {
        size.min_element() * 0.5
    }
}

pub fn resting_on(passenger: Vec3, host: Transform, host_half: Vec3, reach: f32) -> bool {
    let local = host.rotation.inverse() * (passenger - host.translation);
    signed_distance_to_box(local, host_half) <= reach + 0.08
}

/// Keeps a loose part glued to a host while that host is tilted or slid in the editor.
pub fn carried_transform(
    host_old: Transform,
    host_new: Transform,
    passenger: Transform,
) -> Transform {
    let local_pos = host_old.rotation.inverse() * (passenger.translation - host_old.translation);
    let local_rot = host_old.rotation.inverse() * passenger.rotation;
    let mut updated = passenger;
    updated.translation = host_new.translation + host_new.rotation * local_pos;
    updated.rotation = (host_new.rotation * local_rot).normalize();
    updated
}

fn riding_poses(
    parts: &mut Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    host: Entity,
    host_old: Transform,
    host_new: Transform,
) -> Vec<(Entity, Transform)> {
    let host_kind = match parts.get(host) {
        Ok((_, _, part, _, _)) => part.kind,
        Err(_) => return Vec::new(),
    };
    if host_kind.round() {
        return Vec::new();
    }
    let half = host_kind.spec().size * 0.5;
    let mut rides = Vec::new();
    for (entity, transform, part, _, _) in parts.iter() {
        if entity == host || part.sticky {
            continue;
        }
        if !resting_on(
            transform.translation,
            host_old,
            half,
            support_reach(part.kind),
        ) {
            continue;
        }
        rides.push((entity, carried_transform(host_old, host_new, *transform)));
    }
    rides
}

pub fn apply_pose(
    parts: &mut Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    links: &mut Query<
        (
            Entity,
            &ChainLink,
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Without<PlayerPart>,
    >,
    entity: Entity,
    pose: Transform,
) {
    let old = match parts.get(entity) {
        Ok((_, transform, _, _, _)) => *transform,
        Err(_) => return,
    };
    let rides = riding_poses(parts, entity, old, pose);
    let spin = pose.rotation * old.rotation.inverse();
    let mut follows = Vec::new();
    for (link_entity, link, transform, _, _) in links.iter() {
        if link.root != entity {
            continue;
        }
        let offset = transform.translation - old.translation;
        follows.push((
            link_entity,
            Transform {
                translation: pose.translation + spin * offset,
                rotation: spin * transform.rotation,
                scale: Vec3::ONE,
            },
        ));
    }
    if let Ok((_, mut transform, _, position, rotation)) = parts.get_mut(entity) {
        write_pose(&mut transform, position, rotation, pose);
    }
    for (ride, ride_pose) in rides {
        if let Ok((_, mut transform, _, position, rotation)) = parts.get_mut(ride) {
            write_pose(&mut transform, position, rotation, ride_pose);
        }
    }
    for (link_entity, follow) in follows {
        if let Ok((_, _, mut transform, position, rotation)) = links.get_mut(link_entity) {
            write_pose(&mut transform, position, rotation, follow);
        }
    }
}

pub fn static_cuboids<'a>(
    supports: impl IntoIterator<Item = (&'a Transform, &'a Collider, &'a RigidBody)>,
) -> Vec<(Transform, Vec3)> {
    let mut found = Vec::new();
    for (transform, collider, body) in supports {
        if *body != RigidBody::Static {
            continue;
        }
        let Some(cuboid) = collider.shape_scaled().as_cuboid() else {
            continue;
        };
        let half = cuboid.half_extents;
        found.push((*transform, Vec3::new(half.x, half.y, half.z)));
    }
    found
}

/// Puts a part on the first upward face the cursor ray hits.
pub fn point_on_supports(
    origin: Vec3,
    direction: Dir3,
    kind: PartKind,
    rotation_z: f32,
    supports: &[(Transform, Vec3)],
) -> Option<Vec3> {
    let dir = direction.as_vec3();
    let rotation = Quat::from_rotation_z(rotation_z);
    let spec = kind.spec();
    let mut hits = Vec::new();
    for (pose, half) in supports {
        if let Some((distance, point, normal)) = ray_cuboid(origin, dir, *pose, *half) {
            hits.push((distance, point, normal, *pose, *half));
        }
    }
    hits.sort_by(|left, right| left.0.total_cmp(&right.0));
    for (_, point, normal, pose, half) in hits {
        let (surface, face_normal) = if normal.y >= 0.25 {
            (point, normal)
        } else if let Some(top) = top_above(point, pose, half) {
            top
        } else {
            continue;
        };
        let extent = if kind.round() {
            spec.size.x * 0.5
        } else {
            extent_along(spec.size, rotation, face_normal)
        };
        return Some(clamp_table(surface + face_normal * extent));
    }
    ray_lane(origin, direction).map(clamp_table)
}

/// The upward face of `pose`, directly above `point`, when that face is close.
fn top_above(point: Vec3, pose: Transform, half: Vec3) -> Option<(Vec3, Vec3)> {
    let sign = if (pose.rotation * Vec3::Y).y >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let mut local = pose.rotation.inverse() * (point - pose.translation);
    local.x = local.x.clamp(-half.x, half.x);
    local.z = local.z.clamp(-half.z, half.z);
    local.y = sign * half.y;
    let surface = pose.translation + pose.rotation * local;
    if (surface.y - point.y).abs() > 0.45 {
        return None;
    }
    let normal = (pose.rotation * (Vec3::Y * sign)).normalize();
    (normal.y >= 0.25).then_some((surface, normal))
}

/// Drops a ball onto the slope under it when the surface is close.
/// `max_drop` is how far above that surface the ball may be and still be seated.
/// A buried ball (negative drop) is lifted out as long as it is within 0.45 m.
pub fn rest_on_slope(
    translation: Vec3,
    radius: f32,
    supports: &[(Transform, Vec3)],
    max_drop: f32,
) -> Vec3 {
    let origin = translation + Vec3::Y * 4.0;
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for (pose, half) in supports {
        let Some((distance, point, normal)) = ray_cuboid(origin, Vec3::NEG_Y, *pose, *half) else {
            continue;
        };
        if normal.y < 0.25 {
            continue;
        }
        if best
            .as_ref()
            .is_some_and(|(best_distance, _, _)| *best_distance <= distance)
        {
            continue;
        }
        best = Some((distance, point, normal));
    }
    let Some((_, point, normal)) = best else {
        return translation;
    };
    // Move only vertically, onto the plane under the ball. A ball already
    // resting on that plane has a signed distance equal to its radius, so it stays.
    let signed = normal.dot(translation - point);
    let target_y = translation.y + (radius - signed) / normal.y;
    let drop = translation.y - target_y;
    if drop < max_drop && drop > -0.45 {
        let mut seated = translation;
        seated.y = target_y;
        seated
    } else {
        translation
    }
}

pub fn unbury_balls(
    state: Res<State<GameState>>,
    supports: Query<(&Transform, &Collider, &RigidBody), (Without<PlayerPart>, Without<ChainLink>)>,
    mut balls: Query<
        (
            Entity,
            &mut Transform,
            &Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        With<PlayerPart>,
    >,
) {
    if *state.get() != GameState::Editing {
        return;
    }
    let mut supports = static_cuboids(&supports);
    for (_, transform, part, _, _) in &balls {
        if part.sticky && !part.kind.round() && part.kind != PartKind::Rope {
            supports.push((*transform, part.kind.spec().size * 0.5));
        }
    }
    let mut fixes = Vec::new();
    for (entity, transform, part, _, _) in &balls {
        if !part.kind.round() || part.sticky {
            continue;
        }
        let radius = part.kind.spec().size.x * 0.5;
        let seated = rest_on_slope(transform.translation, radius, &supports, 0.0);
        if seated.distance(transform.translation) > 1.0e-3 {
            fixes.push((entity, seated));
        }
    }
    for (entity, translation) in fixes {
        if let Ok((_, mut transform, _, position, rotation)) = balls.get_mut(entity) {
            let mut pose = *transform;
            pose.translation = translation;
            write_pose(&mut transform, position, rotation, pose);
        }
    }
}

fn extent_along(size: Vec3, rotation: Quat, normal: Vec3) -> f32 {
    let half = size * 0.5;
    half.x * (rotation * Vec3::X).dot(normal).abs()
        + half.y * (rotation * Vec3::Y).dot(normal).abs()
        + half.z * (rotation * Vec3::Z).dot(normal).abs()
}

fn clamp_table(point: Vec3) -> Vec3 {
    Vec3::new(
        point.x.clamp(-6.2, 6.8),
        point.y.clamp(0.15, 6.4),
        point.z.clamp(-0.35, 1.0),
    )
}

/// Ray against an oriented box. Returns distance, point, and outward normal.
fn ray_cuboid(origin: Vec3, dir: Vec3, pose: Transform, half: Vec3) -> Option<(f32, Vec3, Vec3)> {
    let inv = pose.rotation.inverse();
    let local_origin = inv * (origin - pose.translation);
    let local_dir = inv * dir;
    let mut t_min = 0.0_f32;
    let mut t_max = f32::MAX;
    let mut enter_axis = 0;
    let mut enter_sign = 0.0;
    for axis in 0..3 {
        let direction = local_dir[axis];
        let start = local_origin[axis];
        let extent = half[axis];
        if direction.abs() < 1.0e-8 {
            if start < -extent || start > extent {
                return None;
            }
            continue;
        }
        let inv_dir = 1.0 / direction;
        let mut near = (-extent - start) * inv_dir;
        let mut far = (extent - start) * inv_dir;
        let mut sign = -1.0;
        if near > far {
            std::mem::swap(&mut near, &mut far);
            sign = 1.0;
        }
        if near > t_min {
            t_min = near;
            enter_axis = axis;
            enter_sign = sign;
        }
        if far < t_max {
            t_max = far;
        }
        if t_min > t_max {
            return None;
        }
    }
    if t_max < 0.0 {
        return None;
    }
    let distance = if t_min >= 0.0 { t_min } else { t_max };
    let mut local_normal = Vec3::ZERO;
    local_normal[enter_axis] = enter_sign;
    let normal = (pose.rotation * local_normal).normalize();
    let point = origin + dir * distance;
    Some((distance, point, normal))
}

fn signed_distance_to_box(local: Vec3, half: Vec3) -> f32 {
    let delta = local.abs() - half;
    let outside = delta.max(Vec3::ZERO).length();
    if outside > 1.0e-5 {
        outside
    } else {
        delta.max_element()
    }
}

pub fn orbit_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    ui: Res<UiPointer>,
    gizmo: Res<AxisGizmo>,
    state: Res<State<GameState>>,
    mut cameras: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    let Some((mut orbit, mut transform)) = cameras.iter_mut().next() else {
        return;
    };
    let mut delta = Vec2::ZERO;
    for event in motion.read() {
        delta += event.delta;
    }
    if !ui.blocks {
        if mouse.pressed(MouseButton::Right) {
            orbit.yaw -= delta.x * 0.005;
            orbit.pitch = (orbit.pitch + delta.y * 0.005).clamp(0.12, 1.25);
        }
        if mouse.pressed(MouseButton::Middle) {
            let right = transform.right();
            let up = transform.up();
            let scale = 0.0022 * orbit.radius;
            orbit.focus += (-right * delta.x + up * delta.y) * scale;
        }
        let axis_handles_scroll = gizmo.axis.is_some() && *state.get() == GameState::Editing;
        if !axis_handles_scroll {
            for event in wheel.read() {
                let lines = match event.unit {
                    MouseScrollUnit::Line => event.y,
                    MouseScrollUnit::Pixel => event.y / 60.0,
                };
                orbit.radius = (orbit.radius * (1.0 - lines * 0.12)).clamp(3.2, 22.0);
            }
        } else {
            wheel.clear();
        }
    } else {
        wheel.clear();
    }
    *transform = orbit.transform();
}

pub fn edit_keys(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    ui: Res<UiPointer>,
    mut shelf: ResMut<Shelf>,
    selection: Res<Selection>,
    gizmo: Res<AxisGizmo>,
    mut commands: Commands,
    mut parts: Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    mut links: Query<
        (
            Entity,
            &ChainLink,
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Without<PlayerPart>,
    >,
    joints: Query<(Entity, &RopeJoint)>,
) {
    if *state.get() != GameState::Editing || ui.keys {
        return;
    }
    let dt = time.delta_secs();
    let mut step = 0.0;
    if keys.pressed(KeyCode::KeyQ) {
        step += if keys.just_pressed(KeyCode::KeyQ) {
            0.07
        } else {
            dt * 1.15
        };
    }
    if keys.pressed(KeyCode::KeyE) {
        step -= if keys.just_pressed(KeyCode::KeyE) {
            0.07
        } else {
            dt * 1.15
        };
    }
    if step != 0.0 {
        if let Some(entity) = selection.0 {
            let pose = parts.get(entity).ok().map(|(_, transform, _, _, _)| {
                let mut pose = *transform;
                spin(&mut pose, gizmo.axis.unwrap_or(Axis::Z), step);
                pose
            });
            if let Some(pose) = pose {
                if parts
                    .get(entity)
                    .is_ok_and(|(_, _, part, _, _)| part.kind == PartKind::Rope)
                {
                    let pins: Vec<_> = joints.iter().map(|(joint, rope)| (joint, *rope)).collect();
                    release_rope_pins(&mut commands, entity, &pins);
                }
                apply_pose(&mut parts, &mut links, entity, pose);
            }
        } else if shelf.armed.is_some() {
            shelf.tilt += step;
        }
    }
}

pub fn gizmo_scroll(
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    ui: Res<UiPointer>,
    selection: Res<Selection>,
    mut gizmo: ResMut<AxisGizmo>,
    mut parts: Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    supports: Query<(&Transform, &Collider, &RigidBody), (Without<PlayerPart>, Without<ChainLink>)>,
    mut links: Query<
        (
            Entity,
            &ChainLink,
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Without<PlayerPart>,
    >,
    mut commands: Commands,
    colliders: Query<&Collider>,
    fixtures: Query<
        (Entity, &Transform, &Part),
        (With<Fixture>, Without<PlayerPart>, Without<ChainLink>),
    >,
    mut joints: Query<(
        Entity,
        &RopeJoint,
        Option<&mut DistanceJoint>,
        Option<&mut SphericalJoint>,
    )>,
) {
    if selection.0.is_none() {
        gizmo.clear();
    }
    if *state.get() != GameState::Editing || ui.blocks {
        wheel.clear();
        return;
    }
    let Some(entity) = selection.0 else {
        return;
    };
    let Some(axis) = gizmo.axis else {
        return;
    };
    let lines = wheel_lines(&mut wheel);
    if lines == 0.0 {
        return;
    }
    if parts
        .get(entity)
        .is_ok_and(|(_, _, part, _, _)| part.kind == PartKind::Rope)
    {
        if let Some(side) = gizmo.rope_end {
            let delta = axis.local() * lines * SLIDE_PER_NOTCH;
            shift_rope_end(
                &mut commands,
                entity,
                side,
                delta,
                &mut parts,
                &mut links,
                &colliders,
                &fixtures,
                &mut joints,
            );
        }
        return;
    }
    let mut surfaces = static_cuboids(&supports);
    for (_, transform, part, _, _) in &parts {
        if part.sticky && !part.kind.round() && part.kind != PartKind::Rope {
            surfaces.push((*transform, part.kind.spec().size * 0.5));
        }
    }
    let Some(pose) = (|| {
        let Ok((_, transform, part, _, _)) = parts.get(entity) else {
            gizmo.clear();
            return None;
        };
        let part = *part;
        let mut pose = *transform;
        let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
        if ctrl {
            spin(&mut pose, axis, lines * SPIN_PER_NOTCH);
        } else {
            slide(&mut pose, axis, lines * SLIDE_PER_NOTCH);
            if part.kind.round() && !part.sticky {
                let radius = part.kind.spec().size.x * 0.5;
                pose.translation = rest_on_slope(pose.translation, radius, &surfaces, 0.35);
            }
        }
        Some(pose)
    })() else {
        return;
    };
    apply_pose(&mut parts, &mut links, entity, pose);
}

pub fn sync_ghost(
    mut commands: Commands,
    state: Res<State<GameState>>,
    ui: Res<UiPointer>,
    shelf: Res<Shelf>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    supports: Query<(&Transform, &Collider, &RigidBody), (Without<PlayerPart>, Without<ChainLink>)>,
    nailed: Query<(&Transform, &Part), With<PlayerPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<Palette>,
    ghosts: Query<Entity, With<crate::parts::Ghost>>,
) {
    for entity in &ghosts {
        commands.entity(entity).despawn();
    }
    if *state.get() != GameState::Editing || ui.blocks {
        return;
    }
    let Some(kind) = shelf.armed else {
        return;
    };
    let Some((origin, direction)) = cursor_ray(&windows, &cameras) else {
        return;
    };
    let mut surfaces = static_cuboids(&supports);
    for (transform, part) in &nailed {
        if part.sticky && !part.kind.round() && part.kind != PartKind::Rope {
            surfaces.push((*transform, part.kind.spec().size * 0.5));
        }
    }
    let Some(point) = point_on_supports(origin, direction, kind, shelf.tilt, &surfaces) else {
        return;
    };
    let spec = kind.spec();
    crate::parts::spawn_part(
        &mut commands,
        &mut meshes,
        &mut materials,
        &palette,
        &crate::parts::SpawnPart {
            spec: &spec,
            position: point,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: shelf.tilt,
            sticky: true,
            player: false,
            ghost: true,
        },
    );
}

pub fn pointer(
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<State<GameState>>,
    ui: Res<UiPointer>,
    mut shelf: ResMut<Shelf>,
    mut selection: ResMut<Selection>,
    mut gizmo: ResMut<AxisGizmo>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    supports: Query<(&Transform, &Collider, &RigidBody), (Without<PlayerPart>, Without<ChainLink>)>,
    spatial: SpatialQuery,
    players: Query<(&Transform, &Part), With<PlayerPart>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<Palette>,
    links: Query<(Entity, &ChainLink, &Transform)>,
) {
    if *state.get() != GameState::Editing || ui.blocks || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some((origin, direction)) = cursor_ray(&windows, &cameras) else {
        return;
    };
    if let Some(entity) = selection.0 {
        if let Ok((transform, part)) = players.get(entity) {
            let picked = if part.kind == PartKind::Rope {
                rope_gizmo_under_cursor(&windows, &cameras, &links, entity, transform.translation)
                    .map(|(side, hit)| (Some(side), hit, rope_collider_entities(&links, entity)))
            } else {
                axis_under_cursor(&windows, &cameras, transform, part.kind.spec().size)
                    .map(|hit| (None, hit, vec![entity]))
            };
            if let Some((side, hit, ends)) = picked {
                let body = spatial.cast_ray_predicate(
                    origin,
                    direction,
                    80.0,
                    true,
                    &SpatialQueryFilter::default(),
                    &|candidate| ends.contains(&candidate),
                );
                let axis_is_in_front = body
                    .map(|body| hit.ray_distance <= body.distance + 0.02)
                    .unwrap_or(true);
                if axis_is_in_front {
                    gizmo.axis = Some(hit.axis);
                    gizmo.rope_end = side;
                    return;
                }
            }
        }
    }
    let hit = spatial.cast_ray_predicate(
        origin,
        direction,
        80.0,
        true,
        &SpatialQueryFilter::default(),
        &|entity| players.contains(entity) || links.contains(entity),
    );
    if let Some(hit) = hit {
        // A click on any capsule selects the rope, whose root is the shelf part.
        let selected = links
            .get(hit.entity)
            .map(|(_, link, _)| link.root)
            .unwrap_or(hit.entity);
        selection.0 = Some(selected);
        gizmo.clear();
        shelf.armed = None;
        return;
    }
    selection.0 = None;
    gizmo.clear();
    let Some(kind) = shelf.armed else {
        return;
    };
    if shelf.remaining[kind.slot()] == 0 {
        shelf.armed = None;
        return;
    }
    let mut surfaces = static_cuboids(&supports);
    for (transform, part) in &players {
        if part.sticky && !part.kind.round() && part.kind != PartKind::Rope {
            surfaces.push((*transform, part.kind.spec().size * 0.5));
        }
    }
    let Some(point) = point_on_supports(origin, direction, kind, shelf.tilt, &surfaces) else {
        return;
    };
    level::spawn_player_part(
        &mut commands,
        &mut meshes,
        &mut materials,
        &palette,
        Placement {
            kind,
            position: [point.x, point.y, point.z],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: shelf.tilt,
            sticky: kind.spec().sticky_default,
            ends: [[0.0; 3]; 2],
        },
    );
    shelf.remaining[kind.slot()] -= 1;
    if shelf.remaining[kind.slot()] == 0 {
        shelf.armed = None;
    }
}

pub fn delete_selected(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    ui: Res<UiPointer>,
    mut shelf: ResMut<Shelf>,
    mut selection: ResMut<Selection>,
    mut gizmo: ResMut<AxisGizmo>,
    parts: Query<&Part, With<PlayerPart>>,
    links: Query<(Entity, &ChainLink)>,
    joints: Query<(Entity, &RopeJoint)>,
) {
    if *state.get() != GameState::Editing || ui.keys {
        return;
    }
    if !(keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace)) {
        return;
    }
    let Some(entity) = selection.0.take() else {
        return;
    };
    let Ok(part) = parts.get(entity) else {
        gizmo.clear();
        return;
    };
    shelf.remaining[part.kind.slot()] += 1;
    let link_ids: Vec<_> = links
        .iter()
        .filter(|(_, link)| link.root == entity)
        .map(|(link_entity, _)| link_entity)
        .collect();
    let joint_ids: Vec<_> = joints
        .iter()
        .filter(|(_, joint)| joint.root == entity)
        .map(|(joint_entity, _)| joint_entity)
        .collect();
    despawn_chain(&mut commands, entity, link_ids, joint_ids);
    gizmo.clear();
}

pub fn despawn_chain(
    commands: &mut Commands,
    root: Entity,
    links: impl IntoIterator<Item = Entity>,
    joints: impl IntoIterator<Item = Entity>,
) {
    commands.entity(root).despawn();
    for entity in links {
        commands.entity(entity).despawn();
    }
    for entity in joints {
        commands.entity(entity).despawn();
    }
}

pub fn highlight_selection(
    selection: Res<Selection>,
    palette: Res<Palette>,
    joints: Query<&RopeJoint>,
    mut parts: Query<
        (Entity, &Part, &mut MeshMaterial3d<StandardMaterial>),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    mut fixtures: Query<
        (Entity, &Part, &mut MeshMaterial3d<StandardMaterial>),
        (With<Fixture>, Without<PlayerPart>, Without<ChainLink>),
    >,
    mut links: Query<(&ChainLink, &mut MeshMaterial3d<StandardMaterial>), Without<PlayerPart>>,
) {
    let pinned = |entity: Entity| {
        joints
            .iter()
            .any(|joint| selection.0 == Some(joint.root) && joint.host == Some(entity))
    };
    for (entity, part, mut material) in &mut parts {
        material.0 = if selection.0 == Some(entity) || pinned(entity) {
            palette.selected.clone()
        } else {
            palette.for_kind(part.kind)
        };
    }
    for (entity, part, mut material) in &mut fixtures {
        material.0 = if pinned(entity) {
            palette.selected.clone()
        } else {
            palette.for_kind(part.kind)
        };
    }
    for (link, mut material) in &mut links {
        material.0 = if selection.0 == Some(link.root) {
            palette.selected.clone()
        } else {
            palette.rope.clone()
        };
    }
}

fn cursor_ray(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
) -> Option<(Vec3, Dir3)> {
    let window = windows.iter().next()?;
    let cursor = window.cursor_position()?;
    let (camera, transform) = cameras.iter().next()?;
    let ray = camera.viewport_to_world(transform, cursor).ok()?;
    Some((ray.origin, ray.direction))
}

fn ray_lane(origin: Vec3, direction: Dir3) -> Option<Vec3> {
    let dir = direction.as_vec3();
    if dir.z.abs() < 1e-5 {
        return None;
    }
    let t = (LANE_Z - origin.z) / dir.z;
    if t < 0.0 {
        return None;
    }
    Some(origin + dir * t)
}

pub fn draw_axes(
    state: Res<State<GameState>>,
    selection: Res<Selection>,
    gizmo: Res<AxisGizmo>,
    parts: Query<(&Transform, &Part), With<PlayerPart>>,
    links: Query<(&ChainLink, &Transform)>,
    mut gizmos: Gizmos,
) {
    if *state.get() != GameState::Editing {
        return;
    }
    let Some(entity) = selection.0 else {
        return;
    };
    let Ok((transform, part)) = parts.get(entity) else {
        return;
    };
    if part.kind == PartKind::Rope {
        let end_size = Vec3::splat(0.16);
        paint_axes(
            &mut gizmos,
            &Transform::from_translation(transform.translation),
            end_size,
            gizmo_axis_for(&gizmo, RopeEndSide::Near),
        );
        if let Some(far) = far_end_translation(&links, entity) {
            paint_axes(
                &mut gizmos,
                &Transform::from_translation(far),
                end_size,
                gizmo_axis_for(&gizmo, RopeEndSide::Far),
            );
        }
        return;
    }
    paint_axes(&mut gizmos, transform, part.kind.spec().size, gizmo.axis);
}

fn gizmo_axis_for(gizmo: &AxisGizmo, side: RopeEndSide) -> Option<Axis> {
    (gizmo.rope_end == Some(side))
        .then_some(gizmo.axis)
        .flatten()
}

fn paint_axes(gizmos: &mut Gizmos, pose: &Transform, size: Vec3, active: Option<Axis>) {
    for axis in Axis::ALL {
        let (start, end) = axis_segment(pose, size, axis);
        let selected = active == Some(axis);
        let end = if selected {
            start + (end - start) * 1.18
        } else {
            end
        };
        gizmos.arrow(start, end, axis_color(axis, selected));
    }
}

fn far_end_translation(links: &Query<(&ChainLink, &Transform)>, root: Entity) -> Option<Vec3> {
    links.iter().find_map(|(link, transform)| {
        (link.root == root && link.end).then_some(transform.translation)
    })
}

fn rope_collider_entities(
    links: &Query<(Entity, &ChainLink, &Transform)>,
    root: Entity,
) -> Vec<Entity> {
    let mut bodies = vec![root];
    for (entity, link, _) in links.iter() {
        if link.root == root {
            bodies.push(entity);
        }
    }
    bodies
}

fn rope_gizmo_under_cursor(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    links: &Query<(Entity, &ChainLink, &Transform)>,
    root: Entity,
    near: Vec3,
) -> Option<(RopeEndSide, AxisHit)> {
    let size = Vec3::splat(0.16);
    let near_hit = axis_under_cursor(windows, cameras, &Transform::from_translation(near), size);
    let far_hit = far_end_translation_from(links, root).and_then(|far| {
        axis_under_cursor(windows, cameras, &Transform::from_translation(far), size)
    });
    match (near_hit, far_hit) {
        (Some(near_hit), Some(far_hit)) if far_hit.pixels < near_hit.pixels => {
            Some((RopeEndSide::Far, far_hit))
        }
        (Some(near_hit), _) => Some((RopeEndSide::Near, near_hit)),
        (None, Some(far_hit)) => Some((RopeEndSide::Far, far_hit)),
        (None, None) => None,
    }
}

fn far_end_translation_from(
    links: &Query<(Entity, &ChainLink, &Transform)>,
    root: Entity,
) -> Option<Vec3> {
    links.iter().find_map(|(_, link, transform)| {
        (link.root == root && link.end).then_some(transform.translation)
    })
}

struct AxisHit {
    axis: Axis,
    ray_distance: f32,
    pixels: f32,
}

fn axis_under_cursor(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &GlobalTransform), With<OrbitCamera>>,
    pose: &Transform,
    size: Vec3,
) -> Option<AxisHit> {
    let window = windows.iter().next()?;
    let cursor = window.cursor_position()?;
    let (camera, camera_transform) = cameras.iter().next()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let mut best: Option<AxisHit> = None;
    let mut best_pixels = AXIS_PICK_PIXELS;
    for axis in Axis::ALL {
        let (start, end) = axis_segment(pose, size, axis);
        let Some((along, point, gap)) =
            closest_on_segment(ray.origin, ray.direction.as_vec3(), start, end)
        else {
            continue;
        };
        if gap > AXIS_PICK_GAP {
            continue;
        }
        let Ok(screen) = camera.world_to_viewport(camera_transform, point) else {
            continue;
        };
        let pixels = cursor.distance(screen);
        if pixels <= best_pixels {
            best_pixels = pixels;
            best = Some(AxisHit {
                axis,
                ray_distance: along,
                pixels,
            });
        }
    }
    best
}

fn axis_segment(pose: &Transform, size: Vec3, axis: Axis) -> (Vec3, Vec3) {
    let local = axis.local();
    let half = size.dot(local.abs()) * 0.5;
    let direction = pose.rotation * local;
    let start = pose.translation + direction * (half + AXIS_GAP);
    let end = start + direction * AXIS_LENGTH;
    (start, end)
}

fn closest_on_segment(origin: Vec3, dir: Vec3, start: Vec3, end: Vec3) -> Option<(f32, Vec3, f32)> {
    let dir = dir.normalize_or_zero();
    if dir.length_squared() < 0.5 {
        return None;
    }
    let mut best_gap = f32::MAX;
    let mut best: Option<(f32, Vec3, f32)> = None;
    const STEPS: usize = 32;
    for step in 0..=STEPS {
        let point = start.lerp(end, step as f32 / STEPS as f32);
        let offset = point - origin;
        let along = offset.dot(dir);
        if along < 0.02 {
            continue;
        }
        let gap = (offset - dir * along).length();
        if gap < best_gap {
            best_gap = gap;
            best = Some((along, point, gap));
        }
    }
    best
}

#[cfg(test)]
fn closest_axis(origin: Vec3, dir: Vec3, pose: &Transform, size: Vec3) -> Option<Axis> {
    let mut best: Option<(Axis, f32)> = None;
    for axis in Axis::ALL {
        let (start, end) = axis_segment(pose, size, axis);
        let Some((_, _, gap)) = closest_on_segment(origin, dir, start, end) else {
            continue;
        };
        if gap > AXIS_PICK_GAP {
            continue;
        }
        if best.map(|(_, best_gap)| gap < best_gap).unwrap_or(true) {
            best = Some((axis, gap));
        }
    }
    best.map(|(axis, _)| axis)
}

fn axis_color(axis: Axis, selected: bool) -> Color {
    let color = match axis {
        Axis::X => Color::srgb(0.95, 0.28, 0.22),
        Axis::Y => Color::srgb(0.32, 0.86, 0.3),
        Axis::Z => Color::srgb(0.32, 0.52, 0.98),
    };
    if selected {
        Color::srgb(1.0, 0.94, 0.62)
    } else {
        color
    }
}

fn wheel_lines(wheel: &mut MessageReader<MouseWheel>) -> f32 {
    let mut lines = 0.0;
    for event in wheel.read() {
        lines += match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 60.0,
        };
    }
    lines
}

pub fn release_rope_pins(commands: &mut Commands, root: Entity, joints: &[(Entity, RopeJoint)]) {
    for (entity, joint) in joints {
        if joint.root != root {
            continue;
        }
        if let Some(end) = joint.pinned_end {
            commands.entity(*entity).despawn();
            commands.entity(end).remove::<Sensor>();
        }
    }
}

struct HostBody {
    entity: Entity,
    transform: Transform,
    shape: HostShape,
}

enum HostShape {
    Cuboid(Vec3),
    Ball(f32),
    Capsule { a: Vec3, b: Vec3, radius: f32 },
}

pub fn shift_rope_end(
    commands: &mut Commands,
    root: Entity,
    side: RopeEndSide,
    delta: Vec3,
    parts: &mut Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    links: &mut Query<
        (
            Entity,
            &ChainLink,
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Without<PlayerPart>,
    >,
    colliders: &Query<&Collider>,
    fixtures: &Query<
        (Entity, &Transform, &Part),
        (With<Fixture>, Without<PlayerPart>, Without<ChainLink>),
    >,
    joints: &mut Query<(
        Entity,
        &RopeJoint,
        Option<&mut DistanceJoint>,
        Option<&mut SphericalJoint>,
    )>,
) {
    let Ok((_, near_transform, _, _, _)) = parts.get(root) else {
        return;
    };
    let near0 = near_transform.translation;
    let Some((far_entity, far0)) = links.iter().find_map(|(entity, link, transform, _, _)| {
        (link.root == root && link.end).then_some((entity, transform.translation))
    }) else {
        return;
    };
    let mut near = near0;
    let mut far = far0;
    match side {
        RopeEndSide::Near => near += delta,
        RopeEndSide::Far => far += delta,
    }
    if near.distance(far) < 0.25 {
        let dir = delta.normalize_or(Vec3::X);
        match side {
            RopeEndSide::Near => near = far0 - dir * 0.25,
            RopeEndSide::Far => far = near0 + dir * 0.25,
        }
    }
    let end_entity = match side {
        RopeEndSide::Near => root,
        RopeEndSide::Far => far_entity,
    };
    let end_pos = match side {
        RopeEndSide::Near => near,
        RopeEndSide::Far => far,
    };

    let mut hosts = Vec::new();
    for (entity, transform, _, _, _) in parts.iter() {
        if entity == root {
            continue;
        }
        push_host(&mut hosts, entity, *transform, colliders);
    }
    for (entity, transform, _) in fixtures.iter() {
        push_host(&mut hosts, entity, *transform, colliders);
    }

    retie_rope(root, near, far, parts, links, joints);

    let best = hosts
        .iter()
        .filter(|host| end_overlaps(end_pos, 0.05, host))
        .min_by(|left, right| {
            left.transform
                .translation
                .distance(end_pos)
                .partial_cmp(&right.transform.translation.distance(end_pos))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    let existing = joints.iter().find_map(|(joint_entity, rope, _, _)| {
        (rope.pinned_end == Some(end_entity)).then_some((joint_entity, rope.host))
    });
    if let Some(host) = best {
        let local = host.transform.rotation.inverse() * (end_pos - host.transform.translation);
        if let Some((joint_entity, pinned_host)) = existing {
            if pinned_host == Some(host.entity) {
                if let Ok((_, _, _, mut spherical)) = joints.get_mut(joint_entity) {
                    if let Some(spherical) = spherical.as_mut() {
                        spherical.frame2.anchor = JointAnchor::Local(local);
                    }
                }
            } else {
                commands.entity(joint_entity).despawn();
                pin_rope_end(commands, root, end_entity, host.entity, local);
            }
        } else {
            pin_rope_end(commands, root, end_entity, host.entity, local);
        }
    } else if let Some((joint_entity, _)) = existing {
        commands.entity(joint_entity).despawn();
        commands.entity(end_entity).remove::<Sensor>();
    }
}

fn push_host(
    hosts: &mut Vec<HostBody>,
    entity: Entity,
    transform: Transform,
    colliders: &Query<&Collider>,
) {
    let Ok(collider) = colliders.get(entity) else {
        return;
    };
    let shape = collider.shape_scaled();
    let shape = if let Some(cuboid) = shape.as_cuboid() {
        let half = cuboid.half_extents;
        HostShape::Cuboid(Vec3::new(half.x, half.y, half.z))
    } else if let Some(ball) = shape.as_ball() {
        HostShape::Ball(ball.radius)
    } else if let Some(capsule) = shape.as_capsule() {
        HostShape::Capsule {
            a: Vec3::new(
                capsule.segment.a.x,
                capsule.segment.a.y,
                capsule.segment.a.z,
            ),
            b: Vec3::new(
                capsule.segment.b.x,
                capsule.segment.b.y,
                capsule.segment.b.z,
            ),
            radius: capsule.radius,
        }
    } else {
        return;
    };
    hosts.push(HostBody {
        entity,
        transform,
        shape,
    });
}

fn end_overlaps(end: Vec3, radius: f32, host: &HostBody) -> bool {
    match host.shape {
        HostShape::Ball(host_radius) => {
            end.distance(host.transform.translation) <= host_radius + radius
        }
        HostShape::Cuboid(half) => {
            let local = host.transform.rotation.inverse() * (end - host.transform.translation);
            let grown = half + Vec3::splat(radius);
            local.x.abs() <= grown.x && local.y.abs() <= grown.y && local.z.abs() <= grown.z
        }
        HostShape::Capsule {
            a,
            b,
            radius: host_radius,
        } => {
            let local = host.transform.rotation.inverse() * (end - host.transform.translation);
            let along = b - a;
            let length_sq = along.length_squared();
            let t = if length_sq < 1e-8 {
                0.0
            } else {
                ((local - a).dot(along) / length_sq).clamp(0.0, 1.0)
            };
            local.distance(a + along * t) <= host_radius + radius
        }
    }
}

fn retie_rope(
    root: Entity,
    near: Vec3,
    far: Vec3,
    parts: &mut Query<
        (
            Entity,
            &mut Transform,
            &mut Part,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        (With<PlayerPart>, Without<ChainLink>),
    >,
    links: &mut Query<
        (
            Entity,
            &ChainLink,
            &mut Transform,
            Option<&mut Position>,
            Option<&mut Rotation>,
        ),
        Without<PlayerPart>,
    >,
    joints: &mut Query<(
        Entity,
        &RopeJoint,
        Option<&mut DistanceJoint>,
        Option<&mut SphericalJoint>,
    )>,
) {
    let mut placed = vec![(root, 0u8)];
    for (entity, link, _, _, _) in links.iter() {
        if link.root == root {
            placed.push((entity, link.index));
        }
    }
    placed.sort_by_key(|(_, index)| *index);
    if placed.len() < 2 {
        return;
    }
    let span = far - near;
    let length = span.length();
    if length < 1e-4 {
        return;
    }
    let rotation = Quat::from_rotation_arc(Vec3::Y, span / length);
    let last = (placed.len() - 1) as f32;
    let poses: Vec<_> = placed
        .into_iter()
        .map(|(entity, index)| {
            (
                entity,
                Transform {
                    translation: near.lerp(far, index as f32 / last),
                    rotation,
                    scale: Vec3::ONE,
                },
            )
        })
        .collect();
    for (entity, pose) in poses {
        if entity == root {
            if let Ok((_, mut transform, _, position, rotation)) = parts.get_mut(entity) {
                write_pose(&mut transform, position, rotation, pose);
            }
        } else if let Ok((_, _, mut transform, position, rotation)) = links.get_mut(entity) {
            write_pose(&mut transform, position, rotation, pose);
        }
    }
    let spacing = length / last;
    let rest = spacing * ROPE_SLACK;
    let tip = Vec3::Y * (rest * 0.5);
    let chain: Vec<_> = joints
        .iter()
        .filter_map(|(entity, rope, _, _)| {
            (rope.root == root && rope.pinned_end.is_none()).then_some(entity)
        })
        .collect();
    for entity in chain {
        let Ok((_, _, mut distance, mut spherical)) = joints.get_mut(entity) else {
            continue;
        };
        if let Some(distance) = distance.as_mut() {
            distance.limits.min = 0.0;
            distance.limits.max = spacing;
        }
        if let Some(spherical) = spherical.as_mut() {
            spherical.frame1.anchor = JointAnchor::Local(tip);
            spherical.frame2.anchor = JointAnchor::Local(-tip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose_at(translation: Vec3, rotation: Quat) -> Transform {
        Transform {
            translation,
            rotation,
            scale: Vec3::ONE,
        }
    }

    #[test]
    fn slide_follows_the_part_axis_after_it_has_been_turned() {
        let mut pose = pose_at(
            Vec3::new(1.0, 2.0, 0.25),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
        slide(&mut pose, Axis::X, 2.0);
        assert!(
            (pose.translation.x - 1.0).abs() < 1e-4,
            "{}",
            pose.translation.x
        );
        assert!(
            (pose.translation.y - 4.0).abs() < 1e-4,
            "{}",
            pose.translation.y
        );
        assert!((pose.translation.z - 0.25).abs() < 1e-4);
    }

    #[test]
    fn spin_turns_around_the_axis_and_keeps_the_pivot() {
        let pivot = Vec3::new(0.4, 1.2, 0.25);
        let mut pose = pose_at(pivot, Quat::IDENTITY);
        spin(&mut pose, Axis::Y, std::f32::consts::FRAC_PI_2);
        assert!(pose.translation.distance(pivot) < 1e-4);
        let pointed = pose.rotation * Vec3::X;
        assert!(
            pointed.distance(Vec3::new(0.0, 0.0, -1.0)) < 1e-4,
            "{pointed:?}"
        );
    }

    #[test]
    fn a_ray_aimed_at_one_arrow_picks_that_axis() {
        let pose = pose_at(Vec3::ZERO, Quat::IDENTITY);
        let size = Vec3::splat(0.2);
        let y = axis_segment(&pose, size, Axis::Y);
        let aim = (y.0 + y.1) * 0.5;
        let origin = aim + Vec3::X;
        let picked = closest_axis(origin, -Vec3::X, &pose, size);
        assert_eq!(picked, Some(Axis::Y));

        let x = axis_segment(&pose, size, Axis::X);
        let aim = (x.0 + x.1) * 0.5;
        let picked = closest_axis(aim + Vec3::Y, -Vec3::Y, &pose, size);
        assert_eq!(picked, Some(Axis::X));

        let missed = closest_axis(Vec3::new(0.0, 3.0, 0.0), Vec3::X, &pose, size);
        assert_eq!(missed, None);
    }

    #[test]
    fn a_turned_part_picks_its_local_axis() {
        let pose = pose_at(
            Vec3::new(1.0, 1.0, 0.0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
        let size = Vec3::splat(0.2);
        let local_x = axis_segment(&pose, size, Axis::X);
        let aim = (local_x.0 + local_x.1) * 0.5;
        let picked = closest_axis(aim + Vec3::Z, -Vec3::Z, &pose, size);
        assert_eq!(picked, Some(Axis::X));
    }

    #[test]
    fn tilting_a_board_carries_a_ball_that_was_sitting_on_it() {
        let board = pose_at(Vec3::new(0.0, 2.0, 0.25), Quat::IDENTITY);
        let mut tilted = board;
        tilted.rotation = Quat::from_rotation_z(-0.4);
        let ball = pose_at(Vec3::new(-0.2, 2.0 + 0.06 + 0.16, 0.25), Quat::IDENTITY);
        let half = PartKind::Board.spec().size * 0.5;
        let reach = support_reach(PartKind::Ball);
        assert!(resting_on(ball.translation, board, half, reach));
        assert!(!resting_on(ball.translation + Vec3::Y, board, half, reach));

        let carried = carried_transform(board, tilted, ball);
        let kept = tilted.rotation.inverse() * (carried.translation - tilted.translation);
        let original = board.rotation.inverse() * (ball.translation - board.translation);
        assert!(
            kept.distance(original) < 1e-4,
            "ball left the board, local {kept:?} vs {original:?}"
        );
        assert!(carried.translation.distance(ball.translation) > 0.05);
    }

    #[test]
    fn a_ball_slid_along_the_fixed_ramp_drops_onto_the_slope() {
        let angle = -0.28_f32;
        let pose = pose_at(Vec3::new(-1.35, 2.8, 0.25), Quat::from_rotation_z(angle));
        let half = Vec3::new(3.3, 0.12, 0.5) * 0.5;
        let supports = [(pose, half)];
        let normal = Quat::from_rotation_z(angle) * Vec3::Y;
        let radius = 0.16;
        let resting = pose.translation + normal * (half.y + radius);
        let stayed = rest_on_slope(resting, radius, &supports, 0.35);
        assert!(
            stayed.distance(resting) < 1.0e-3,
            "a ball already on the slope was moved to {stayed:?}"
        );

        let slid = resting + Vec3::new(0.4, 0.0, 0.0);
        let seated = rest_on_slope(slid, radius, &supports, 0.35);
        assert!(
            seated.y < slid.y - 0.05,
            "ball stayed level at {seated:?} instead of following the ramp"
        );
        assert!((seated.x - slid.x).abs() < 1.0e-3);
        let expected_drop = 0.4 * angle.tan().abs();
        assert!(
            (slid.y - seated.y - expected_drop).abs() < 0.02,
            "drop {} expected about {expected_drop}",
            slid.y - seated.y
        );
    }

    #[test]
    fn a_click_on_the_ramp_top_seats_the_ball_on_the_slope() {
        let angle = -0.28_f32;
        let pose = pose_at(Vec3::new(-1.35, 2.8, 0.25), Quat::from_rotation_z(angle));
        let half = Vec3::new(3.3, 0.12, 0.5) * 0.5;
        let normal = Quat::from_rotation_z(angle) * Vec3::Y;
        let top = pose.translation + normal * half.y;
        let origin = top + Vec3::new(0.4, 2.0, 3.0);
        let direction = Dir3::new((top - origin).normalize()).expect("ray");
        let seated = point_on_supports(origin, direction, PartKind::Ball, 0.0, &[(pose, half)])
            .expect("the ramp top should catch the click");
        let expected = top + normal * 0.16;
        assert!(
            seated.distance(expected) < 0.03,
            "seated {seated:?}, expected {expected:?}"
        );
    }

    #[test]
    fn a_click_on_the_ramp_front_still_seats_the_ball_on_top() {
        let angle = -0.28_f32;
        let pose = pose_at(Vec3::new(-1.35, 2.8, 0.25), Quat::from_rotation_z(angle));
        let half = Vec3::new(3.3, 0.12, 0.5) * 0.5;
        let front = pose.translation + Vec3::Z * half.z;
        let seated = point_on_supports(
            front + Vec3::Z * 2.0,
            Dir3::NEG_Z,
            PartKind::Ball,
            0.0,
            &[(pose, half)],
        )
        .expect("the ramp should catch a click on its front");
        let normal = Quat::from_rotation_z(angle) * Vec3::Y;
        let top_edge = pose.translation + pose.rotation * Vec3::new(0.0, half.y, half.z);
        let expected = top_edge + normal * 0.16;
        assert!(
            seated.distance(expected) < 0.05,
            "seated {seated:?}, expected {expected:?}"
        );
    }
}
