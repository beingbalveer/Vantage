//! Vantage — native GPUI desktop dashboard.
//!
//! Repositories are read from the local engine scan and persisted group
//! assignments, so the UI always reflects real work trees.

mod app;
mod app_icon;
mod avatar_cache;
mod backend;
mod chrome;
mod icons;
mod layout;
mod menu;
mod model;
mod path;
mod root;
mod settings;
mod text;
mod theme;
mod ui;
mod views;
mod watcher;

use std::borrow::Cow;

use gpui::{
    point, px, size, App, AppContext, Application, Bounds, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};

use crate::app::VantageApp;
use crate::icons::Assets;
use crate::settings::UiSettings;

macro_rules! fonts {
    ($($file:literal),* $(,)?) => {
        vec![$(Cow::Borrowed(include_bytes!(concat!("../assets/fonts/", $file)).as_slice())),*]
    };
}

fn register_fonts(cx: &App) {
    cx.text_system()
        .add_fonts(fonts![
            "PublicSans-400.ttf",
            "PublicSans-500.ttf",
            "PublicSans-600.ttf",
            "PlusJakartaSans-600.ttf",
            "PlusJakartaSans-700.ttf",
            "JetBrainsMono-400.ttf",
            "JetBrainsMono-500.ttf",
            "JetBrainsMono-600.ttf",
        ])
        .expect("register bundled fonts");
}

/// Dev/QA helper: `VANTAGE_STATE=dark,sidebar,section:changes,inspector:first,modal,newgroup`
/// drives the UI into a given state so screenshots can be taken without scripting.
fn apply_env_state(entity: &gpui::Entity<VantageApp>, cx: &mut App) {
    let Ok(state) = std::env::var("VANTAGE_STATE") else {
        return;
    };
    let tokens: Vec<String> = state.split(',').map(|t| t.trim().to_string()).collect();
    entity.update(cx, |app, cx| {
        for token in &tokens {
            match token.as_str() {
                "dark" => app.theme = theme::Theme::dark(),
                "light" => app.theme = theme::Theme::light(),
                "modal" => app.open_add_modal(cx),
                "persist" => {
                    app.persist(cx);
                }
                "fetch" => {
                    if let Some(repo) = app.repos.first().cloned() {
                        app.fetch_repo(&repo, cx);
                    }
                }
                "newgroup" => app.is_adding_group = true,
                "sidebar" => app.sidebar_collapsed = true,
                other => {
                    if let Some(path) = other.strip_prefix("modalpath:") {
                        app.open_add_modal(cx);
                        app.modal_path.set(path.to_string());
                    } else if let Some(section) = other.strip_prefix("section:") {
                        app.active_section = match section {
                            "changes" => model::NavSection::Changes,
                            "attention" => model::NavSection::Attention,
                            _ => model::NavSection::Overview,
                        };
                    } else if other == "inspector:first" {
                        if let Some(repo) = app.repos.first() {
                            let id = repo.id.clone();
                            app.select_repo(id, cx);
                        }
                    } else if let Some(repo) = other.strip_prefix("inspector:") {
                        app.select_repo(repo.to_string(), cx);
                        if let Some(first) = app
                            .selected_repo()
                            .and_then(|r| r.uncommitted_files.first())
                            .filter(|f| !f.diff_preview.is_empty())
                            .map(|f| f.path.clone())
                        {
                            app.expanded_file = Some(first);
                        }
                    } else if let Some(tab) = other.strip_prefix("itab:") {
                        app.inspector_tab = match tab {
                            "branches" => app::InspectorTab::Branches,
                            "commits" => app::InspectorTab::Commits,
                            _ => app::InspectorTab::Changes,
                        };
                    } else if let Some(group) = other.strip_prefix("group:") {
                        app.active_group = group.to_string();
                    }
                }
            }
        }
        cx.notify();
    });

    // QA: `inspector:first` is evaluated before the first scan lands, so
    // re-select once the scan has replaced the repository list.
    if tokens.iter().any(|token| token == "inspector:first") {
        let entity = entity.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1600))
                .await;
            entity
                .update(cx, |app, cx| {
                    if let Some(repo) = app.repos.first() {
                        let id = repo.id.clone();
                        let preview = repo
                            .uncommitted_files
                            .iter()
                            .find(|file| !file.diff_preview.is_empty())
                            .map(|file| file.path.clone());
                        app.select_repo(id, cx);
                        app.expanded_file = preview;
                    }
                })
                .ok();
        })
        .detach();
    }
}

