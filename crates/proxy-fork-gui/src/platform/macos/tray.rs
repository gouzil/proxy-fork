use crate::{app::Action, event::AppEvent};
use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use winit::event_loop::EventLoopProxy;

pub struct Tray {
    _icon: TrayIcon,
    dashboard: MenuItem,
    system_proxy: CheckMenuItem,
    toggle: MenuItem,
    quit: MenuItem,
}
impl Tray {
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Result<Self> {
        let menu = Menu::new();
        let dashboard = MenuItem::with_id("dashboard", rust_i18n::t!("tray.dashboard"), true, None);
        let system_proxy = CheckMenuItem::with_id(
            "system-proxy",
            rust_i18n::t!("tray.system_proxy"),
            true,
            false,
            None,
        );
        let toggle = MenuItem::with_id("toggle", rust_i18n::t!("action.start"), true, None);
        let quit = MenuItem::with_id("quit", rust_i18n::t!("action.exit"), true, None);
        let first_separator = PredefinedMenuItem::separator();
        let second_separator = PredefinedMenuItem::separator();
        menu.append_items(&[
            &dashboard,
            &first_separator,
            &system_proxy,
            &toggle,
            &second_separator,
            &quit,
        ])?;
        let callback = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let Some(event) = menu_event(event.id.as_ref()) else {
                return;
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
            for x in 3..13 {
                let glyph =
                    x == 3 || (y == 2 && x <= 12) || (y == 8 && x <= 12) || (x == 12 && y <= 8);
                if glyph {
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
            dashboard,
            system_proxy,
            toggle,
            quit,
        })
    }
    pub fn update(&self, running: bool, busy: bool, system_proxy: bool) {
        self.dashboard.set_text(rust_i18n::t!("tray.dashboard"));
        self.system_proxy
            .set_text(rust_i18n::t!("tray.system_proxy"));
        self.system_proxy.set_enabled(!busy);
        self.system_proxy.set_checked(system_proxy);
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

fn menu_event(id: &str) -> Option<AppEvent> {
    match id {
        "dashboard" => Some(AppEvent::ShowDashboard),
        "system-proxy" => Some(AppEvent::ToggleSystemProxy),
        "toggle" => Some(AppEvent::Action(Action::Start)),
        "quit" => Some(AppEvent::Action(Action::Exit)),
        _ => None,
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
    fn tray_menu_ids_route_to_expected_events() {
        assert!(matches!(
            menu_event("dashboard"),
            Some(AppEvent::ShowDashboard)
        ));
        assert!(matches!(
            menu_event("system-proxy"),
            Some(AppEvent::ToggleSystemProxy)
        ));
        assert!(matches!(
            menu_event("quit"),
            Some(AppEvent::Action(Action::Exit))
        ));
        assert!(menu_event("unknown").is_none());
    }

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
