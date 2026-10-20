# KiteNest

KiteNest is a GTK4/libadwaita desktop app and CLI for Fedora-family Linux. It
brings configured RPM repositories, Flatpak remotes, and locally managed
AppImages together without using pacman, ALPM, or the AUR.

The GUI and CLI call the same Rust library and source backends. GTK4 with
libadwaita provides a native, Wayland-friendly interface; package operations
stay in a shared backend so behavior and distro policy are consistent across
frontends. RPM transactions use the host's `dnf5` CLI, falling back to `dnf`,
and ask polkit through `pkexec` only for the privileged helper. The GUI itself
never runs as root.

## Screenshots

Screenshots will be added after the first release.

## Features

- Search and list RPMs, Flatpaks, and installed AppImages.
- Install/remove RPMs and Flatpaks; install AppImages from a local file or
  HTTPS URL, add a desktop menu entry, and register a generic icon.
- Check and apply updates per source or together with `kitenest updates all`
  and `kitenest upgrade all`.
- Skip Flatpak or AppImage checks in combined updates with persistent config.
- Detect the distro using `/etc/os-release` and `/run/ostree-booted`.
- Use configured Fedora, RPM Fusion, and Nobara repositories without changing
  repo configuration.
- Fall back from `dnf5` to `dnf` where necessary.

For AppImages with embedded zsync update metadata, KiteNest uses
`appimageupdatetool` (from AppImageUpdate) to check or apply updates. Images
without embedded update metadata are still tracked and launchable but cannot
be updated automatically.

## Distro behavior

**Fedora Workstation/KDE:** Uses normal `dnf5` transactions when available,
then `dnf`. Search and update checks read the host's configured repositories.

**Nobara:** Keeps Nobara's configured repositories and uses them for package
search and individual package install/removal. KiteNest deliberately skips
system-wide RPM upgrades in `upgrade all`; run those through Nobara's own
system updater. This avoids bypassing Nobara's update workflow.

**Bazzite and other rpm-ostree systems:** Flatpak and AppImage are the primary
sources. System RPM installs/removals are staged through `rpm-ostree` when it
is available and display a reboot-required notice. KiteNest does not run
system-wide rpm-ostree upgrades. Explicit DNF RPM update checks are disabled
for immutable systems.

## Install

### Fedora Workstation/KDE and Nobara

Install a release RPM with DNF:

```sh
sudo dnf install ./kitenest-0.1.1-1.fc44.x86_64.rpm
```

Launch **KiteNest** from the application grid, or use `kitenest` in a
terminal. RPM installs/removals open a polkit authentication prompt. On
Nobara, use Nobara's updater for system-wide upgrades.

### Bazzite

Prefer Flatpak and AppImage packages inside KiteNest. To install KiteNest
itself from a downloaded RPM, stage it with rpm-ostree and reboot:

```sh
sudo rpm-ostree install-local ./kitenest-0.1.1-1.fc44.x86_64.rpm
systemctl reboot
```

RPM package operations performed by KiteNest are separately staged and also
require a reboot.

### Flatpak and AppImage package formats

KiteNest is a host package manager, so a sandboxed Flatpak build cannot
reliably or safely invoke the host's DNF/libdnf5 and polkit helper. The
project therefore does not publish a Flatpak package. An AppImage build
script is provided for environments with `linuxdeploy`, its GTK plugin, and
`appimagetool`; it bundles the GTK front end without sandboxing away access to
the host package tools. RPM operations still elevate only the narrow helper
through polkit:

```sh
./packaging/build-appimage.sh
```

The AppImage is emitted as `KiteNest-<architecture>.AppImage`. The AppImage
build has not yet been validated in CI; use the RPM for a supported host
installation.
The bundled CLI is available through the AppImage entry point, for example
`./KiteNest-x86_64.AppImage --help`.

## CLI

```sh
kitenest search firefox --source all
kitenest list --source all
kitenest install org.mozilla.firefox --source flatpak
kitenest install ./SomeApp.AppImage --source appimage
kitenest install https://downloads.example.org/SomeApp.AppImage --source appimage
kitenest remove org.mozilla.firefox --source flatpak
kitenest updates all
kitenest upgrade all
kitenest config skip-updates flatpak true
kitenest config skip-updates appimage true
kitenest distro
```

Install, removal, and upgrade actions ask for confirmation. `--yes` skips the
CLI confirmation prompt; RPM changes still go through pkexec/polkit. Flatpak
commands use configured user and system remotes. `upgrade all` skips update
checks disabled in config, while explicitly selecting `updates flatpak` or
`updates appimage` checks that source.

The config file is
`~/.config/kitenest/config.json`; the AppImage registry is
`~/.local/share/kitenest/appimages.json`. Installed AppImages live in
`~/Applications/KiteNest/`, with menu entries and icons under
`~/.local/share/`.

## Build and test

Requirements: Rust stable, GTK4 development headers, libadwaita development
headers, and `pkg-config`. RPM, Flatpak, and AppImage system tools are only
needed to exercise their respective runtime backends.

```sh
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release --all-features
cargo run --bin kitenest -- --help
cargo run --features gui --bin kitenest-gui
```

The RPM spec builds offline. Prepare an RPM source tree with Cargo's locked
dependencies vendored, and provide the matching vendor archive as
`kitenest-0.1.1-vendor.tar.gz` alongside the source tarball:

```sh
./scripts/prepare-rpm-sources.sh
rpmbuild -ba --define "_sourcedir $PWD/rpmbuild/SOURCES" packaging/kitenest.spec
```

## Credits

KiteNest is independently implemented for Fedora-family Linux and does not
reuse Shelly's Arch-specific code. Its shared-backend plus separate CLI/GUI
structure was inspired by [Shelly](https://github.com/Seafoam-Labs/Shelly-ALPM),
the GPL-3.0 GTK4 GUI and CLI package manager for Arch Linux. Thanks to the
Shelly contributors for publishing their work.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
