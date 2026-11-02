# GraphBeX

GraphBeX computes and draws the layout of large graphs in 2D and 3D. It is written in Rust with Bevy and wgpu.

## Features

- Reads Edge List, GraphML, GML and DOT files, with the graph stored in compact CSR form.
- Three layout algorithms, in 2D and 3D: Random, Spectral and Force-Directed (Fruchterman-Reingold with Barnes-Hut).
- The layout runs on its own threads, and the drawing updates while it converges.
- Camera navigation, vertex selection with its neighborhood, and color by vertex attribute.
- Export of the layout (CSV) and of a screenshot (PNG).
- Live CPU, RAM, VRAM, FPS and time per iteration.

GraphBeX treats every graph as static and undirected. It has been tested on Windows 11.

## Requirements

- Rust 1.85 or newer, installed with [rustup](https://rustup.rs).
- A GPU with Vulkan, Metal, or DirectX 12.
- 8 GB of RAM. The first build is the heaviest step.

## Windows

1. Install Rust from [rustup.rs](https://rustup.rs). When the installer asks, also install the Visual Studio C++ Build Tools.
2. In this folder, run:

```powershell
cargo run --release
```

## Linux

1. Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Install the libraries Bevy needs. On Ubuntu or Debian:

```bash
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev
```

For other distributions, see [Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md). The GPU also needs a Vulkan driver (Mesa for AMD and Intel, the proprietary driver for NVIDIA).

3. In this folder, run:

```bash
cargo run --release
```

## Usage

1. Open a graph with File > Open Graph (Ctrl+O). It appears with a random layout.
2. Choose the algorithm, 2D or 3D and the parameters in the side panel.
3. Run Layout (F5) and watch it converge. Stop interrupts it.
4. In 3D, rotate with the left button, pan with the right button and zoom with the wheel. In 2D, any button pans. Home frames the graph again.
5. Click a vertex to see its degree, position and neighbors.

## Notes

- Run the commands from this folder. The file dialog opens in a path relative to it.
- The first build compiles Bevy and takes several minutes. If it runs out of memory, use `cargo build --release -j 1`. Do not interrupt a build halfway, as that leaves broken files behind.
- `cargo test --release` runs the tests.

## Sample graphs

`data/graph/` holds compressed sample graphs, and `data/graph_sources.md` lists where each one comes from. Extract them before loading. GraphBeX picks the format by the file extension:

| Format | Extension |
|---|---|
| Edge List | `.el`, `.csv`, `.txt` |
| GraphML | `.graphml` |
| GML | `.gml` |
| DOT | `.dot`, `.gv` |

The sample graphs keep the terms of their original sources.

## Documentation

| Document | Content |
|---|---|
| [architecture.md](docs/architecture.md) | Layers, modules, threads and flows |
| [abstractions.md](docs/abstractions.md) | Core traits and how to add algorithms and formats |
| [ingestion.md](docs/ingestion.md) | File reading and CSR construction |
| [algorithms.md](docs/algorithms.md) | The three layout algorithms and their parameters |
| [presentation.md](docs/presentation.md) | Rendering, camera, interface and metrics |
| [development.md](docs/development.md) | Build profile, tests, dependencies and assets |

## About

GraphBeX was developed by Bernardo Santos Nilson as an undergraduate thesis at PUCRS.

## License

GraphBeX is released under the [MIT License](LICENSE). The embedded fonts keep their own licenses (`src/modules/font/`), and the sample graphs keep the terms of their sources (`data/graph_sources.md`).
