//! Vantage — native GPUI port of the React Vantage dashboard.
//!
//! Repositories are read from the local engine scan and persisted group
//! assignments, so the UI always reflects real work trees.

mod add_repo_modal;
mod app;
mod backend;
mod branch_matrix;
mod icons;
mod inspector;
mod menu;
mod model;
mod pending_changes;
mod repo_table;
mod root;
mod sidebar;
mod store;
mod text;
mod theme;
mod topbar;
mod viewport;
mod watcher;
mod widgets;

use std::borrow::Cow;

use gpui::{
    px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowOptions,
};

use crate::app::VantageApp;
use crate::icons::Assets;

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

/// Dev/QA helper: `VANTAGE_STATE=dark,sidebar,section:branches,inspector:first,modal,newgroup`
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
                "persist" => app.persist(cx),
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
                            "branches" => model::NavSection::Branches,
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
        register_fonts(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        let root: std::cell::RefCell<Option<gpui::Entity<VantageApp>>> =
            std::cell::RefCell::new(None);
        let _window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Vantage".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    window_min_size: Some(size(px(1040.0), px(640.0))),
                    window_background: WindowBackgroundAppearance::Opaque,
                    app_id: Some("Vantage".to_string()),
                    ..Default::default()
                },
                |_window, cx| {
                    let entity = cx.new(VantageApp::new);
                    *root.borrow_mut() = Some(entity.clone());
                    cx.new(|_| root::RootView::new(entity))
                },
            )
            .expect("open Vantage window");

        if let Some(entity) = root.into_inner() {
            menu::install(cx, entity.clone());
            apply_env_state(&entity, cx);
            entity.update(cx, |app, cx| app.start_scan(cx));
        }
        cx.activate(true);
    });
}
