Name:           kitenest
Version:        0.1.0
Release:        1%{?dist}
Summary:        Friendly package manager for Fedora-family Linux
License:        GPL-3.0-or-later
URL:            https://github.com/FAKERH0G/kitenest
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  pkgconfig

%description
KiteNest manages RPM packages, Flatpaks, and AppImages through a shared
backend with GTK4/libadwaita and command-line frontends.

%prep
%autosetup -n %{name}-%{version} -a 1

%build
cd %{name}-%{version}
export CARGO_TARGET_DIR="$PWD/target"
cargo build --offline --locked --release --all-features

%install
cd %{name}-%{version}
export CARGO_TARGET_DIR="$PWD/target"
install -Dpm0755 target/release/kitenest %{buildroot}%{_bindir}/kitenest
install -Dpm0755 target/release/kitenest-gui %{buildroot}%{_bindir}/kitenest-gui
install -Dpm0755 target/release/kitenest-privileged %{buildroot}%{_bindir}/kitenest-privileged
install -Dpm0644 data/org.kitenest.PackageManager.desktop \
    %{buildroot}%{_datadir}/applications/org.kitenest.PackageManager.desktop
install -Dpm0644 data/icons/hicolor/scalable/apps/org.kitenest.PackageManager.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/org.kitenest.PackageManager.svg

%files
%license LICENSE
%doc README.md
%{_bindir}/kitenest
%{_bindir}/kitenest-gui
%{_bindir}/kitenest-privileged
%{_datadir}/applications/org.kitenest.PackageManager.desktop
%{_datadir}/icons/hicolor/scalable/apps/org.kitenest.PackageManager.svg

%changelog
* Tue Oct 20 2026 KiteNest Contributors <kitenest-maintainers@example.invalid> - 0.1.0-1
- Initial Fedora-family package
