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
- Click a shelf part, then click the scene to place it. A click on the fixed ramp or another piece sets the part on that slope. Click a placed part to select it.
- The side panel edits position, rotation around X, Y, and Z, and stickiness. Sticky parts are nailed down and do not tumble. Delete removes the selected part and returns it to the shelf.
- A selected shelf part shows three axis arrows. Click one. Scroll slides the part along that axis. Ctrl+scroll turns it around that axis.
- A loose ball sitting on a shelf part stays with that part while you tilt or slide it. Slide the ball along the fixed ramp and it stays on that slope. It rolls when you press **Play**.
- **Q** and **E** turn the selected part around the picked axis, or around its own Z axis when no arrow is picked. They also tilt the part you are about to place.
- Right drag orbits the camera. Middle drag pans. Scroll zooms, unless an axis arrow is picked. The camera keeps moving while the machine runs.

Level pieces that ship with the table cannot be moved. Only the shelf parts can.

## How the picture gets onto the screen

Drawing goes through Bevy's wgpu backend: Vulkan on Linux, and WebGL or WebGPU in the browser. That is what uses the GPU, including an NVIDIA card, on both targets.

The physics step is [Avian](https://github.com/avianphysics/avian), running on the CPU. A CUDA solver cannot run inside the web page, and a second solver would make the Linux build and the page disagree. Colliders are cuboids and spheres so a crowded table stays cheap. The native build turns on Avian's parallel solver. The web build leaves that off, because Rayon does not run in the browser.

## Checks

```bash
cargo test
```

The tests parse both level files, check the dwell timer, and play "A Rolling Start" through to the win, then stop and confirm the ball is back where you placed it.
