use avian3d::prelude::*;
use bevy::prelude::*;

use crate::parts::{Part, PartKind};

pub fn advance_dwell(held: f32, inside: bool, dt: f32) -> f32 {
    if inside && dt > 0.0 {
        held + dt
    } else if inside {
        held
    } else {
        0.0
    }
}

pub fn point_in_obb(world_point: Vec3, center: Vec3, rotation: Quat, half: Vec3) -> bool {
    let local = rotation.inverse() * (world_point - center);
    local.x.abs() <= half.x + 1e-4
        && local.y.abs() <= half.y + 1e-4
        && local.z.abs() <= half.z + 1e-4
}

#[derive(Component)]
pub struct GoalZone {
    pub half_extents: Vec3,
}

#[derive(Resource)]
pub struct GoalClock {
    pub held: f32,
    pub required: f32,
    pub max_speed: f32,
    pub kind: PartKind,
}

impl Default for GoalClock {
    fn default() -> Self {
        Self {
            held: 0.0,
            required: 4.0,
            max_speed: 0.8,
            kind: PartKind::Ball,
        }
    }
}

pub fn watch_goal(
    time: Res<Time>,
    mut clock: ResMut<GoalClock>,
    mut next: ResMut<NextState<GameState>>,
    zones: Query<(&Transform, &GoalZone)>,
    parts: Query<(&Part, &Transform, Option<&Position>, Option<&LinearVelocity>)>,
) {
    let Some((zone_transform, zone)) = zones.iter().next() else {
        return;
    };
    let mut inside = false;
    for (part, transform, position, velocity) in &parts {
        if part.kind != clock.kind {
            continue;
        }
        let point = position.map(|position| position.0).unwrap_or(transform.translation);
        let speed = velocity.map(|velocity| velocity.length()).unwrap_or(0.0);
        if speed <= clock.max_speed
            && point_in_obb(
                point,
                zone_transform.translation,
                zone_transform.rotation,
                zone.half_extents,
            )
        {
            inside = true;
            break;
        }
    }
    clock.held = advance_dwell(clock.held, inside, time.delta_secs());
    if clock.held >= clock.required {
        next.set(GameState::Won);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
pub enum GameState {
    #[default]
    Editing,
    Playing,
    Won,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwell_grows_while_the_ball_stays_and_resets_when_it_leaves() {
        let held = advance_dwell(1.5, true, 0.25);
        assert!((held - 1.75).abs() < 1e-5);
        assert_eq!(advance_dwell(held, false, 0.1), 0.0);
        let almost = advance_dwell(3.9, true, 0.2);
        assert!((almost - 4.1).abs() < 1e-5);
    }

    #[test]
    fn a_point_inside_a_rotated_box_is_detected() {
        let center = Vec3::new(1.0, 2.0, 0.25);
        let rotation = Quat::from_rotation_z(-0.4);
        let half = Vec3::new(0.5, 0.2, 0.2);
        let inside = center + rotation * Vec3::new(0.4, 0.0, 0.0);
        let outside = center + rotation * Vec3::new(0.0, 0.35, 0.0);
        assert!(point_in_obb(inside, center, rotation, half));
        assert!(!point_in_obb(outside, center, rotation, half));
    }
}
