use bevy::prelude::*;

use crate::level::LANE_Z;

#[derive(Resource)]
pub struct Palette {
    pub board: Handle<StandardMaterial>,
    pub block: Handle<StandardMaterial>,
    pub plank: Handle<StandardMaterial>,
    pub ball: Handle<StandardMaterial>,
    pub selected: Handle<StandardMaterial>,
    pub ghost: Handle<StandardMaterial>,
    pub frame: Handle<StandardMaterial>,
    pub felt: Handle<StandardMaterial>,
    pub goal: Handle<StandardMaterial>,
}

impl Palette {
    pub fn for_kind(&self, kind: crate::parts::PartKind) -> Handle<StandardMaterial> {
        match kind {
            crate::parts::PartKind::Board => self.board.clone(),
            crate::parts::PartKind::Block => self.block.clone(),
            crate::parts::PartKind::Plank => self.plank.clone(),
            crate::parts::PartKind::Ball => self.ball.clone(),
        }
    }
}

#[derive(Component)]
pub struct Scenery;

pub fn setup_palette(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wood = |materials: &mut Assets<StandardMaterial>, color: Color, rough: f32| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: rough,
            ..default()
        })
    };
    commands.insert_resource(Palette {
        board: wood(&mut materials, Color::srgb(0.77, 0.54, 0.29), 0.72),
        block: wood(&mut materials, Color::srgb(0.52, 0.33, 0.18), 0.78),
        plank: wood(&mut materials, Color::srgb(0.86, 0.69, 0.42), 0.68),
        ball: materials.add(StandardMaterial {
            base_color: Color::srgb(0.89, 0.29, 0.24),
            perceptual_roughness: 0.32,
            metallic: 0.04,
            ..default()
        }),
        selected: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.78, 0.35),
            perceptual_roughness: 0.4,
            ..default()
        }),
        ghost: materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.82, 0.45, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        frame: wood(&mut materials, Color::srgb(0.23, 0.14, 0.09), 0.85),
        felt: wood(&mut materials, Color::srgb(0.12, 0.42, 0.27), 0.92),
        goal: materials.add(StandardMaterial {
            base_color: Color::srgba(0.45, 0.9, 0.5, 0.28),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
    });
}

pub fn spawn_frame(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<Palette>,
) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(1.0, 0.94, 0.86),
        brightness: 280.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::new(0.5, 2.0, LANE_Z), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 3_500.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 4.0, 3.0).looking_at(Vec3::new(0.0, 2.0, LANE_Z), Vec3::Y),
    ));

    scenery_box(
        &mut commands,
        &mut meshes,
        palette.felt.clone(),
        Vec3::new(0.4, -0.15, LANE_Z),
        Vec3::new(14.0, 0.3, 1.8),
    );
    scenery_box(
        &mut commands,
        &mut meshes,
        palette.frame.clone(),
        Vec3::new(0.4, 3.3, -0.55),
        Vec3::new(14.0, 7.4, 0.28),
    );
    scenery_box(
        &mut commands,
        &mut meshes,
        palette.frame.clone(),
        Vec3::new(-6.7, 3.2, LANE_Z),
        Vec3::new(0.28, 7.0, 1.4),
    );
    scenery_box(
        &mut commands,
        &mut meshes,
        palette.frame.clone(),
        Vec3::new(7.5, 3.2, LANE_Z),
        Vec3::new(0.28, 7.0, 1.4),
    );

    // One invisible stop at the front edge of the felt. The wooden backboard
    // already closes the other side. Walls used to stand against the boards,
    // so a ball on a slope hit an unseen rail and could not fall off.
    let felt_front = LANE_Z + 0.9;
    lane_wall(&mut commands, felt_front + 0.04);
}

fn scenery_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Scenery,
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(material),
        Transform::from_translation(position),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(size.x, size.y, size.z),
        avian3d::prelude::Position(position),
        avian3d::prelude::Friction::new(0.6),
    ));
}

fn lane_wall(commands: &mut Commands, z: f32) {
    let position = Vec3::new(0.4, 3.2, z);
    let size = Vec3::new(13.6, 6.6, 0.08);
    commands.spawn((
        Scenery,
        Transform::from_translation(position),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(size.x, size.y, size.z),
        avian3d::prelude::Position(position),
        avian3d::prelude::Friction::new(0.2),
    ));
}

#[derive(Component)]
pub struct OrbitCamera {
    pub focus: Vec3,
    pub radius: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl OrbitCamera {
    pub fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.radius * self.pitch.cos() * self.yaw.sin(),
            self.radius * self.pitch.sin(),
            self.radius * self.pitch.cos() * self.yaw.cos(),
        );
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}

pub fn setup_camera(mut commands: Commands) {
    let orbit = OrbitCamera {
        focus: Vec3::new(0.6, 2.15, LANE_Z),
        radius: 8.8,
        yaw: 0.42,
        pitch: 0.38,
    };
    commands.spawn((
        Camera3d::default(),
        Projection::from(PerspectiveProjection {
            fov: 50.0_f32.to_radians(),
            ..default()
        }),
        orbit.transform(),
        orbit,
    ));
}
