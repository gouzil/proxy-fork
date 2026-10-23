use crate::{app::Action, event::AppEvent};
use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use winit::event_loop::EventLoopProxy;

pub struct Tray {
    _icon: TrayIcon,
    show: MenuItem,
    toggle: MenuItem,
    quit: MenuItem,
}
impl Tray {
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Result<Self> {
        let menu = Menu::new();
        let show = MenuItem::with_id("show", rust_i18n::t!("action.show"), true, None);
        let toggle = MenuItem::with_id("toggle", rust_i18n::t!("action.start"), true, None);
        let quit = MenuItem::with_id("quit", rust_i18n::t!("action.exit"), true, None);
        menu.append_items(&[&show, &toggle, &quit])?;
        let callback = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let event = match event.id.as_ref() {
                "show" => AppEvent::Show,
                "toggle" => AppEvent::Action(Action::Start),
                "quit" => AppEvent::Action(Action::Exit),
                _ => return,
            };
            let _ = callback.send_event(event);
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if requests_window(&event) {
                let _ = proxy.send_event(AppEvent::Show);
            }
        }));
        let mut rgba = vec![0u8; 18 * 18 * 4];
        for y in 2..16 {
            for x in 2..16 {
                if x == 2 || x == 15 || y == 2 || y == 15 || y == 8 {
                    let i = (y * 18 + x) * 4;
                    rgba[i + 3] = 255;
                }
            }
        }
        let icon = Icon::from_rgba(rgba, 18, 18)?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip("Proxy Fork")
            .build()?;
        Ok(Self {
            _icon: icon,
            show,
            toggle,
            quit,
        })
    }
    pub fn update(&self, running: bool, busy: bool) {
        self.show.set_text(rust_i18n::t!("action.show"));
        self.toggle.set_text(rust_i18n::t!(if running {
            "action.stop"
        } else {
            "action.start"
        }));
        self.toggle.set_enabled(!busy);
        self.quit.set_enabled(!busy);
        self.quit.set_text(rust_i18n::t!("action.exit"));
    }
}

fn requests_window(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hovering_over_the_tray_does_not_restore_a_hidden_window() {
        let enter = TrayIconEvent::Enter {
            id: "test".into(),
            position: (0.0, 0.0).into(),
            rect: Default::default(),
        };
        assert!(!requests_window(&enter));
        let click = TrayIconEvent::Click {
            id: "test".into(),
            position: (0.0, 0.0).into(),
            rect: Default::default(),
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
        };
        assert!(requests_window(&click));
    }
}
