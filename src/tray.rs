use image::GenericImageView;
use ksni::menu::*;
use ksni::{Tray, TrayService};
use tokio::sync::mpsc;

pub enum Event {
    Next,
    Quit,
}

struct WallpaperTray {
    tx: mpsc::UnboundedSender<Event>,
}

impl Tray for WallpaperTray {
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Event::Next);
    }

    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        const PNG: &[u8] = include_bytes!("../assets/tray-icon.png");
        let img = image::load_from_memory(PNG)
            .expect("failed to decode assets/tray-icon.png")
            .into_rgba8();
        let (w, h) = img.dimensions();
        // SNI expects ARGB32 (big-endian), image crate gives RGBA — swap to ARGB.
        let data = img
            .pixels()
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect();
        vec![ksni::Icon {
            width: w as i32,
            height: h as i32,
            data,
        }]
    }

    fn title(&self) -> String {
        "gtkwallpapers".into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Next wallpaper".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(Event::Next);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(Event::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the tray icon on a background thread. Events are sent over `tx`.
pub fn spawn(tx: mpsc::UnboundedSender<Event>) {
    std::thread::spawn(move || {
        if let Err(e) = TrayService::new(WallpaperTray { tx }).run() {
            eprintln!("Tray error: {e}");
        }
    });
}
