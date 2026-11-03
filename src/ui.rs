use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::edit::{self, AxisGizmo};
use crate::goal::{GameState, GoalClock};
use crate::level::{Campaign, LevelOps};
use crate::parts::{Part, PlayerPart};
use crate::sim::{refund, Selection, Shelf, UiPointer};

pub fn draw_ui(
    mut contexts: EguiContexts,
    mut styled: Local<bool>,
    state: Res<State<GameState>>,
    campaign: Res<Campaign>,
    clock: Res<GoalClock>,
    mut shelf: ResMut<Shelf>,
    mut selection: ResMut<Selection>,
    gizmo: Res<AxisGizmo>,
    mut ops: ResMut<LevelOps>,
    mut next: ResMut<NextState<GameState>>,
    mut ui_pointer: ResMut<UiPointer>,
    mut commands: Commands,
    mut parts: Query<(
        Entity,
        &mut Transform,
        &mut Part,
        Option<&mut Position>,
        Option<&mut Rotation>,
    ), With<PlayerPart>>,
    supports: Query<(&Transform, &Collider, &RigidBody), Without<PlayerPart>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !*styled {
        style_ui(ctx);
        *styled = true;
    }

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    let level = campaign.current();
    let editing = *state.get() == GameState::Editing;
    let running = *state.get() == GameState::Playing;
    let brass = egui::Color32::from_rgb(225, 177, 90);
    let cream = egui::Color32::from_rgb(255, 246, 228);

    egui::Panel::top("bar").show(&mut viewport, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.heading(egui::RichText::new("Contraption").color(cream));
            ui.label("Build the machine. Then let it run.");
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "MACHINE {} OF {}",
                    campaign.index + 1,
                    campaign.levels.len()
                ))
                .color(brass)
                .small(),
            );
            ui.heading(level.name.clone());
        });
        ui.label(level.hint.clone());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(editing, egui::Button::new("Play").fill(brass))
                .clicked()
            {
                next.set(GameState::Playing);
            }
            if ui
                .add_enabled(!editing, egui::Button::new("Stop"))
                .clicked()
            {
                next.set(GameState::Editing);
            }
            if ui.button("Reset").clicked() {
                ops.reload = true;
            }
            ui.separator();
            if ui.add_enabled(editing, egui::Button::new("Previous")).clicked() {
                ops.go_to = Some(campaign.index + campaign.levels.len() - 1);
            }
            if ui.add_enabled(editing, egui::Button::new("Next")).clicked() {
                ops.go_to = Some(campaign.index + 1);
            }
            ui.separator();
            if running || *state.get() == GameState::Won {
                ui.label(format!(
                    "{} held  {:.1} / {:.0} s",
                    clock.kind.label(),
                    clock.held.min(clock.required),
                    clock.required
                ));
            } else if let Some(axis) = gizmo.axis {
                ui.label(format!(
                    "Scroll slides on {}. Ctrl+scroll turns around it.",
                    axis.label()
                ));
            } else {
                ui.label("Right drag orbits. Middle drag pans. Scroll zooms.");
            }
        });
    });

    egui::Panel::bottom("shelf").show(&mut viewport, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Parts");
            ui.weak("Click a part, then click the scene. Pick an axis arrow: scroll slides, Ctrl+scroll turns.");
            for kind in crate::parts::PartKind::ALL {
                let count = shelf.remaining[kind.slot()];
                let armed = shelf.armed == Some(kind);
                let text = format!("{}  {count}", kind.label());
                let mut button = egui::Button::new(text);
                if armed {
                    button = button.fill(brass);
                }
                let response = ui.add_enabled(editing && (count > 0 || armed), button);
                if response.clicked() {
                    shelf.armed = if armed { None } else { Some(kind) };
                    shelf.tilt = 0.0;
                    if shelf.armed.is_some() {
                        selection.0 = None;
                    }
                }
            }
        });
    });

    if editing {
        if let Some(entity) = selection.0 {
            if parts.get(entity).is_ok() {
                egui::Panel::right("inspector")
                    .resizable(false)
                    .default_size(240.0)
                    .show(&mut viewport, |ui| {
                        ui.heading("Part");
                        ui.label("Level pieces stay put. This one is yours.");
                        let Some((old_pose, kind, sticky_now)) = parts.get(entity).ok().map(
                            |(_, transform, part, _, _)| (*transform, part.kind, part.sticky),
                        ) else {
                            return;
                        };
                        ui.label(kind.label());
                        if let Some(axis) = gizmo.axis {
                            ui.label(format!(
                                "{} axis picked. Scroll slides. Ctrl+scroll turns.",
                                axis.label()
                            ));
                        }
                        let mut position_value = old_pose.translation;
                        let (euler_x, euler_y, euler_z) =
                            old_pose.rotation.to_euler(EulerRot::XYZ);
                        let mut degrees = Vec3::new(
                            euler_x.to_degrees(),
                            euler_y.to_degrees(),
                            euler_z.to_degrees(),
                        );
                        let mut moved = false;
                        let mut turned = false;
                        ui.horizontal(|ui| {
                            moved |= drag(ui, "x", &mut position_value.x);
                            moved |= drag(ui, "y", &mut position_value.y);
                            moved |= drag(ui, "z", &mut position_value.z);
                        });
                        ui.horizontal(|ui| {
                            turned |= drag_angle(ui, "x", &mut degrees.x);
                            turned |= drag_angle(ui, "y", &mut degrees.y);
                            turned |= drag_angle(ui, "z", &mut degrees.z);
                        });
                        let mut sticky = sticky_now;
                        if ui.checkbox(&mut sticky, "Sticky (nailed)").changed() {
                            if let Ok((_, _, mut part, _, _)) = parts.get_mut(entity) {
                                part.sticky = sticky;
                            }
                            commands.entity(entity).insert(if sticky {
                                RigidBody::Static
                            } else {
                                RigidBody::Dynamic
                            });
                        }
                        if moved || turned {
                            let mut pose = old_pose;
                            if moved {
                                pose.translation = position_value;
                            }
                            if turned {
                                pose.rotation = Quat::from_euler(
                                    EulerRot::XYZ,
                                    degrees.x.to_radians(),
                                    degrees.y.to_radians(),
                                    degrees.z.to_radians(),
                                );
                            }
                            let lifted = position_value.y > old_pose.translation.y + 0.001;
                            if kind == crate::parts::PartKind::Ball && !sticky && moved && !lifted {
                                let mut surfaces = edit::static_cuboids(&supports);
                                for (_, transform, part, _, _) in &parts {
                                    if part.sticky && part.kind != crate::parts::PartKind::Ball {
                                        surfaces.push((*transform, part.kind.spec().size * 0.5));
                                    }
                                }
                                let radius = kind.spec().size.x * 0.5;
                                pose.translation =
                                    edit::rest_on_slope(pose.translation, radius, &surfaces, 0.35);
                            }
                            edit::apply_pose(&mut parts, entity, pose);
                        }
                        ui.add_space(8.0);
                        if ui.button("Remove").clicked() {
                            refund(&mut shelf, &Part { kind, sticky });
                            commands.entity(entity).despawn();
                            selection.0 = None;
                        }
                    });
            } else {
                selection.0 = None;
            }
        }
    }

    if *state.get() == GameState::Won {
        egui::Window::new("Machine complete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.heading("It works");
                ui.label(level.name.clone());
                ui.horizontal(|ui| {
                    if ui.button("Tinker again").clicked() {
                        next.set(GameState::Editing);
                    }
                    if ui
                        .add(egui::Button::new("Next machine").fill(brass))
                        .clicked()
                    {
                        ops.go_to = Some(campaign.index + 1);
                    }
                });
            });
    }

    ui_pointer.blocks = ctx.is_pointer_over_egui() || ctx.egui_wants_pointer_input();
    ui_pointer.keys = ctx.egui_wants_keyboard_input();
}

