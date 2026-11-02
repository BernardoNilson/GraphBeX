# Development

## Commands

Run them from the repository root. The file dialog opens in `data/graph`, a relative path.

```bash
cargo run                # builds and opens the app
cargo test               # runs the tests
cargo run --release      # optimized build, for large graphs and measurements
```

The package and the executable are called `graphbex`.

The log goes to the terminal and shows the main events: graph loaded, start and end of each layout, and the requirements check. Vulkan setup messages (`wgpu_hal`) are hidden. Only its errors appear.

## Build profile and memory

Compiling Bevy is heavy, and on a machine with 8 GB of RAM the default parallelism can run out of memory. So:

- `.cargo/config.toml` limits the build to 2 jobs (`jobs = 2`). If memory still runs out, use `cargo build -j 1` and close other heavy programs.
- `Cargo.toml` reduces debug information (`debug = "line-tables-only"`).
- In debug builds, dependencies are compiled with optimizations (`opt-level = 3`), because Bevy without optimizations is too slow for large graphs. They are cached, so only the first build after a version change is long.

**Never interrupt a build halfway.** It leaves partial files behind that cause misleading errors such as "can't find crate for bevy". When that happens, run the build again before looking at the code.

## Tests

The 82 tests live next to the code of each module: 50 in file reading and export, 16 in the CSR and the algorithms, 10 in the engine, parameters and monitoring, and 6 in the presentation. The algorithm tests check the result, not just the absence of errors. For example, Barnes-Hut with θ = 0 must give the exact force.

## Dependencies

| Crate | Use |
|---|---|
| `bevy` 0.18 | Engine and interface |
| `bevy_polyline` | Edges |
| `bytemuck` | Vertex instance buffer |
| `wgpu` | VRAM counters |
| `petgraph` | CSR |
| `memmap2` | Memory-mapped reading |
| `quick-xml` | GraphML |
| `sprs` | Sparse matrix of the spectral layout |
| `rayon` | Force-Directed parallelism |
| `rand` | Random layout |
| `crossbeam-channel` | Channels between threads |
| `sysinfo` | CPU and RAM |
| `rfd` | Native dialogs |
| `thiserror` | Reading errors |
| `image`, `winit`, `winresource` | Window and executable icon |
| `log`, `env_logger` | Logging |

Bevy is on 0.18, not the latest version, because it is the latest one supported by `bevy_polyline`.

## Assets

- `src/modules/font/`: the Inter 4.1 font, Medium and SemiBold (SIL OFL 1.1, `Inter-LICENSE.txt`), and the Lucide 1.61.0 icon font (ISC, `Lucide-LICENSE.txt`).
- `src/modules/icon/`: the logo as PNG and ICO. The transparent PNG is the window icon and the menu bar symbol, and the ICO is embedded in the executable by `build.rs`.
- `presentation/instancing.rs` and `presentation/vertices.wgsl` are adapted from Bevy's `custom_shader_instancing` example.

## Sample graphs

The sample graphs are compressed in `data/graph/`, and `data/graph_sources.md` lists the source of each one. Extract them in the same folder before opening. Only the archives are versioned.
