#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod discovery;
mod model;
mod network;
mod notify;
mod ui;
mod wire;
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

fn main() {
    let instance = match TcpListener::bind("127.0.0.1:45874") {
        Ok(listener) => listener,
        Err(_) => {
            match TcpStream::connect_timeout(
                &"127.0.0.1:45874".parse().unwrap(),
                Duration::from_secs(1),
            ) {
                // Another copy is running: bring its window to the front.
                Ok(mut s) => {
                    let _ = s.write_all(b"LANT-SHOW");
                }
                // The port is held by something that does not answer.
                Err(_) => {
                    rfd::MessageDialog::new()
                        .set_title("邻传无法启动")
                        .set_description(
                            "本机端口 45874 被其他程序占用，邻传无法确认是否已在运行。\n请关闭占用该端口的程序后重试。",
                        )
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
            return;
        }
    };
    let icon = ui::icon(64);
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("邻传")
            .with_inner_size([460., 660.])
            .with_min_inner_size([400., 540.])
            .with_drag_and_drop(true)
            .with_icon(egui::IconData {
                rgba: icon,
                width: 64,
                height: 64,
            }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    // Without winit's default menu, ⌘Q reaches the window, so quitting asks
    // first when transfers are running.
    #[cfg(target_os = "macos")]
    {
        options.event_loop_builder = Some(Box::new(
            |builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                builder.with_default_menu(false);
            },
        ));
    }
    let result = eframe::run_native(
        "邻传",
        options,
        Box::new(move |cc| {
            ui::configure(&cc.egui_ctx);
            let (settings, warning) = model::load_settings();
            let shared = model::Shared::new(settings, cc.egui_ctx.clone());
            if let Ok(handle) = cc.window_handle() {
                if let RawWindowHandle::Win32(h) = handle.as_raw() {
                    shared.window.store(h.hwnd.get(), Ordering::Relaxed);
                }
            }
            let other = shared.clone();
            std::thread::spawn(move || {
                for mut s in instance.incoming().flatten() {
                    let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut bytes = [0; 9];
                    if s.read_exact(&mut bytes).is_ok() && &bytes == b"LANT-SHOW" {
                        other.show();
                    }
                }
            });
            let quit = Arc::new(AtomicBool::new(false));
            #[cfg(feature = "demo")]
            if let Some(app) = ui::demo::app(&cc.egui_ctx, shared.clone(), quit.clone()) {
                return Ok(Box::new(app));
            }
            let tray = ui::make_tray(shared.clone(), quit.clone());
            let tray_error = tray
                .as_ref()
                .err()
                .filter(|_| !cfg!(target_os = "linux"))
                .map(|e| {
                    format!(
                        "无法显示{}图标（{e}），关闭窗口将退出程序。",
                        if cfg!(target_os = "macos") {
                            "菜单栏"
                        } else {
                            "托盘"
                        }
                    )
                });
            network::start_receiver(shared.clone());
            discovery::start(shared.clone());
            #[allow(unused_mut)]
            let mut app = ui::App::new(shared, quit, tray.ok(), warning.or(tray_error));
            // End-to-end tests: files to pick at start, as if dropped.
            #[cfg(feature = "demo")]
            if let Ok(paths) = std::env::var("LAN_TRANSFER_PICK") {
                app.add_paths(paths.split(':').map(Into::into).collect());
            }
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = result {
        rfd::MessageDialog::new()
            .set_title("邻传无法启动")
            .set_description(format!(
                "无法创建应用窗口：{e}\n请确认显卡驱动支持 OpenGL 3.3，或联系开发者。"
            ))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
