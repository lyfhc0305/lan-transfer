//! Menu-bar (macOS) / system-tray (Windows) icon and its menu.
use super::icons;
use crate::model::Shared;
#[cfg(target_os = "macos")]
use eframe::egui;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

pub fn make_tray(
    shared: Arc<Shared>,
    quit: Arc<AtomicBool>,
) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    // Linux is only used for development; tray-icon needs a GTK main loop there.
    if cfg!(target_os = "linux") {
        return Err("Linux 开发版不提供托盘".into());
    }
    let menu = Menu::new();
    let show = MenuItem::new("打开邻传", true, None);
    let folder = MenuItem::new("打开接收文件夹", true, None);
    let exit = MenuItem::new("退出邻传", true, None);
    menu.append_items(&[&show, &folder, &PredefinedMenuItem::separator(), &exit])?;
    let show_id = show.id().clone();
    let folder_id = folder.id().clone();
    let exit_id = exit.id().clone();
    #[cfg(target_os = "macos")]
    let app_menu = app_menu()?;
    let s = shared.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        #[cfg(target_os = "macos")]
        if let Some(&key) = app_menu.edit.get(&event.id) {
            if let Some(e) = super::widgets::edit_event(key) {
                super::widgets::queue_edit_command(&s.ctx, None, e);
            }
            return;
        }
        #[cfg(target_os = "macos")]
        let quitting = event.id == exit_id || event.id == app_menu.quit;
        #[cfg(not(target_os = "macos"))]
        let quitting = event.id == exit_id;
        if event.id == show_id {
            s.show();
        } else if event.id == folder_id {
            let path = s.settings.lock().unwrap().folder.clone();
            if let Err(e) = super::open_folder(&path) {
                s.event(crate::model::Event::Note(format!(
                    "无法打开接收文件夹：{e}"
                )));
                s.show();
            }
        } else if quitting {
            quit.store(true, Ordering::Relaxed);
            s.show();
        }
    }));
    let s = shared.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(event, TrayIconEvent::DoubleClick { .. }) {
            s.show();
        }
    }));
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("邻传 · 局域网文件互传");
    // macOS menu-bar icons are monochrome "template" images that follow the
    // light / dark menu bar; Windows tray icons are shown in colour.
    let builder = if cfg!(target_os = "macos") {
        builder
            .with_icon(tray_icon::Icon::from_rgba(icons::tray_glyph(36), 36, 36)?)
            .with_icon_as_template(true)
    } else {
        builder.with_icon(tray_icon::Icon::from_rgba(icons::icon(32), 32, 32)?)
    };
    Ok(builder.build()?)
}

#[cfg(target_os = "macos")]
struct AppMenu {
    edit: std::collections::HashMap<tray_icon::menu::MenuId, egui::Key>,
    quit: tray_icon::menu::MenuId,
}

/// The macOS menu bar: the app menu and an Edit menu.
///
/// Without a main menu, ⌘C / ⌘V / ⌘X / ⌘A never reach text fields on macOS
/// (the menu bar is also where macOS looks for these key equivalents when
/// the app runs as a menu-bar-only app). Each Edit item sends egui the same
/// command the keyboard shortcut would, to whatever field has focus.
#[cfg(target_os = "macos")]
fn app_menu() -> Result<AppMenu, Box<dyn std::error::Error>> {
    use tray_icon::menu::{accelerator::Accelerator, Submenu};
    let item = |label: &str, shortcut: &str| -> Result<MenuItem, Box<dyn std::error::Error>> {
        Ok(MenuItem::new(
            label,
            true,
            Some(shortcut.parse::<Accelerator>()?),
        ))
    };
    let quit = item("退出邻传", "Cmd+Q")?;
    let app = Submenu::with_items(
        "邻传",
        true,
        &[
            &PredefinedMenuItem::hide(Some("隐藏邻传")),
            &PredefinedMenuItem::hide_others(Some("隐藏其他")),
            &PredefinedMenuItem::show_all(Some("全部显示")),
            &PredefinedMenuItem::separator(),
            &quit,
        ],
    )?;
    let cut = item("剪切", "Cmd+X")?;
    let copy = item("复制", "Cmd+C")?;
    let paste = item("粘贴", "Cmd+V")?;
    let select_all = item("全选", "Cmd+A")?;
    let edit = Submenu::with_items(
        "编辑",
        true,
        &[
            &cut,
            &copy,
            &paste,
            &PredefinedMenuItem::separator(),
            &select_all,
        ],
    )?;
    let bar = Menu::with_items(&[&app, &edit])?;
    bar.init_for_nsapp();
    // The menu bar lives as long as the app.
    std::mem::forget(bar);
    Ok(AppMenu {
        edit: [
            (cut.id().clone(), egui::Key::X),
            (copy.id().clone(), egui::Key::C),
            (paste.id().clone(), egui::Key::V),
            (select_all.id().clone(), egui::Key::A),
        ]
        .into(),
        quit: quit.id().clone(),
    })
}