fn drag_angle(ui: &mut egui::Ui, axis: &str, degrees: &mut f32) -> bool {
    ui.add(
        egui::DragValue::new(degrees)
            .speed(0.4)
            .suffix(format!("° {axis}")),
    )
    .changed()
}

fn drag(ui: &mut egui::Ui, prefix: &str, value: &mut f32) -> bool {
    ui.add(
        egui::DragValue::new(value)
            .speed(0.01)
            .prefix(format!("{prefix} ")),
    )
    .changed()
}

fn style_ui(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
    let cream = egui::Color32::from_rgb(255, 246, 228);
    let walnut = egui::Color32::from_rgb(90, 59, 36);
    style.visuals.panel_fill = egui::Color32::from_rgb(44, 27, 18);
    style.visuals.window_fill = egui::Color32::from_rgb(58, 36, 22);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(28, 16, 10);
    style.visuals.override_text_color = Some(cream);
    style.visuals.widgets.noninteractive.fg_stroke.color = cream;
    style.visuals.widgets.inactive.fg_stroke.color = cream;
    style.visuals.widgets.hovered.fg_stroke.color = cream;
    style.visuals.widgets.inactive.weak_bg_fill = walnut;
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(138, 90, 50);
    style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(196, 140, 70);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(225, 177, 90);
    });
}
