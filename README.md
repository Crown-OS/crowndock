# crowndock

The dock for [CrownOS](https://github.com/Crown-OS). A bottom-anchored,
auto-hiding [layer-shell](https://github.com/Crown-OS/crownshell) surface with
drag-and-drop application pinning.

**Status: Partial.** It builds and runs, animates nicely, and **cannot launch
applications**. See [Known limitations](#known-limitations).

## Surface

| | |
|---|---|
| Layer | `Overlay` |
| Anchor | BOTTOM |
| Size | 1044 × 100, fixed |
| Exclusive zone | 0 (it auto-hides) |
| Namespace | `Crowndock` |

## Prerequisites

Any Wayland compositor supporting `wlr-layer-shell` — `crownpositor`, Hyprland,
Sway, river, KWin.

Native dependencies (Arch):

```bash
sudo pacman -S --needed base-devel pkgconf \
  wayland wayland-protocols libxkbcommon \
  vulkan-icd-loader mesa libglvnd fontconfig dbus
```

Full list, including Debian/Ubuntu:
[Prerequisites](https://github.com/Crown-OS/crownos-documentations/blob/main/docs/10-getting-started/prerequisites.md).

## Build and run

> **You need the dev overlay first.** `crowndock` depends on
> `crownshell = "0.3"`, and 0.3 is **not published** — crates.io has only
> `crownshell` 0.1.0 and 0.2.0. A fresh clone fails at `cargo metadata` until
> Cargo is pointed at a local `crownshell` checkout.
>
> `crownos-setup`'s `./bootstrap.sh --dev` clones the repos side by side and
> writes a `[patch.crates-io]` overlay into a `.cargo/config.toml` one directory
> **above** them:
>
> ```
> ~/crownos/
> ├── .cargo/config.toml   # [patch.crates-io] crownshell = { path = "crownshell" }
> ├── crowndock/
> └── crownshell/
> ```
>
> Cargo walks up from the working directory to find that file, and the paths in
> it are relative to the file's own directory. No particular layout *inside* a
> repo is required.

```bash
cargo run
```

Drag a `.desktop` file onto the dock to pin it. Pinned items persist to
`~/.config/crowndock/items.toml`.

> `crowndock` does **not** call `env_logger::init()`, so its `log::warn!` output
> is invisible regardless of `RUST_LOG`. Add the init locally if you need to
> debug it.

## How it works

**Visibility** is a state machine — `Hidden → PendingShow → Showing → Shown →
PendingHide → Hiding` — driven by a spring (stiffness 240, damping 28, damping
ratio ≈ 0.904, integrated at a fixed 1/240 s substep).

Two details worth knowing if you touch the surface code:

- **The input region shrinks to a 1-pixel strip** at the bottom edge while
  hidden, so clicks fall through to whatever is underneath.
- **The blur region is striped into 32 bands** so the compositor blurs behind the
  icon row rather than the whole rectangle.

**Icons** come from the `.desktop` file's `Icon=` key, parsed with
`freedesktop_entry_parser` and resolved through `freedesktop-icons`. SVG is
rasterised with `resvg`/`tiny-skia`; raster formats through `image`.

**Drag and drop** accepts `text/uri-list`, filters to `.desktop` files, and
hand-rolls `file://` percent-decoding.

## Known limitations

- **Clicking an icon does not launch anything.** There is no `Exec=` parsing and
  no `std::process::Command` anywhere in the crate — `on_pointer_press` and
  `on_pointer_release` only drive drag state. This is the most valuable open task
  in the repo.
- **It deviates from the CrownOS config convention**, storing pinned items in
  `~/.config/crowndock/items.toml` — TOML, in its own directory — rather than a
  section in `~/.config/crownos/`.
- **The size is fixed** at 1044 × 100, not derived from screen width or icon
  count.
- **Blur is requested but does not happen** under `crownpositor`, which never
  advertises `ext-background-effect-v1`.
- **No tests.**
- **It does not build from a fresh clone on its own.** `crownshell = "0.3"` is an
  unpublished version; you need the `[patch.crates-io]` overlay described under
  [Build and run](#build-and-run).
- `tiny-skia` (0.11) and `dirs` (5) are a major version behind sibling crates;
  `tracing` is declared and unused.
- It carries its own spring implementation rather than using `crownshell`'s.

## Tests

There are none. `cargo test` compiles the crate and reports zero tests. The
`.desktop` parsing and the `file://` percent-decoder are pure functions and the
obvious place to start.

## Contributing

See the organization-wide
[contribution guide](https://github.com/Crown-OS/crownos-documentations/blob/main/CONTRIBUTING.md).
Default branch here is **`main`**.

## License

Licensed under the [MIT License](LICENSE).
