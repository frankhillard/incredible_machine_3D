use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;

mod edit;
mod goal;
mod level;
mod parts;
mod scene;
mod sim;
mod ui;

pub use goal::{advance_dwell, GameState};
pub use level::builtin_levels;

pub fn run() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.08, 0.05, 0.03)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Contraption".into(),
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            PhysicsPlugins::default().with_collision_hooks::<parts::RopeContactHooks>(),
            EguiPlugin::default(),
        ));
    sim::add_sim_systems(&mut app);
    app.init_resource::<edit::AxisGizmo>()
        .add_systems(Startup, (scene::setup_camera, edit::configure_gizmos))
        .add_systems(OnEnter(GameState::Playing), edit::clear_axis_gizmo)
        .add_systems(
            Update,
            (
                (
                    edit::edit_keys,
                    edit::gizmo_scroll,
                    edit::orbit_camera,
                    edit::draw_axes,
                )
                    .chain(),
                edit::unbury_balls,
                edit::sync_ghost,
                edit::delete_selected,
                edit::highlight_selection,
            ),
        )
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (ui::draw_ui, edit::pointer).chain(),
        )
        .run();
}

#[cfg(test)]
mod play_test {
    use super::*;
    use crate::goal::GoalClock;
    use crate::level::{self, Placement};
    use crate::parts::{ChainLink, Fixture, Part, PartKind, PlayerPart, RopeJoint};
    use crate::scene::Palette;
    use crate::sim::PlaySnapshot;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[derive(Resource)]
    struct Scripted(Vec<Placement>);

