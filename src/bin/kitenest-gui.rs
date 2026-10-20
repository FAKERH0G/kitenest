use std::{cell::RefCell, process::Command, sync::mpsc, thread, time::Duration};

use adw::prelude::*;
use gtk4 as gtk;
use kitenest::{
    Manager,
    config::Config,
    error::Result,
    model::{Package, Source},
};
use libadwaita as adw;

fn main() {
    if std::env::args_os().len() > 1 {
        match run_cli_args() {
            Ok(true) => std::process::exit(0),
            Ok(false) => std::process::exit(1),
            Err(error) => {
                eprintln!("kitenest: {error}");
                std::process::exit(1);
            }
        }
    }
    let application = adw::Application::builder()
        .application_id("org.kitenest.PackageManager")
        .build();
    application.connect_activate(build_window);
    application.run();
}

fn run_cli_args() -> std::io::Result<bool> {
    let executable = std::env::current_exe()?;
    let cli = executable.with_file_name("kitenest");
    let status = Command::new(cli)
        .args(std::env::args_os().skip(1))
        .status()?;
    Ok(status.success())
}

fn build_window(application: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("KiteNest")
        .default_width(820)
        .default_height(620)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let title = gtk::Label::new(Some("One home for your Linux apps"));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    content.append(&title);

    let distro = Manager::detect()
        .map(|manager| {
            if manager.distro.atomic {
                format!(
                    "{} · immutable; RPM changes need a reboot; prefer Flatpak/AppImage",
                    manager.distro.name
                )
            } else {
                format!("{} · RPM packages enabled", manager.distro.name)
            }
        })
        .unwrap_or_else(|error| format!("Could not detect distribution: {error}"));
    let distro_label = gtk::Label::new(Some(&distro));
    distro_label.set_xalign(0.0);
    distro_label.set_wrap(true);
    content.append(&distro_label);

    let source = gtk::DropDown::from_strings(&["RPM", "Flatpak", "AppImage", "All sources"]);
    content.append(&source);
    let query = gtk::Entry::builder()
        .placeholder_text("Search packages, or enter an AppImage file path / HTTPS URL")
        .hexpand(true)
        .build();
    content.append(&query);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let search_button = gtk::Button::with_label("Search");
    let list_button = gtk::Button::with_label("List installed");
    let install_button = gtk::Button::with_label("Install");
    let remove_button = gtk::Button::with_label("Remove");
    let updates_button = gtk::Button::with_label("Check updates");
    let upgrade_button = gtk::Button::with_label("Apply updates");
    for button in [
        &search_button,
        &list_button,
        &install_button,
        &remove_button,
        &updates_button,
        &upgrade_button,
    ] {
        buttons.append(button);
    }
    content.append(&buttons);

    let config_result = Config::load();
    let config = config_result.as_ref().cloned().unwrap_or_default();
    let preferences = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let skip_flatpak = gtk::CheckButton::with_label("Skip Flatpak checks in Update All");
    skip_flatpak.set_active(config.skip_flatpak_updates);
    let skip_appimage = gtk::CheckButton::with_label("Skip AppImage checks in Update All");
    skip_appimage.set_active(config.skip_appimage_updates);
    preferences.append(&skip_flatpak);
    preferences.append(&skip_appimage);
    content.append(&preferences);

    let output = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .vexpand(true)
        .build();
    output.buffer().set_text(
        &config_result
            .as_ref()
            .err()
            .map(|error| format!("Could not load settings: {error}"))
            .unwrap_or_else(|| "Search or manage installed applications.".into()),
    );
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&output)
        .build();
    content.append(&scroller);

    let window_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window_content.append(&content);
    window.set_content(Some(&window_content));

    let output_for_search = output.clone();
    let query_for_search = query.clone();
    let source_for_search = source.clone();
    search_button.connect_clicked(move |_| {
        let query = query_for_search.text().to_string();
        let source = selected_source(&source_for_search);
        run_job(&output_for_search, move || {
            let manager = Manager::detect()?;
            let packages = manager.search(source, &query)?;
            Ok(format_packages(packages))
        });
    });

    let output_for_list = output.clone();
    let source_for_list = source.clone();
    list_button.connect_clicked(move |_| {
        let source = selected_source(&source_for_list);
        run_job(&output_for_list, move || {
            let manager = Manager::detect()?;
            let packages = manager.list_installed(source)?;
            Ok(format_packages(packages))
        });
    });

    let output_for_install = output.clone();
    let query_for_install = query.clone();
    let source_for_install = source.clone();
    let window_for_install = window.clone();
    install_button.connect_clicked(move |_| {
        let package = query_for_install.text().to_string();
        let source = selected_source(&source_for_install);
        confirm_action(
            &window_for_install,
            &output_for_install,
            &format!("Install {package} from {source}?"),
            move || {
                let manager = Manager::detect()?;
                manager.install(source, &package, true)
            },
        );
    });

    let output_for_remove = output.clone();
    let query_for_remove = query.clone();
    let source_for_remove = source.clone();
    let window_for_remove = window.clone();
    remove_button.connect_clicked(move |_| {
        let package = query_for_remove.text().to_string();
        let source = selected_source(&source_for_remove);
        confirm_action(
            &window_for_remove,
            &output_for_remove,
            &format!("Remove {package} from {source}?"),
            move || {
                let manager = Manager::detect()?;
                manager.remove(source, &package, true)
            },
        );
    });

    let output_for_updates = output.clone();
    let source_for_updates = source.clone();
    updates_button.connect_clicked(move |_| {
        let source = selected_source(&source_for_updates);
        run_job(&output_for_updates, move || {
            let manager = Manager::detect()?;
            let updates = manager.updates(source)?;
            if updates.is_empty() {
                Ok("No updates available.".into())
            } else {
                Ok(updates.join("\n"))
            }
        });
    });

    let output_for_upgrade = output.clone();
    let source_for_upgrade = source.clone();
    let window_for_upgrade = window.clone();
    upgrade_button.connect_clicked(move |_| {
        let source = selected_source(&source_for_upgrade);
        confirm_action(
            &window_for_upgrade,
            &output_for_upgrade,
            &format!("Apply available {source} updates?"),
            move || {
                let manager = Manager::detect()?;
                let results = manager.upgrade(source, true)?;
                Ok(results.join("\n"))
            },
        );
    });

    let output_for_flatpak_toggle = output.clone();
    skip_flatpak.connect_toggled(move |button| {
        let result = Manager::detect()
            .and_then(|mut manager| manager.set_skip_updates(Source::Flatpak, button.is_active()));
        report_toggle(&output_for_flatpak_toggle, result);
    });
    let output_for_appimage_toggle = output.clone();
    skip_appimage.connect_toggled(move |button| {
        let result = Manager::detect()
            .and_then(|mut manager| manager.set_skip_updates(Source::AppImage, button.is_active()));
        report_toggle(&output_for_appimage_toggle, result);
    });

    window.present();
}