fn initial_window_bounds(settings: &UiSettings, cx: &App) -> Bounds<gpui::Pixels> {
    if settings.remember_window {
        if let Some(ref saved) = settings.window_bounds {
            let bounds = Bounds {
                origin: point(px(saved.x as f32), px(saved.y as f32)),
                size: size(px(saved.width as f32), px(saved.height as f32)),
            };
            if visible_on_any_display(bounds, cx) {
                return bounds;
            }
        }
    }
    Bounds::centered(None, size(px(1440.0), px(900.0)), cx)
}

fn visible_on_any_display(bounds: Bounds<gpui::Pixels>, cx: &App) -> bool {
    cx.displays()
        .iter()
        .any(|display| display.bounds().intersects(&bounds))
}

fn main() {
    if std::env::args().any(|arg| arg == "--print-fonts") {
        Application::headless().run(|cx: &mut App| {
            register_fonts(cx);
            let mut names: Vec<String> = cx
                .text_system()
                .all_font_names()
                .into_iter()
                .filter(|name| {
                    name.contains("Public Sans")
                        || name.contains("Jakarta")
                        || name.contains("JetBrains")
                })
                .collect();
            names.sort();
            for name in names {
                println!("font: {name}");
            }
            cx.quit();
        });
        return;
    }

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        // Unbundled binaries have no Info.plist icon; give the process the
        // brand icon before the first window appears.
        app_icon::install();
        register_fonts(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let (persisted, startup_warning) = vantage_core::repos::load_state();
        let settings = UiSettings::from_state(&persisted);
        let bounds = initial_window_bounds(&settings, cx);
        let root: std::cell::RefCell<Option<gpui::Entity<VantageApp>>> =
            std::cell::RefCell::new(None);
        let _window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Vantage".into()),
                        // macOS: let the app header draw under the titlebar and
                        // place the three system buttons inside the 42px bar.
                        #[cfg(target_os = "macos")]
                        appears_transparent: true,
                        #[cfg(not(target_os = "macos"))]
                        appears_transparent: false,
                        #[cfg(target_os = "macos")]
                        traffic_light_position: Some(gpui::point(px(20.0), px(14.0))),
                        #[cfg(not(target_os = "macos"))]
                        traffic_light_position: None,
                    }),
                    window_min_size: Some(size(
                        px(layout::MIN_WINDOW_WIDTH),
                        px(layout::MIN_WINDOW_HEIGHT),
                    )),
                    window_background: WindowBackgroundAppearance::Opaque,
                    app_id: Some("Vantage".to_string()),
                    ..Default::default()
                },
                |_window, cx| {
                    let entity = match startup_warning {
                        Some(warning) => cx
                            .new(move |cx| VantageApp::new_with_startup_warning(cx, Some(warning))),
                        None => cx.new(VantageApp::new),
                    };
                    *root.borrow_mut() = Some(entity.clone());
                    cx.new(|_| root::RootView::new(entity))
                },
            )
            .expect("open Vantage window");

        if let Some(entity) = root.into_inner() {
            menu::install(cx, entity.clone());

            let appearance_entity = entity.clone();
            _window
                .update(cx, |_, window, cx| {
                    appearance_entity.update(cx, |app, cx| {
                        app.apply_theme(window.appearance());
                        cx.observe_window_appearance(window, |app, window, cx| {
                            app.on_appearance_changed(window, cx)
                        })
                        .detach();
                        cx.observe_window_activation(window, |app, window, cx| {
                            if window.is_window_active() {
                                app.refresh_on_focus(cx);
                            }
                        })
                        .detach();
                    });

                    let bounds_entity = appearance_entity.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        let bounds = window.bounds();
                        let fullscreen = window.is_fullscreen();
                        bounds_entity.update(cx, |app, cx| {
                            app.save_window_bounds(bounds, fullscreen, cx);
                            app.flush_state();
                        });
                        true
                    });
                })
                .ok();

            apply_env_state(&entity, cx);
            entity.update(cx, |app, cx| {
                app.start_scan(cx);
                app.start_auto_fetch(cx);
                app.watch_state_notices(cx);
            });
        }
        cx.activate(true);
    });
}