    fn spawn_scripted(
        mut commands: Commands,
        mut scripted: ResMut<Scripted>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        palette: Res<Palette>,
    ) {
        for placement in scripted.0.drain(..) {
            level::spawn_player_part(
                &mut commands,
                &mut meshes,
                &mut materials,
                &palette,
                placement,
            );
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            bevy::state::app::StatesPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins(
            PhysicsPlugins::default().with_collision_hooks::<crate::parts::RopeContactHooks>(),
        );
        sim::add_sim_systems(&mut app);
        app.add_systems(Update, spawn_scripted.after(sim::apply_level_ops));
        app
    }

    fn ball_translation(app: &mut App) -> Option<Vec3> {
        let mut query = app.world_mut().query::<(&Part, &Transform)>();
        let world = app.world();
        query.iter(world).find_map(|(part, transform)| {
            (part.kind == PartKind::Ball).then_some(transform.translation)
        })
    }

    #[test]
    fn rolling_start_reaches_the_basket_and_stop_puts_the_ball_back() {
        let level = builtin_levels().remove(0);
        let placements = level::rolling_start_placements(&level);
        let ball_start = Vec3::from_array(placements[1].position);

        let mut app = test_app();
        app.insert_resource(Scripted(placements.to_vec()));
        app.finish();
        app.cleanup();
        app.update();

        let spawned = ball_translation(&mut app).expect("the scripted ball should spawn");
        assert!(
            spawned.distance(ball_start) < 0.05,
            "spawned at {spawned:?}, expected {ball_start:?}"
        );

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);

        let mut won = false;
        let mut moved = false;
        let mut trace = String::new();
        for frame in 0..1_500 {
            app.update();
            let Some(position) = ball_translation(&mut app) else {
                trace.push_str(&format!("{frame}: ball missing\n"));
                continue;
            };
            if position.distance(ball_start) > 0.45 {
                moved = true;
            }
            let held = app.world().resource::<GoalClock>().held;
            if frame % 45 == 0 {
                trace.push_str(&format!("{frame}: {position:?} held={held:.2}\n"));
            }
            if *app.world().resource::<State<GameState>>().get() == GameState::Won {
                won = true;
                trace.push_str(&format!("won at frame {frame}: {position:?}\n"));
                break;
            }
        }
        assert!(moved, "the ball never left the ramp\n{trace}");
        assert!(won, "the ball never stayed in the end zone\n{trace}");

        let saved_ball = app
            .world()
            .resource::<PlaySnapshot>()
            .poses
            .as_ref()
            .expect("play should keep a snapshot")
            .iter()
            .find(|(_, transform)| transform.translation.distance(ball_start) < 0.05)
            .map(|(_, transform)| transform.translation)
            .expect("snapshot should include the ball");

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Editing);
        app.update();
        let restored = ball_translation(&mut app).expect("the ball should still exist");
        assert!(
            restored.distance(saved_ball) < 1e-2,
            "stop left the ball at {restored:?}, snapshot was {saved_ball:?}"
        );
    }

    #[test]
    fn level_1_springs_carry_the_ball_into_the_basket() {
        let levels = builtin_levels();
        let level = &levels[3];
        let placements = level::level1_placements(level);
        let ball_start = Vec3::from_array(
            level
                .fixtures
                .iter()
                .find(|fixture| fixture.kind == PartKind::Ball)
                .expect("level 1 starts with a loose ball")
                .position,
        );

        let mut app = test_app();
        app.insert_resource(Scripted(placements.to_vec()));
        app.insert_resource(level::LevelOps {
            go_to: Some(3),
            reload: false,
        });
        app.finish();
        app.cleanup();
        app.update();

        let spawned = ball_translation(&mut app).expect("the level ball should spawn");
        assert!(
            spawned.distance(ball_start) < 0.05,
            "spawned at {spawned:?}, expected {ball_start:?}"
        );

        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);

        let mut won = false;
        let mut trace = String::new();
        for frame in 0..1_500 {
            app.update();
            let Some(position) = ball_translation(&mut app) else {
                trace.push_str(&format!("{frame}: ball missing\n"));
                continue;
            };
            let held = app.world().resource::<GoalClock>().held;
            if frame % 45 == 0 {
                trace.push_str(&format!("{frame}: {position:?} held={held:.2}\n"));
            }
            if *app.world().resource::<State<GameState>>().get() == GameState::Won {
                won = true;
                trace.push_str(&format!("won at frame {frame}: {position:?}\n"));
                break;
            }
        }
        assert!(
            won,
            "the springs did not land the ball in the basket\n{trace}"
        );
    }

    #[test]
    fn a_ball_carried_onto_a_tilted_board_then_rolls_down_it() {
        let mut app = test_app();
        app.insert_resource(Scripted(Vec::new()));
        app.finish();
        app.cleanup();
        app.update();

        let board_pos = Vec3::new(0.0, 8.0, 0.25);
        let flat = Transform::from_translation(board_pos);
        let tilted = flat.with_rotation(Quat::from_rotation_z(-0.4));
        let ball_flat = Transform::from_translation(board_pos + Vec3::new(-0.25, 0.06 + 0.16, 0.0));
        let ball_pose = crate::edit::carried_transform(flat, tilted, ball_flat);

        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(1.55, 0.12, 0.5),
            Friction::new(0.18),
            Restitution::new(0.0),
            tilted,
            Position(tilted.translation),
            Rotation(tilted.rotation),
        ));
        let ball = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Collider::sphere(0.16),
                Friction::new(0.16),
                Restitution::new(0.0),
                Mass(0.7),
                LinearDamping(0.04),
                AngularDamping(0.2),
                ball_pose,
                Position(ball_pose.translation),
                Rotation(ball_pose.rotation),
                LinearVelocity::ZERO,
                AngularVelocity::ZERO,
            ))
            .id();

        for _ in 0..4 {
            app.update();
        }
        app.world_mut().resource_mut::<Time<Physics>>().unpause();
        let start = ball_pose.translation;
        let mut last = start;
        for _ in 0..40 {
            app.update();
            last = app.world().get::<Transform>(ball).unwrap().translation;
        }
        assert!(
            last.x > start.x + 0.08,
            "ball stayed at {last:?} instead of rolling down from {start:?}"
        );
        assert!(last.y > 7.0, "ball left the board and fell to {last:?}");
    }

    #[test]
    fn a_ball_on_the_fixed_ramp_rolls_down_the_slope() {
        let mut app = test_app();
        app.insert_resource(Scripted(Vec::new()));
        app.finish();
        app.cleanup();
        app.update();

        let level = builtin_levels().remove(0);
        let ramp = level
            .fixtures
            .iter()
            .max_by(|left, right| left.rotation_z.abs().total_cmp(&right.rotation_z.abs()))
            .unwrap();
        let angle = ramp.rotation_z;
        let along = Vec2::new(angle.cos(), angle.sin());
        let normal = Vec2::new(-angle.sin(), angle.cos());
        let center = Vec2::new(ramp.position[0], ramp.position[1]);
        let uphill = center - along * (ramp.size[0] * 0.5);
        let radius = 0.16;
        let ball_xy = uphill + normal * (ramp.size[1] * 0.5 + radius) + along * 0.35;
        let start = Vec3::new(ball_xy.x, ball_xy.y, ramp.position[2]);

        let ball = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Collider::sphere(radius),
                Friction::new(0.16),
                Restitution::new(0.0),
                Mass(0.7),
                Transform::from_translation(start),
                Position(start),
                Rotation(Quat::IDENTITY),
                LinearVelocity::ZERO,
                AngularVelocity::ZERO,
            ))
            .id();

        for _ in 0..4 {
            app.update();
        }
        app.world_mut().resource_mut::<Time<Physics>>().unpause();
        let mut last = start;
        for _ in 0..90 {
            app.update();
            last = app.world().get::<Transform>(ball).unwrap().translation;
        }
        let dx = last.x - start.x;
        let slope = (last.y - start.y) / dx;
        assert!(dx > 0.4, "ball stayed at {last:?}");
        assert!(
            (slope - angle.tan()).abs() < 0.08,
            "ball slope {slope}, ramp is {}",
            angle.tan()
        );
        assert!(last.y > 2.2, "ball fell off the ramp early at {last:?}");
    }

    #[test]
    fn a_ball_dropped_on_a_spring_bounces() {
        let mut app = test_app();
        let spring_y = 12.0;
        let start_y = 13.4;
        app.insert_resource(Scripted(vec![
            Placement {
                kind: PartKind::Spring,
                position: [0.0, spring_y, 0.25],
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                sticky: true,
                ends: [[0.0; 3]; 2],
            },
            Placement {
                kind: PartKind::Ball,
                position: [0.0, start_y, 0.25],
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                sticky: false,
                ends: [[0.0; 3]; 2],
            },
        ]));
        app.finish();
        app.cleanup();
        app.update();

        let spring_material = app.world().resource::<Palette>().spring.clone();
        let mut materials = app
            .world_mut()
            .query::<(&Part, &MeshMaterial3d<StandardMaterial>)>();
        let painted = materials
            .iter(app.world())
            .find(|(part, _)| part.kind == PartKind::Spring)
            .map(|(_, material)| material.0.clone())
            .expect("the spring should spawn");
        assert_eq!(painted, spring_material);
        let color = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(&spring_material)
            .expect("spring material")
            .base_color
            .to_srgba();
        assert!(
            (color.red - color.green).abs() < 0.08 && (color.green - color.blue).abs() < 0.08,
            "spring should be gray, got {color:?}"
        );
        assert!(
            color.red < 0.75 && color.red > 0.35,
            "spring gray is {color:?}"
        );

        app.world_mut().resource_mut::<Time<Physics>>().unpause();
        let mut lowest = start_y;
        let mut highest_after = 0.0_f32;
        for _ in 0..180 {
            app.update();
            let y = ball_translation(&mut app).expect("ball").y;
            lowest = lowest.min(y);
            if lowest < start_y - 0.5 {
                highest_after = highest_after.max(y);
            }
        }
        assert!(
            lowest < spring_y + 0.55,
            "ball never reached the spring, lowest {lowest}"
        );
        assert!(
            highest_after > lowest + 0.45,
            "ball reached {lowest} and only came back to {highest_after}"
        );
    }

    #[test]
    fn a_pinned_rope_sags_between_its_ends() {
        let placement = Placement {
            kind: PartKind::Rope,
            position: [0.0, 8.0, 0.25],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: true,
            ends: [[0.0; 3]; 2],
        };
        let mut app = test_app();
        app.insert_resource(Scripted(vec![placement]));
        app.finish();
        app.cleanup();
        app.update();

        let mut distance_query = app.world_mut().query::<&DistanceJoint>();
        let distances = distance_query.iter(app.world()).count();
        let mut socket_query = app.world_mut().query::<&SphericalJoint>();
        let sockets = socket_query.iter(app.world()).count();
        assert_eq!(distances, 2, "the two end pairs are distance joints");
        assert_eq!(sockets, 5, "the interior pairs are spherical joints");
        assert_eq!(
            app.world().resource::<SubstepCount>().0,
            12,
            "the rope solver should take twelve substeps"
        );

        let (root, near, far_entity, far) = rope_ends(&mut app);
        for end in [root, far_entity] {
            assert_eq!(
                app.world().get::<RigidBody>(end).copied(),
                Some(RigidBody::Dynamic),
                "a rope end stays loose until it is pinned"
            );
        }
        for (end, place) in [(root, near), (far_entity, far)] {
            let anchor = app
                .world_mut()
                .spawn((
                    RigidBody::Static,
                    Transform::from_translation(place),
                    Position(place),
                    Rotation(Quat::IDENTITY),
                ))
                .id();
            app.world_mut().spawn(
                SphericalJoint::new(end, anchor)
                    .with_local_anchor1(Vec3::ZERO)
                    .with_local_anchor2(Vec3::ZERO),
            );
        }

        let start_middle = rope_middle_y(&mut app);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        for _ in 0..120 {
            app.update();
        }
        let sagged = rope_middle_y(&mut app);
        let droop = start_middle - sagged;
        assert!(
            droop > -0.02 && droop < 0.12,
            "a rope should give a little under its own weight, middle went from {start_middle} to {sagged}"
        );

        let ends = rope_end_ys(&mut app);
        assert!(
            ends.iter().all(|y| (y - 8.0).abs() < 0.05),
            "pinned ends should stay put, got {ends:?}"
        );
    }

    #[test]
    fn dragging_a_rope_end_onto_a_loose_board_pins_that_end() {
        let rope = Placement {
            kind: PartKind::Rope,
            position: [0.0, 12.0, 0.25],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: false,
            ends: [[0.0; 3]; 2],
        };
        let board = Placement {
            kind: PartKind::Board,
            position: [2.2, 12.0, 0.25],
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: 0.0,
            sticky: false,
            ends: [[0.0; 3]; 2],
        };
        let mut app = test_app();
        app.insert_resource(Scripted(vec![rope, board]));
        app.add_systems(Update, drag_the_far_end.after(spawn_scripted));
        app.finish();
        app.cleanup();
        app.update();

        let mut pins = app.world_mut().query::<&RopeJoint>();
        let pinned: Vec<_> = pins
            .iter(app.world())
            .filter_map(|joint| {
                joint
                    .host
                    .map(|host| (joint.pinned_end.expect("pin"), host))
            })
            .collect();
        assert_eq!(pinned.len(), 1, "the end that met the board should pin");
        let (end, host) = pinned[0];
        assert_eq!(
            app.world().get::<Part>(host).map(|part| part.kind),
            Some(PartKind::Board)
        );
        assert_eq!(
            app.world().get::<RigidBody>(host).copied(),
            Some(RigidBody::Dynamic),
            "the pin has to follow a loose part"
        );
        assert_eq!(
            app.world().get::<RigidBody>(end).copied(),
            Some(RigidBody::Dynamic)
        );
        let mut sockets = app.world_mut().query::<&SphericalJoint>();
        assert!(
            sockets
                .iter(app.world())
                .any(|joint| joint.body1 == end && joint.body2 == host),
            "the end should be jointed to the board"
        );
    }

    fn drag_the_far_end(
        mut once: Local<bool>,
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
        colliders: Query<&Collider>,
        fixtures: Query<
            (Entity, &Transform, &Part),
            (
                With<crate::parts::Fixture>,
                Without<PlayerPart>,
                Without<ChainLink>,
            ),
        >,
        mut joints: Query<(
            Entity,
            &RopeJoint,
            Option<&mut DistanceJoint>,
            Option<&mut SphericalJoint>,
        )>,
    ) {
        if *once {
            return;
        }
        *once = true;
        let Some(root) = parts
            .iter()
            .find_map(|(entity, _, part, _, _)| (part.kind == PartKind::Rope).then_some(entity))
        else {
            return;
        };
        super::edit::shift_rope_end(
            &mut commands,
            root,
            super::edit::RopeEndSide::Far,
            Vec3::X * 1.4,
            &mut parts,
            &mut links,
            &colliders,
            &fixtures,
            &mut joints,
        );
    }

    fn rope_ends(app: &mut App) -> (Entity, Vec3, Entity, Vec3) {
        let mut parts = app.world_mut().query::<(Entity, &Part, &Transform)>();
        let (root, near) = parts
            .iter(app.world())
            .find_map(|(entity, part, transform)| {
                (part.kind == PartKind::Rope).then_some((entity, transform.translation))
            })
            .expect("rope");
        let mut links = app.world_mut().query::<(Entity, &ChainLink, &Transform)>();
        let (far_entity, far) = links
            .iter(app.world())
            .find_map(|(entity, link, transform)| {
                (link.root == root && link.end).then_some((entity, transform.translation))
            })
            .expect("far end");
        (root, near, far_entity, far)
    }

    fn rope_middle_y(app: &mut App) -> f32 {
        let mut query = app.world_mut().query::<(&ChainLink, &Transform)>();
        let world = app.world();
        query
            .iter(world)
            .min_by(|(_, a), (_, b)| {
                a.translation
                    .x
                    .abs()
                    .partial_cmp(&b.translation.x.abs())
                    .unwrap()
            })
            .map(|(_, transform)| transform.translation.y)
            .expect("rope links")
    }

    #[test]
    fn level_3_rope_spans_the_authored_end_joints() {
        let levels = builtin_levels();
        let rope = levels[5]
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Rope)
            .expect("level 3 rope");
        let ends = rope.ends;
        let near = Vec3::from_array(ends[0]);
        let far = Vec3::from_array(ends[1]);

        let mut app = test_app();
        app.insert_resource(Scripted(Vec::new()));
        app.insert_resource(level::LevelOps {
            go_to: Some(5),
            reload: false,
        });
        app.finish();
        app.cleanup();
        app.update();

        let mut parts = app.world_mut().query::<(&Part, &Transform, &Fixture)>();
        let world = app.world();
        let spawned_near = parts
            .iter(world)
            .find(|(part, _, _)| part.kind == PartKind::Rope)
            .map(|(_, transform, _)| transform.translation)
            .expect("rope root");
        assert!(
            spawned_near.distance(near) < 1e-3,
            "near end at {spawned_near:?}, expected {near:?}"
        );

        let mut links = app.world_mut().query::<(&ChainLink, &Transform)>();
        let world = app.world();
        let spawned_far = links
            .iter(world)
            .find(|(link, _)| link.end)
            .map(|(_, transform)| transform.translation)
            .expect("far link");
        assert!(
            spawned_far.distance(far) < 1e-3,
            "far end at {spawned_far:?}, expected {far:?}"
        );

        let mut pins = app.world_mut().query::<&RopeJoint>();
        let world = app.world();
        let hosts: Vec<Entity> = pins.iter(world).filter_map(|joint| joint.host).collect();
        assert_eq!(hosts.len(), 2, "each end should pin to the part it sits in");

        let mut kinds = app.world_mut().query::<&Part>();
        let world = app.world();
        let mut pinned: Vec<PartKind> = hosts
            .iter()
            .filter_map(|entity| kinds.get(world, *entity).ok().map(|part| part.kind))
            .collect();
        pinned.sort_by_key(|kind| kind.slot());
        assert_eq!(pinned, vec![PartKind::Block, PartKind::Plank]);
    }

    #[test]
    fn level_4_weight_hangs_from_the_rope_without_being_thrown() {
        let levels = builtin_levels();
        let rope = levels[7]
            .fixtures
            .iter()
            .find(|fixture| fixture.kind == PartKind::Rope)
            .expect("level 5 rope");
        let anchor = Vec3::from_array(rope.ends[0]);
        let start = Vec3::from_array(rope.ends[1]);
        let reach = anchor.distance(start);

        let mut app = test_app();
        app.insert_resource(Scripted(Vec::new()));
        app.insert_resource(level::LevelOps {
            go_to: Some(7),
            reload: false,
        });
        app.finish();
        app.cleanup();
        app.update();

        let mut parts = app.world_mut().query::<(Entity, &Part)>();
        let weight = parts
            .iter(app.world())
            .find_map(|(entity, part)| (part.kind == PartKind::Weight).then_some(entity))
            .expect("weight");
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        let mut fastest = 0.0_f32;
        let mut trace = String::new();
        for frame in 0..180 {
            app.update();
            let position = app.world().get::<Transform>(weight).unwrap().translation;
            let speed = app
                .world()
                .get::<LinearVelocity>(weight)
                .map(|velocity| velocity.length())
                .unwrap_or(0.0);
            fastest = fastest.max(speed);
            if frame % 30 == 0 {
                trace.push_str(&format!("{frame}: {position:?} speed={speed:.2}\n"));
            }
            assert!(
                (position.z - start.z).abs() < 0.05,
                "the weight left the lane at {position:?}\n{trace}"
            );
            assert!(
                position.y < anchor.y,
                "the rope threw the weight above its anchor at {position:?}\n{trace}"
            );
            assert!(
                position.distance(anchor) < reach * 1.25,
                "the weight escaped the rope at {position:?}\n{trace}"
            );
        }
        assert!(
            fastest < 4.0,
            "the weight was kicked to {fastest:.2} m/s\n{trace}"
        );
    }

    fn rope_end_ys(app: &mut App) -> Vec<f32> {
        let mut query = app.world_mut().query::<(&Part, &Transform)>();
        let world = app.world();
        let mut ys = Vec::new();
        if let Some((_, transform)) = query
            .iter(world)
            .find(|(part, _)| part.kind == PartKind::Rope)
        {
            ys.push(transform.translation.y);
        }
        let mut links = app.world_mut().query::<(&ChainLink, &Transform)>();
        let world = app.world();
        if let Some((_, transform)) = links.iter(world).find(|(link, _)| link.end) {
            ys.push(transform.translation.y);
        }
        ys
    }
}
