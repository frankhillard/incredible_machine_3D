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
        .add_plugins((PhysicsPlugins::default(), EguiPlugin::default()));
    sim::add_sim_systems(&mut app);
    app        .init_resource::<edit::AxisGizmo>()
        .add_systems(Startup, (scene::setup_camera, edit::configure_gizmos))
        .add_systems(OnEnter(GameState::Playing), edit::clear_axis_gizmo)
        .add_systems(
            Update,
            (
                (edit::edit_keys, edit::gizmo_scroll, edit::orbit_camera, edit::draw_axes).chain(),
                edit::unbury_balls,
                edit::sync_ghost,
                edit::delete_selected,
                edit::highlight_selection,
            ),
        )
        .add_systems(bevy_egui::EguiPrimaryContextPass, (ui::draw_ui, edit::pointer).chain())
        .run();
}

#[cfg(test)]
mod play_test {
    use super::*;
    use crate::goal::GoalClock;
    use crate::level::{self, Placement};
    use crate::parts::{Part, PartKind};
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
            .add_plugins(PhysicsPlugins::default());
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
        assert!(won, "the springs did not land the ball in the basket\n{trace}");
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
        assert!(
            last.y > 7.0,
            "ball left the board and fell to {last:?}"
        );
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
            },
            Placement {
                kind: PartKind::Ball,
                position: [0.0, start_y, 0.25],
                rotation_x: 0.0,
                rotation_y: 0.0,
                rotation_z: 0.0,
                sticky: false,
            },
        ]));
        app.finish();
        app.cleanup();
        app.update();

        let spring_material = app.world().resource::<Palette>().spring.clone();
        let mut materials = app.world_mut().query::<(&Part, &MeshMaterial3d<StandardMaterial>)>();
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
        assert!(color.red < 0.75 && color.red > 0.35, "spring gray is {color:?}");

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
}
