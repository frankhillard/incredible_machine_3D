//! Play the current level for a few seconds and write what it did.
//!
//! `.objspec` lists every part, rope link, and scenery collider once.
//! `.datapos` gets one pose line per object per displayed frame.
//! `.mp4` is those same frames, when `ffmpeg` is on the path.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{Monitor, PrimaryMonitor, PrimaryWindow};

use crate::goal::GameState;
use crate::parts::{ChainLink, Ghost, Part, PartKind, PlayerPart};
use crate::scene::Scenery;

/// One take. Dropping it closes the pose log and asks ffmpeg to finish the video.
#[derive(Resource)]
pub struct Recording {
    folder: PathBuf,
    name: String,
    duration: f32,
    elapsed: f32,
    frame: u32,
    started: bool,
    closing: bool,
    close_wait: f32,
    pending: u32,
    fps: u32,
    objects: Vec<Entity>,
    datapos: std::fs::File,
    video: Mutex<Video>,
}

enum Video {
    Waiting,
    Live(Encoder),
    Skipped,
}

struct Encoder {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    width: u32,
    height: u32,
}

impl Drop for Encoder {
    fn drop(&mut self) {
        self.stdin.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

impl Encoder {
    fn start(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        let size = format!("{width}x{height}");
        let rate = fps.max(1).to_string();
        let mut child = Command::new("ffmpeg")
            .arg("-y")
            .arg("-loglevel")
            .arg("error")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(&size)
            .arg("-r")
            .arg(&rate)
            .arg("-i")
            .arg("-")
            .arg("-an")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| err.to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "ffmpeg did not accept frames".to_string())?;
        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            width,
            height,
        })
    }

    fn write_frame(&mut self, pixels: &[u8]) -> std::io::Result<()> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "ffmpeg is closed",
            ));
        };
        stdin.write_all(pixels)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.datapos.flush();
        if let Ok(mut video) = self.video.lock() {
            let _ = std::mem::replace(&mut *video, Video::Skipped);
        }
    }
}

/// `/tmp/run1.mp4` becomes the folder `/tmp` and the stem `run1`.
pub fn split_record_path(path: &Path) -> Option<(PathBuf, String)> {
    let name = path.file_stem()?.to_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some((folder, name.to_string()))
}

/// Start a take in `folder` named `name`, and switch the machine to play.
///
/// The pose log and the object spec are written even when ffmpeg is missing.
/// A desktop build without a window still writes those two files and skips the video.
pub fn record(world: &mut World, folder: &Path, name: &str, duration_secs: f32) {
    if cfg!(target_arch = "wasm32") {
        error!("Recording needs the desktop build.");
        return;
    }
    if world.get_resource::<Recording>().is_some() {
        return;
    }
    let objects = catalog(world);
    if let Err(err) = std::fs::create_dir_all(folder) {
        error!("could not record the level: {err}");
        return;
    }
    let spec_path = folder.join(format!("{name}.objspec"));
    let data_path = folder.join(format!("{name}.datapos"));
    let mut spec = String::new();
    for (index, object) in objects.iter().enumerate() {
        spec.push_str(&object.line(index));
        spec.push('\n');
    }
    if let Err(err) = std::fs::write(&spec_path, spec) {
        error!("could not write {}: {err}", spec_path.display());
        return;
    }
    let datapos = match std::fs::File::create(&data_path) {
        Ok(file) => file,
        Err(err) => {
            error!("could not write {}: {err}", data_path.display());
            return;
        }
    };
    let entities = objects.into_iter().map(|object| object.entity).collect();
    let fps = monitor_hz(world);
    world.insert_resource(Recording {
        folder: folder.to_path_buf(),
        name: name.to_string(),
        duration: duration_secs.max(0.0),
        elapsed: 0.0,
        frame: 0,
        started: false,
        closing: false,
        close_wait: 0.0,
        pending: 0,
        fps,
        objects: entities,
        datapos,
        video: Mutex::new(Video::Waiting),
    });
    world
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
}

struct Catalogued {
    entity: Entity,
    kind: String,
    role: &'static str,
    size: Vec3,
    friction: f32,
    restitution: f32,
    mass: f32,
    sticky: u8,
}

