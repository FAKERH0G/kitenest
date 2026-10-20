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
    install_sharp_corner_style();
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("KiteNest")
        .default_width(1080)
        .default_height(720)
        .build();
    window.add_css_class("kitenest-window");
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

    let config_result = Config::load();
    let config = config_result.as_ref().cloned().unwrap_or_default();
    let skip_flatpak = gtk::CheckButton::with_label("Skip Flatpak checks in Update All");
    skip_flatpak.set_active(config.skip_flatpak_updates);
    let skip_appimage = gtk::CheckButton::with_label("Skip AppImage checks in Update All");
    skip_appimage.set_active(config.skip_appimage_updates);

    let header = adw::HeaderBar::new();
    let app_title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let app_icon = gtk::Image::from_icon_name("org.kitenest.PackageManager");
    app_icon.set_pixel_size(24);
    let app_name = gtk::Label::new(Some("KiteNest"));
    app_name.add_css_class("heading");
    app_title.append(&app_icon);
    app_title.append(&app_name);
    header.set_title_widget(Some(&app_title));
    window.set_titlebar(Some(&header));

    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.add_css_class("kitenest-shell");

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 16);
    sidebar.set_width_request(232);
    sidebar.set_margin_top(20);
    sidebar.set_margin_bottom(16);
    sidebar.set_margin_start(14);
    sidebar.set_margin_end(14);
    sidebar.add_css_class("kitenest-sidebar");

    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let brand_icon = gtk::Image::from_icon_name("org.kitenest.PackageManager");
    brand_icon.set_pixel_size(40);
    let brand_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let brand_name = gtk::Label::new(Some("KiteNest"));
    brand_name.add_css_class("title-3");
    brand_name.set_xalign(0.0);
    let brand_tagline = gtk::Label::new(Some("Software center"));
    brand_tagline.add_css_class("dim-label");
    brand_tagline.set_xalign(0.0);
    brand_text.append(&brand_name);
    brand_text.append(&brand_tagline);
    brand.append(&brand_icon);
    brand.append(&brand_text);
    sidebar.append(&brand);

    let distro_label = gtk::Label::new(Some(&distro));
    distro_label.set_xalign(0.0);
    distro_label.set_wrap(true);
    distro_label.add_css_class("dim-label");
    distro_label.add_css_class("distro-summary");
    sidebar.append(&distro_label);

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    let nav = gtk::Box::new(gtk::Orientation::Vertical, 4);
    nav.add_css_class("navigation-sidebar");
    let discover_nav = navigation_button("Discover", "system-search-symbolic");
    let installed_nav = navigation_button("Installed", "view-list-symbolic");
    let updates_nav = navigation_button("Updates", "software-update-available-symbolic");
    let settings_nav = navigation_button("Settings", "emblem-system-symbolic");
    installed_nav.set_group(Some(&discover_nav));
    updates_nav.set_group(Some(&discover_nav));
    settings_nav.set_group(Some(&discover_nav));
    discover_nav.set_active(true);
    for (button, page) in [
        (&discover_nav, "discover"),
        (&installed_nav, "installed"),
        (&updates_nav, "updates"),
        (&settings_nav, "settings"),
    ] {
        let stack = stack.clone();
        let page = page.to_owned();
        button.connect_toggled(move |button| {
            if button.is_active() {
                stack.set_visible_child_name(&page);
            }
        });
        nav.append(button);
    }
    sidebar.append(&nav);

    let sidebar_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_spacer.set_vexpand(true);
    sidebar.append(&sidebar_spacer);
    let source_summary = gtk::Label::new(Some("RPM  ·  Flatpak  ·  AppImage"));
    source_summary.set_xalign(0.0);
    source_summary.add_css_class("caption");
    source_summary.add_css_class("dim-label");
    sidebar.append(&source_summary);
    shell.append(&sidebar);

    let source = gtk::DropDown::from_strings(&["RPM", "Flatpak", "AppImage", "All sources"]);
    let query = gtk::Entry::builder()
        .placeholder_text("Search apps, or enter an AppImage path / HTTPS URL")
        .hexpand(true)
        .build();
    let search_button = gtk::Button::with_label("Search");
    search_button.add_css_class("suggested-action");
    let install_button = gtk::Button::with_label("Install");
    let install_entry = gtk::Entry::builder()
        .placeholder_text("Package ID, local AppImage, or HTTPS URL")
        .hexpand(true)
        .build();
    let discover_output = output_view(
        &config_result
            .as_ref()
            .err()
            .map(|error| format!("Could not load settings: {error}"))
            .unwrap_or_else(|| "Search configured software sources to get started.".into()),
    );
    let discover_page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    discover_page.set_margin_top(28);
    discover_page.set_margin_bottom(24);
    discover_page.set_margin_start(32);
    discover_page.set_margin_end(32);
    append_page_heading(
        &discover_page,
        "Discover",
        "Find software in configured RPM repositories and Flatpak remotes, or install an AppImage.",
    );
    let source_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let source_label = gtk::Label::new(Some("Source"));
    source_label.add_css_class("dim-label");
    source_row.append(&source_label);
    source_row.append(&source);
    discover_page.append(&source_row);
    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_row.append(&query);
    search_row.append(&search_button);
    discover_page.append(&search_row);
    append_section_label(&discover_page, "Install from file or URL");
    let install_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    install_row.append(&install_entry);
    install_row.append(&install_button);
    discover_page.append(&install_row);
    append_section_label(&discover_page, "Search results");
    discover_page.append(&output_scroller(&discover_output));
    stack.add_named(&discover_page, Some("discover"));

    let installed_source =
        gtk::DropDown::from_strings(&["All sources", "RPM", "Flatpak", "AppImage"]);
    let list_button = gtk::Button::with_label("Refresh");
    let installed_output = output_view("Your installed software will appear here.");
    let installed_page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    installed_page.set_margin_top(28);
    installed_page.set_margin_bottom(24);
    installed_page.set_margin_start(32);
    installed_page.set_margin_end(32);
    append_page_heading(
        &installed_page,
        "Installed",
        "View and manage software installed from your package sources.",
    );
    let installed_toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let installed_source_label = gtk::Label::new(Some("Show"));
    installed_source_label.add_css_class("dim-label");
    installed_toolbar.append(&installed_source_label);
    installed_toolbar.append(&installed_source);
    let remove_entry = gtk::Entry::builder()
        .placeholder_text("Package ID to remove")
        .hexpand(true)
        .build();
    let remove_button = gtk::Button::with_label("Remove");
    remove_button.add_css_class("destructive-action");
    installed_toolbar.append(&remove_entry);
    installed_toolbar.append(&remove_button);
    installed_toolbar.append(&list_button);
    installed_page.append(&installed_toolbar);
    installed_page.append(&output_scroller(&installed_output));
    stack.add_named(&installed_page, Some("installed"));

    let updates_source =
        gtk::DropDown::from_strings(&["All sources", "RPM", "Flatpak", "AppImage"]);
    let updates_button = gtk::Button::with_label("Check for updates");
    let upgrade_button = gtk::Button::with_label("Apply updates");
    upgrade_button.add_css_class("suggested-action");
    let updates_output = output_view("Check for available updates across your sources.");
    let updates_page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    updates_page.set_margin_top(28);
    updates_page.set_margin_bottom(24);
    updates_page.set_margin_start(32);
    updates_page.set_margin_end(32);
    append_page_heading(
        &updates_page,
        "Updates",
        "Review updates source by source, or apply updates for all supported sources.",
    );
    let updates_toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let updates_source_label = gtk::Label::new(Some("Source"));
    updates_source_label.add_css_class("dim-label");
    updates_toolbar.append(&updates_source_label);
    updates_toolbar.append(&updates_source);
    updates_toolbar.append(&updates_button);
    updates_toolbar.append(&upgrade_button);
    updates_page.append(&updates_toolbar);
    updates_page.append(&output_scroller(&updates_output));
    stack.add_named(&updates_page, Some("updates"));

    let settings_status = gtk::Label::new(Some(
        &config_result
            .as_ref()
            .err()
            .map(|error| format!("Could not load settings: {error}"))
            .unwrap_or_default(),
    ));
    settings_status.set_xalign(0.0);
    settings_status.set_wrap(true);
    settings_status.add_css_class("dim-label");
    let settings_page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    settings_page.set_margin_top(28);
    settings_page.set_margin_bottom(24);
    settings_page.set_margin_start(32);
    settings_page.set_margin_end(32);
    append_page_heading(
        &settings_page,
        "Settings",
        "Choose which sources are included when checking for updates.",
    );
    let preferences = gtk::Box::new(gtk::Orientation::Vertical, 12);
    preferences.add_css_class("settings-group");
    skip_flatpak.set_margin_top(8);
    skip_flatpak.set_margin_bottom(8);
    skip_flatpak.set_margin_start(12);
    skip_flatpak.set_margin_end(12);
    skip_appimage.set_margin_top(8);
    skip_appimage.set_margin_bottom(8);
    skip_appimage.set_margin_start(12);
    skip_appimage.set_margin_end(12);
    preferences.append(&skip_flatpak);
    preferences.append(&skip_appimage);
    settings_page.append(&preferences);
    append_section_label(&settings_page, "Detected system");
    let settings_distro = gtk::Label::new(Some(&distro));
    settings_distro.set_xalign(0.0);
    settings_distro.set_wrap(true);
    settings_page.append(&settings_distro);
    settings_page.append(&settings_status);
    stack.add_named(&settings_page, Some("settings"));
    shell.append(&stack);
    window.set_content(Some(&shell));

    let output_for_search = discover_output.clone();
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

    let output_for_list = installed_output.clone();
    let source_for_list = installed_source.clone();
    list_button.connect_clicked(move |_| {
        let source = selected_installed_source(&source_for_list);
        run_job(&output_for_list, move || {
            let manager = Manager::detect()?;
            let packages = manager.list_installed(source)?;
            Ok(format_packages(packages))
        });
    });

    let output_for_install = discover_output.clone();
    let query_for_install = install_entry.clone();
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

    let output_for_remove = installed_output.clone();
    let query_for_remove = remove_entry.clone();
    let source_for_remove = installed_source.clone();
    let window_for_remove = window.clone();
    remove_button.connect_clicked(move |_| {
        let package = query_for_remove.text().to_string();
        let source = selected_installed_source(&source_for_remove);
        if source == Source::All {
            output_for_remove
                .buffer()
                .set_text("Choose a specific source before removing a package.");
            return;
        }
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

    let output_for_updates = updates_output.clone();
    let source_for_updates = updates_source.clone();
    updates_button.connect_clicked(move |_| {
        let source = selected_updates_source(&source_for_updates);
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

    let output_for_upgrade = updates_output.clone();
    let source_for_upgrade = updates_source.clone();
    let window_for_upgrade = window.clone();
    upgrade_button.connect_clicked(move |_| {
        let source = selected_updates_source(&source_for_upgrade);
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

    let status_for_flatpak_toggle = settings_status.clone();
    skip_flatpak.connect_toggled(move |button| {
        let result = Manager::detect()
            .and_then(|mut manager| manager.set_skip_updates(Source::Flatpak, button.is_active()));
        report_toggle(&status_for_flatpak_toggle, result);
    });
    let status_for_appimage_toggle = settings_status.clone();
    skip_appimage.connect_toggled(move |button| {
        let result = Manager::detect()
            .and_then(|mut manager| manager.set_skip_updates(Source::AppImage, button.is_active()));
        report_toggle(&status_for_appimage_toggle, result);
    });

    window.present();
}

fn install_sharp_corner_style() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "window.kitenest-window, window.kitenest-window * { border-radius: 0; }\n\
         .kitenest-sidebar { background-color: alpha(currentColor, 0.035); border-right: 1px solid alpha(currentColor, 0.12); }\n\
         .sidebar-button { min-height: 38px; padding: 4px 10px; text-align: left; }\n\
         .sidebar-button:checked { background-color: alpha(@accent_bg_color, 0.18); color: @accent_color; }\n\
         .distro-summary { padding: 10px; background-color: alpha(currentColor, 0.045); }\n\
         .settings-group { padding: 8px 12px; background-color: alpha(currentColor, 0.045); }",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn navigation_button(label: &str, icon_name: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::new();
    button.add_css_class("flat");
    button.add_css_class("sidebar-button");
    button.set_hexpand(true);
    button.set_halign(gtk::Align::Fill);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let icon = gtk::Image::from_icon_name(icon_name);
    let title = gtk::Label::new(Some(label));
    title.set_xalign(0.0);
    row.append(&icon);
    row.append(&title);
    button.set_child(Some(&row));
    button
}

fn append_page_heading(page: &gtk::Box, title: &str, subtitle: &str) {
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-1");
    heading.set_xalign(0.0);
    let description = gtk::Label::new(Some(subtitle));
    description.add_css_class("dim-label");
    description.set_xalign(0.0);
    description.set_wrap(true);
    page.append(&heading);
    page.append(&description);
}

fn append_section_label(page: &gtk::Box, title: &str) {
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    page.append(&heading);
}

fn output_view(initial: &str) -> gtk::TextView {
    let output = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .vexpand(true)
        .build();
    output.add_css_class("result-view");
    output.buffer().set_text(initial);
    output
}

fn output_scroller(output: &gtk::TextView) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(output)
        .build();
    scroller.add_css_class("result-scroller");
    scroller
}

fn selected_source(dropdown: &gtk::DropDown) -> Source {
    match dropdown.selected() {
        0 => Source::Rpm,
        1 => Source::Flatpak,
        2 => Source::AppImage,
        _ => Source::All,
    }
}

fn selected_installed_source(dropdown: &gtk::DropDown) -> Source {
    match dropdown.selected() {
        1 => Source::Rpm,
        2 => Source::Flatpak,
        3 => Source::AppImage,
        _ => Source::All,
    }
}

fn selected_updates_source(dropdown: &gtk::DropDown) -> Source {
    match dropdown.selected() {
        1 => Source::Rpm,
        2 => Source::Flatpak,
        3 => Source::AppImage,
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
            let version = if package.version.is_empty() {
                String::new()
            } else {
                format!(" · {}", package.version)
            };
            format!(
                "{}\n{} · {}{}\n",
                package.name, package.source, package.id, version
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

fn report_toggle(status: &gtk::Label, result: Result<()>) {
    status.set_text(&match result {
        Ok(()) => "Update preference saved.".into(),
        Err(error) => format!("Could not save preference: {error}"),
    });
}
