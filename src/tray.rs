use ksni::menu::*;
use ksni::{Handle, Tray, TrayService};
use tokio::sync::mpsc;

pub enum Event {
    Next,
    DeleteNext,
    Info,
    BlockPhotographer,
    UnblockPhotographer(String),
    ShowTerms,
    Quit,
}

/// What the menu shows besides the fixed actions; refreshed by the daemon.
#[derive(Default, Clone, PartialEq)]
pub struct State {
    /// Photographer of the current wallpaper, if known.
    pub photographer: Option<String>,
    pub terms: Vec<String>,
    pub blocked: Vec<String>,
}

struct WallpaperTray {
    tx: mpsc::UnboundedSender<Event>,
    state: State,
}

/// Escape text for a menu label (a single `_` marks an access key).
fn label(text: &str) -> String {
    text.replace('_', "__")
}

fn send(event: impl Fn() -> Event + 'static) -> Box<dyn Fn(&mut WallpaperTray)> {
    Box::new(move |this: &mut WallpaperTray| {
        let _ = this.tx.send(event());
    })
}

fn info_item(text: &str) -> MenuItem<WallpaperTray> {
    StandardItem {
        label: label(text),
        enabled: false,
        ..Default::default()
    }
    .into()
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
        let block_label = match &self.state.photographer {
            Some(name) => format!("Block photographer “{name}” + Next"),
            None => "Block photographer (unknown)".to_owned(),
        };

        let mut terms: Vec<MenuItem<Self>> =
            self.state.terms.iter().map(|t| info_item(t)).collect();
        if terms.is_empty() {
            terms.push(info_item("(none)"));
        }

        let mut blocked: Vec<MenuItem<Self>> = self
            .state
            .blocked
            .iter()
            .map(|name| {
                let name = name.clone();
                StandardItem {
                    label: label(&format!("Unblock “{name}”")),
                    activate: send(move || Event::UnblockPhotographer(name.clone())),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        if blocked.is_empty() {
            blocked.push(info_item("(none)"));
        }

        vec![
            StandardItem {
                label: "Next wallpaper".into(),
                activate: send(|| Event::Next),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Delete + Next Wallpaper".into(),
                activate: send(|| Event::DeleteNext),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: label(&block_label),
                enabled: self.state.photographer.is_some(),
                activate: send(|| Event::BlockPhotographer),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Info".into(),
                activate: send(|| Event::Info),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Search Terms & Blocked Photographers…".into(),
                activate: send(|| Event::ShowTerms),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: format!("Search Terms ({})", self.state.terms.len()),
                submenu: terms,
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: format!("Blocked Photographers ({})", self.state.blocked.len()),
                submenu: blocked,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: send(|| Event::Quit),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Handle for refreshing the tray menu from the daemon.
pub struct TrayHandle(Handle<WallpaperTray>);

impl TrayHandle {
    pub fn set_state(&self, state: State) {
        self.0.update(|tray| {
            if tray.state != state {
                tray.state = state;
            }
        });
    }
}

/// Spawn the tray icon on a background thread. Events are sent over `tx`.
pub fn spawn(tx: mpsc::UnboundedSender<Event>) -> TrayHandle {
    let service = TrayService::new(WallpaperTray {
        tx,
        state: State::default(),
    });
    let handle = service.handle();
    std::thread::spawn(move || {
        if let Err(e) = service.run() {
            eprintln!("Tray error: {e}");
        }
    });
    TrayHandle(handle)
}