impl Catalogued {
    fn line(&self, index: usize) -> String {
        format!(
            "{index} {} {} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {}",
            self.kind,
            self.role,
            self.size.x,
            self.size.y,
            self.size.z,
            self.friction,
            self.restitution,
            self.mass,
            self.sticky
        )
    }
}

fn catalog(world: &mut World) -> Vec<Catalogued> {
    let mut objects = Vec::new();
    let mut parts = world.query_filtered::<(Entity, &Part, Option<&PlayerPart>), (
        Without<ChainLink>,
        Without<Ghost>,
    )>();
    let mut listed = Vec::new();
    for (entity, part, player) in parts.iter(world) {
        listed.push((entity.index(), entity, *part, player.is_some()));
    }
    listed.sort_by_key(|(index, _, _, _)| *index);
    for (_, entity, part, player) in listed {
        let spec = part.kind.spec();
        objects.push(Catalogued {
            entity,
            kind: kind_name(part.kind),
            role: if player { "player" } else { "fixture" },
            size: spec.size,
            friction: spec.friction,
            restitution: spec.restitution,
            mass: spec.mass,
            sticky: if part.sticky { 1 } else { 0 },
        });
    }

    let mut links = world.query_filtered::<(
        Entity,
        &Collider,
        Option<&Friction>,
        Option<&Restitution>,
        Option<&Mass>,
    ), (With<ChainLink>, Without<Ghost>)>();
    let mut listed = Vec::new();
    for (entity, collider, friction, restitution, mass) in links.iter(world) {
        listed.push((
            entity.index(),
            entity,
            collider_size(collider),
            friction
                .map(|friction| friction.dynamic_coefficient)
                .unwrap_or(0.0),
            restitution
                .map(|restitution| restitution.coefficient)
                .unwrap_or(0.0),
            mass.map(|mass| mass.0).unwrap_or(0.0),
        ));
    }
    listed.sort_by_key(|(index, _, _, _, _, _)| *index);
    for (_, entity, size, friction, restitution, mass) in listed {
        objects.push(Catalogued {
            entity,
            kind: "rope".to_string(),
            role: "link",
            size,
            friction,
            restitution,
            mass,
            sticky: 0,
        });
    }

    let mut scenery =
        world.query_filtered::<(Entity, Option<&Collider>), (With<Scenery>, Without<Ghost>)>();
    let mut listed = Vec::new();
    for (entity, collider) in scenery.iter(world) {
        let size = collider.map(collider_size).unwrap_or(Vec3::ZERO);
        listed.push((entity.index(), entity, size));
    }
    listed.sort_by_key(|(index, _, _)| *index);
    for (_, entity, size) in listed {
        objects.push(Catalogued {
            entity,
            kind: "scenery".to_string(),
            role: "scenery",
            size,
            friction: 0.0,
            restitution: 0.0,
            mass: 0.0,
            sticky: 0,
        });
    }
    objects
}

fn kind_name(kind: PartKind) -> String {
    kind.label().to_ascii_lowercase()
}

fn collider_size(collider: &Collider) -> Vec3 {
    let extents = collider.shape_scaled().compute_local_aabb().extents();
    Vec3::new(extents.x, extents.y, extents.z)
}

fn monitor_hz(world: &mut World) -> u32 {
    let mut monitors = world.query_filtered::<&Monitor, With<PrimaryMonitor>>();
    let hz = monitors.iter(world).find_map(|monitor| {
        monitor
            .refresh_rate_millihertz
            .map(|millihertz| (millihertz as f32 / 1000.0).round() as u32)
    });
    match hz {
        Some(hz) if hz > 0 => hz,
        _ => 60,
    }
}

