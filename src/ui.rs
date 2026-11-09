use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::edit::{self, AxisGizmo};
use crate::goal::{GameState, GoalClock};
use crate::level::{Campaign, LevelOps};
use crate::parts::{ChainLink, Part, PlayerPart, RopeJoint};
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
    joints: Query<(Entity, &RopeJoint)>,
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
            if ui.add_enabled(editing, egui::Button::new("Save")).clicked() {
                ops.save = true;
            }
            ui.separator();
            if ui
                .add_enabled(editing, egui::Button::new("Previous"))
                .clicked()
            {
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
                        let Some((old_pose, kind, sticky_now)) = parts
                            .get(entity)
                            .ok()
                            .map(|(_, transform, part, _, _)| (*transform, part.kind, part.sticky))
                        else {
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
                        let (euler_x, euler_y, euler_z) = old_pose.rotation.to_euler(EulerRot::XYZ);
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
                        if kind == crate::parts::PartKind::Rope {
                            ui.label("Slide an end's arrows onto a part to pin that end.");
                        } else if kind == crate::parts::PartKind::Seesaw {
                            ui.label("Pinned at its center. It tips around the axis along its short side.");
                        } else if ui.checkbox(&mut sticky, "Sticky (nailed)").changed() {
                            if let Ok((_, _, mut part, _, _)) = parts.get_mut(entity) {
                                part.sticky = sticky;
                            }
                            let body = if sticky {
                                RigidBody::Static
                            } else {
                                RigidBody::Dynamic
                            };
                            commands.entity(entity).insert(body);
                            for (link_entity, link, _, _, _) in &links {
                                if link.root == entity && link.end {
                                    commands.entity(link_entity).insert(body);
                                }
                            }
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
                            if kind.round() && !sticky && moved && !lifted {
                                let mut surfaces = edit::static_cuboids(&supports);
                                for (_, transform, part, _, _) in &parts {
                                    if part.sticky
                                        && !part.kind.round()
                                        && part.kind != crate::parts::PartKind::Rope
                                    {
                                        surfaces.push((*transform, part.kind.spec().size * 0.5));
                                    }
                                }
                                let radius = kind.spec().size.x * 0.5;
                                pose.translation =
                                    edit::rest_on_slope(pose.translation, radius, &surfaces, 0.35);
                            }
                            if kind == crate::parts::PartKind::Rope {
                                let pins: Vec<_> = joints
                                    .iter()
                                    .map(|(joint_entity, joint)| (joint_entity, *joint))
                                    .collect();
                                edit::release_rope_pins(&mut commands, entity, &pins);
                            }
                            edit::apply_pose(&mut parts, &mut links, entity, pose);
                        }
                        ui.add_space(8.0);
                        if ui.button("Remove").clicked() {
                            refund(&mut shelf, &Part { kind, sticky });
                            let link_ids: Vec<_> = links
                                .iter()
                                .filter(|(_, link, _, _, _)| link.root == entity)
                                .map(|(link_entity, _, _, _, _)| link_entity)
                                .collect();
                            let joint_ids: Vec<_> = joints
                                .iter()
                                .filter(|(_, joint)| joint.root == entity)
                                .map(|(joint_entity, _)| joint_entity)
                                .collect();
                            edit::despawn_chain(&mut commands, entity, link_ids, joint_ids);
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

pub fn save_level_file(
    state: Res<State<GameState>>,
    mut ops: ResMut<LevelOps>,
    campaign: Res<Campaign>,
    shelf: Res<Shelf>,
    parts: Query<(Entity, &Part, &Transform), With<PlayerPart>>,
    links: Query<(&ChainLink, &Transform)>,
) {
    if !ops.save {
        return;
    }
    ops.save = false;
    if *state.get() != GameState::Editing {
        return;
    }
    let placed = parts.iter().map(|(entity, part, transform)| {
        let far = links.iter().find_map(|(link, link_transform)| {
            (link.root == entity && link.end).then_some(link_transform.translation)
        });
        crate::level::fixture_from_placement(
            part.kind,
            part.sticky,
            transform.translation,
            transform.rotation,
            far,
        )
    });
    let snapshot = crate::level::snapshot_level(campaign.current(), &shelf.remaining, placed);
    let text = match crate::level::level_ron(&snapshot) {
        Ok(text) => text,
        Err(err) => {
            error!("could not write the level file: {err}");
            return;
        }
    };
    if let Err(err) = prompt_save(&suggested_ron_name(&snapshot.name), &text) {
        error!("could not save the level: {err}");
    }
}

fn suggested_ron_name(name: &str) -> String {
    let mut file = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            file.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-') && !file.ends_with('_') {
            file.push('_');
        }
    }
    let file = file.trim_matches('_');
    if file.is_empty() {
        "level.ron".into()
    } else {
        format!("{file}.ron")
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn prompt_save(file_name: &str, contents: &str) -> Result<(), String> {
    let Some(mut path) = rfd::FileDialog::new()
        .set_title("Save level")
        .add_filter("Level", &["ron"])
        .set_file_name(file_name)
        .save_file()
    else {
        return Ok(());
    };
    write_ron(path, contents)
}

#[cfg(target_os = "linux")]
fn prompt_save(file_name: &str, contents: &str) -> Result<(), String> {
    let output = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--save",
            "--confirm-overwrite",
            "--title=Save level",
            "--file-filter=Level | *.ron",
        ])
        .arg(format!("--filename={file_name}"))
        .output()
        .map_err(|_| "Install zenity to choose where the level file is saved.".to_string())?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return if err.is_empty() { Ok(()) } else { Err(err) };
    }
    let chosen = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if chosen.is_empty() {
        return Ok(());
    }
    write_ron(chosen.into(), contents)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn prompt_save(_file_name: &str, _contents: &str) -> Result<(), String> {
    Err("Save the level from the desktop build, which can ask for a file path.".into())
}

fn write_ron(mut path: std::path::PathBuf, contents: &str) -> Result<(), String> {
    if path.extension().is_none() {
        path.set_extension("ron");
    }
    std::fs::write(&path, contents).map_err(|err| err.to_string())
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
