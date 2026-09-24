//! Menu-bar (macOS) / system-tray (Windows) icon and its menu.
use super::icons;
use crate::model::Shared;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
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
    let s = shared.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
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
        } else if event.id == exit_id {
            quit.store(true, Ordering::Relaxed);
            s.show();
        }
    }));
    let s = shared.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        // Windows: a left click opens the window, the right click the menu.
        // macOS shows the menu on any click and sends no click events.
        let click = matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } | TrayIconEvent::DoubleClick { .. }
        );
        if click {
            s.show();
        }
    }));
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(cfg!(target_os = "macos"))
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