/// While the take is running, append one pose per object for each displayed frame.
pub fn sample_recording(
    mut commands: Commands,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    time: Res<Time>,
    recording: Option<ResMut<Recording>>,
    windows: Query<(), With<PrimaryWindow>>,
    transforms: Query<&Transform>,
) {
    let Some(mut recording) = recording else {
        return;
    };
    let playing = matches!(state.get(), GameState::Playing | GameState::Won);
    if recording.closing || (recording.started && !playing) {
        recording.closing = true;
        recording.close_wait += time.delta_secs();
        if recording.pending == 0 || recording.close_wait > 2.0 {
            commands.remove_resource::<Recording>();
        }
        return;
    }
    if !playing {
        return;
    }
    recording.started = true;
    let frame = recording.frame;
    let stamp = recording.elapsed;
    let objects = recording.objects.clone();
    for (index, entity) in objects.into_iter().enumerate() {
        let Ok(transform) = transforms.get(entity) else {
            continue;
        };
        let position = transform.translation;
        let rotation = transform.rotation;
        let _ = writeln!(
            recording.datapos,
            "{frame} {stamp:.4} {index} {:.4} {:.4} {:.4} {:.5} {:.5} {:.5} {:.5}",
            position.x, position.y, position.z, rotation.x, rotation.y, rotation.z, rotation.w
        );
    }
    recording.frame += 1;
    recording.elapsed += time.delta_secs();

    let skipped = recording
        .video
        .lock()
        .map(|video| matches!(*video, Video::Skipped))
        .unwrap_or(true);
    if !skipped && windows.iter().next().is_some() {
        recording.pending += 1;
        commands
            .spawn(Screenshot::primary_window())
            .observe(store_captured_frame);
    }
    if recording.elapsed >= recording.duration {
        recording.closing = true;
        next.set(GameState::Editing);
        if recording.pending == 0 {
            commands.remove_resource::<Recording>();
        }
    }
}

fn store_captured_frame(
    shot: On<ScreenshotCaptured>,
    recording: Option<ResMut<Recording>>,
    mut commands: Commands,
) {
    let Some(mut recording) = recording else {
        return;
    };
    if recording.pending > 0 {
        recording.pending -= 1;
    }
    push_frame(&mut recording, &shot.image);
    if recording.closing && recording.pending == 0 {
        commands.remove_resource::<Recording>();
    }
}

fn push_frame(recording: &mut Recording, image: &Image) {
    let Ok(mut video) = recording.video.lock() else {
        return;
    };
    if matches!(*video, Video::Skipped) {
        return;
    }
    let fit = match &*video {
        Video::Live(encoder) => Some((encoder.width, encoder.height)),
        _ => None,
    };
    let Some((width, height, pixels)) = rgba_frame(image, fit) else {
        error!(
            "the captured frame could not be encoded, so {}.mp4 was skipped",
            recording.name
        );
        *video = Video::Skipped;
        return;
    };
    if matches!(*video, Video::Waiting) {
        let path = recording.folder.join(format!("{}.mp4", recording.name));
        match Encoder::start(&path, width, height, recording.fps) {
            Ok(encoder) => *video = Video::Live(encoder),
            Err(err) => {
                error!(
                    "ffmpeg is missing, so {}.mp4 was skipped: {err}",
                    recording.name
                );
                *video = Video::Skipped;
                return;
            }
        }
    }
    let failed = match &mut *video {
        Video::Live(encoder) => encoder.write_frame(&pixels).is_err(),
        _ => false,
    };
    if failed {
        error!(
            "ffmpeg stopped, so the rest of {}.mp4 was skipped",
            recording.name
        );
        *video = Video::Skipped;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_path_keeps_the_folder_and_the_file_stem() {
        let (folder, name) = split_record_path(Path::new("/tmp/run1.mp4")).unwrap();
        assert_eq!(folder, Path::new("/tmp"));
        assert_eq!(name, "run1");
        assert_eq!(
            folder.join(format!("{name}.datapos")),
            Path::new("/tmp/run1.datapos")
        );
        assert_eq!(
            folder.join(format!("{name}.objspec")),
            Path::new("/tmp/run1.objspec")
        );
        assert_eq!(
            folder.join(format!("{name}.mp4")),
            Path::new("/tmp/run1.mp4")
        );
    }
}

fn rgba_frame(image: &Image, fit: Option<(u32, u32)>) -> Option<(u32, u32, Vec<u8>)> {
    let data = image.data.as_ref()?;
    let src_w = image.texture_descriptor.size.width;
    let src_h = image.texture_descriptor.size.height;
    let swap = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
        _ => return None,
    };
    let (width, height) = fit.unwrap_or((src_w - src_w % 2, src_h - src_h % 2));
    if width == 0 || height == 0 || width > src_w || height > src_h {
        return None;
    }
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let start = ((y * src_w + x) * 4) as usize;
            let px = data.get(start..start + 4)?;
            if swap {
                pixels.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            } else {
                pixels.extend_from_slice(px);
            }
        }
    }
    Some((width, height, pixels))
}
