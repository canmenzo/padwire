#![windows_subsystem = "windows"]
mod core;
mod input;
mod model;
mod pad;
mod selftest;
mod ui;

use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--selftest") {
        std::process::exit(selftest::run());
    }
    // --shot <png> [key] [xbox] [drive] [engine] [pad] [popup]: render one frame off screen and save it (for checking the UI)
    let shot = (args.first().is_some_and(|a| a == "--shot") && args.len() > 1).then(|| PathBuf::from(&args[1]));
    let extra = |s: &str| shot.is_some() && args[2..].iter().any(|a| a == s);
    let path = if shot.is_some() { std::env::temp_dir().join("padwire-shot.json") } else { model::Config::path() };
    let mut cfg = if shot.is_some() { model::Config::default() } else { model::Config::load(&path) };
    if extra("xbox") {
        cfg.console = model::Console::Xbox;
    }
    let pick = if shot.is_some() { args[2..].iter().find_map(|a| model::key_code(a)) } else { None };
    let sh = input::Shared::new(core::Core::new(&cfg), cfg.apps.clone(), cfg.console);
    if extra("drive") {
        sh.core.lock().unwrap().flip();
    }
    let (pad, popup) = (extra("pad"), extra("popup"));
    let engine = (shot.is_none() || extra("engine")).then(|| {
        let sh = sh.clone();
        std::thread::spawn(move || input::run(sh))
    });
    // window icon: a light d-pad on a dark tile
    let icon: Vec<u8> = (0..32 * 32)
        .flat_map(|i| {
            let (x, y) = (i % 32, i / 32);
            let cross = (13..19).contains(&x) && (6..26).contains(&y) || (13..19).contains(&y) && (6..26).contains(&x);
            if cross { [0xec, 0xeb, 0xe6, 0xff] } else { [0x12, 0x12, 0x17, 0xff] }
        })
        .collect();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("padwire")
        .with_icon(egui::IconData { rgba: icon, width: 32, height: 32 })
        .with_inner_size([1180.0, 700.0])
        .with_min_inner_size([885.0, 525.0]);
    if shot.is_some() {
        viewport = viewport.with_position([-30000.0, -30000.0]).with_taskbar(false).with_active(false);
    }
    let app_sh = sh.clone();
    let _ = eframe::run_native(
        "padwire",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(move |cc| {
            ui::fonts(&cc.egui_ctx);
            Ok(Box::new(ui::App::new(app_sh, cfg, path).shot(shot, pick, pad, popup)))
        }),
    );
    sh.quit.store(true, Relaxed);
    if let Some(e) = engine {
        let _ = e.join();
    }
}
