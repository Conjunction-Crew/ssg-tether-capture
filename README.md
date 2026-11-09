# SSG Tether Capture

A simulation of orbital debris capture with a space tether: a desktop app (Windows, macOS, Linux) to plan, watch, record and replay a capture, and a headless command-line runner (`ssg-sim`) for repeatable runs. Built with Bevy and Avian3D.
Developed by [Conjunction Crew](https://github.com/conjunction-crew), in collaboration with the UF MAE Space Systems Group.

📖 **[Full documentation →](https://conjunction-crew.github.io/ssg-tether-capture/)**

## Quick start

```bash
git clone https://github.com/conjunction-crew/ssg-tether-capture.git
cd ssg-tether-capture
cargo run
```

## Headless runner

```bash
cargo run --release -p ssg-sim-cli -- run <project-dir> [options]
```

See [Running without a window](https://conjunction-crew.github.io/ssg-tether-capture/docs/getting-started/headless-cli). Installers for the app are on the [Releases](https://github.com/Conjunction-Crew/ssg-tether-capture/releases) page.

## Run with Tracy performance monitor tool enabled
```bash
cargo run --release --features bevy/trace_tracy
```

See [Prerequisites](https://conjunction-crew.github.io/ssg-tether-capture/docs/getting-started/prerequisites) for system dependencies and required assets before building.

## Contributing

See the [Development Setup](https://conjunction-crew.github.io/ssg-tether-capture/docs/contributing/development) and [Testing](https://conjunction-crew.github.io/ssg-tether-capture/docs/contributing/testing) guides.

## Authors

- Blake Owen
- Brandon Rosenthal