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

    fn icon_name(&self) -> String {
        "preferences-desktop-wallpaper".into()
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
