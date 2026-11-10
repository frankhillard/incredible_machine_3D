# Contraption

A 3D puzzle in the spirit of a Rube Goldberg machine. Fixed pieces stay where the level nailed them. You place the parts from the shelf, turn them on any axis, and decide which ones are sticky. Press Play and the loose pieces tumble. The machine works when a ball stays in the green end zone for four seconds.

## Linux

The project pins Rust 1.99.1 in `rust-toolchain.toml` because Bevy 0.20 needs a newer compiler than an older default toolchain.

```bash
cargo run --release
```

`cargo run` without `--release` also works. Dependencies are optimized so the physics step stays responsive while you are iterating.

## Web page

The same crate compiles to WebAssembly. Physics stays on the CPU in both builds, so a machine that works on Linux behaves the same way in the page.

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

Install a `wasm-bindgen-cli` that matches the `wasm-bindgen` version in `Cargo.lock`, then bind the module and serve the page:

```bash
cargo install wasm-bindgen-cli --version 0.2.130
wasm-bindgen --out-dir web/pkg --target web \
  target/wasm32-unknown-unknown/release/incredible_machine_3d.wasm
python3 -m http.server 8080
```

Open [http://localhost:8080](http://localhost:8080). The page is `index.html`, and it loads the module from `web/pkg`.

## Controls

- **Play** starts the physics. Placement and the property panel turn off until **Stop**.
- **Stop** puts every part back where you set it and clears the timer.
- **Reset** clears the parts you placed and reloads the level.
- **Save** is available while you are editing. It asks where to write a `.ron` file of the goal, the fixed pieces, the parts you placed, and the counts left on the shelf. It is off while the machine is running.
- Click a shelf part, then click the scene to place it. A click on the fixed ramp or another piece sets the part on that slope. Click a placed part to select it.
- The side panel edits position, rotation around X, Y, and Z, and stickiness. Sticky parts are nailed down and do not tumble. Delete removes the selected part and returns it to the shelf.
- A selected shelf part shows three axis arrows. Click one. Scroll slides the part along that axis. Ctrl+scroll turns it around that axis.
- A selected rope shows those arrows at each end. Scroll slides that end. If the end meets another part, the part lights up and the end pins to it, loose parts included. Until then the ends hang free.
- A loose ball sitting on a shelf part stays with that part while you tilt or slide it. Slide the ball along the fixed ramp and it stays on that slope. It rolls when you press **Play**.
- **Q** and **E** turn the selected part around the picked axis, or around its own Z axis when no arrow is picked. They also tilt the part you are about to place.
- Right drag orbits the camera. Middle drag pans. Scroll zooms, unless an axis arrow is picked. The camera keeps moving while the machine runs.

Level pieces that ship with the table cannot be moved. Only the shelf parts can.

## Level pieces

A level file in `assets/levels` is [RON](https://github.com/ron-rs/ron). It names the goal, the pieces already on the table, and the counts on the shelf. Pieces that ship with the level stay where they were nailed. The shelf is what you place.

- **Board.** A long flat board, sticky by default, so it usually stays put and other parts can sit on it.
- **Block.** A short chunky block, sticky by default, with a grippy surface.
- **Plank.** A thinner board. It is loose by default and will tumble unless you nail it.
- **Ball.** A light sphere. Most goals ask this ball to rest in the green end zone.
- **Spring.** A small pad with a strong bounce, sticky by default.
- **Rope.** Eight linked capsules. You place it by its two ends. An end that meets another part pins there, and a loose part can be that pin.
- **Weight.** A sphere the size of the ball, and much heavier.
- **Seesaw.** A plank hinged through its center. It tips around the axis along its short side, and that hinge follows the way you turn it. It is never nailed down.

The felt, the wooden frame, and the invisible front stop are part of the table. They are not pieces you place.

## How the picture gets onto the screen

[Bevy](https://bevy.org/) draws the scene. Drawing goes through Bevy's wgpu backend: Vulkan on Linux, and WebGL or WebGPU in the browser. That uses the GPU whenever the backend can reach one, including an NVIDIA card.

[Avian](https://github.com/avianphysics/avian) runs the physics on the CPU, with cuboid, sphere, and rope-capsule colliders. The Linux build turns on Avian's parallel solver through [Rayon](https://github.com/rayon-rs/rayon). The web build leaves that solver on one thread, because Rayon is not supported in the browser. Both builds share this CPU solver, so a machine that works on Linux behaves the same way in the page.

## Dependencies

- **Rust 1.99.1**, pinned in `rust-toolchain.toml`, with the `wasm32-unknown-unknown` target for the web build.
- **[Bevy](https://bevy.org/) 0.20** runs the app, the window, and the scene. Drawing goes through Bevy's wgpu backend: Vulkan on Linux, and WebGL or WebGPU in the browser.
- **[Avian](https://github.com/avianphysics/avian) 0.8** runs the physics on the CPU. The Linux build enables its `parallel` feature, which uses Rayon. The web build leaves that feature off.
- **[bevy_egui](https://github.com/mvlabat/bevy_egui) 0.43** draws the shelf, the part panel, and the buttons.
- **[Serde](https://serde.rs/) 1** and **[RON](https://github.com/ron-rs/ron) 0.10** read and write the level files.
- **[rfd](https://github.com/PolyMeilex/rfd) 0.15** opens the Save and Record dialogs on Windows and macOS. Linux uses Zenity for those dialogs.
- **wasm-bindgen 0.2.130** turns the web build into the module in `web/pkg`.

## Checks

```bash
cargo test
```

The tests parse the level files, check the dwell timer, and play "A Rolling Start" through to the win, then stop and confirm the ball is back where you placed it.

## with Docker

The project contains a dockerfile in order to run the game in a docker container.
You can start a container that serves the web page (It copies wasm and index.html files and run the `python3 -m http.server 8080` command).

Build the docker image

```
docker build -t contraption-web .
docker run --rm -p 8080:8080 contraption-web
```

Run the docker container (on port 8080)