fn selected_source(dropdown: &gtk::DropDown) -> Source {
    match dropdown.selected() {
        0 => Source::Rpm,
        1 => Source::Flatpak,
        2 => Source::AppImage,
        _ => Source::All,
    }
}

fn format_packages(packages: Vec<Package>) -> String {
    if packages.is_empty() {
        return "No matching packages found.".into();
    }
    packages
        .into_iter()
        .map(|package| {
            format!(
                "{}  {}  {}  {}",
                package.source, package.id, package.version, package.name
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_job(output: &gtk::TextView, job: impl FnOnce() -> Result<String> + Send + 'static) {
    output.buffer().set_text("Working…");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = job().map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
    let buffer = output.buffer();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(Ok(message)) => {
                buffer.set_text(&message);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                buffer.set_text(&format!("Error: {error}"));
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                buffer.set_text("The background operation stopped unexpectedly.");
                glib::ControlFlow::Break
            }
        }
    });
}

fn confirm_action(
    parent: &adw::ApplicationWindow,
    output: &gtk::TextView,
    message: &str,
    action: impl FnOnce() -> Result<String> + Send + 'static,
) {
    let dialog = gtk::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk::MessageType::Warning)
        .buttons(gtk::ButtonsType::None)
        .text(message)
        .build();
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Continue", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Cancel);
    let output = output.clone();
    let action = RefCell::new(Some(action));
    dialog.connect_response(move |dialog, response| {
        dialog.close();
        if response == gtk::ResponseType::Accept
            && let Some(action) = action.borrow_mut().take()
        {
            run_job(&output, action);
        }
    });
    dialog.present();
}

fn report_toggle(output: &gtk::TextView, result: Result<()>) {
    output.buffer().set_text(&match result {
        Ok(()) => "Update preference saved.".into(),
        Err(error) => format!("Could not save preference: {error}"),
    });
}
