use imgui::{Context, StyleColor};
use proxy_fork_gui::{preferences::ThemePreference, render::apply_theme};
use winit::window::Theme;

#[test]
fn theme_resolution_and_palette_switching_preserve_readable_surfaces() {
    for (preference, system, expected) in [
        (ThemePreference::System, None, Theme::Light),
        (ThemePreference::System, Some(Theme::Light), Theme::Light),
        (ThemePreference::System, Some(Theme::Dark), Theme::Dark),
        (ThemePreference::Light, Some(Theme::Dark), Theme::Light),
        (ThemePreference::Dark, Some(Theme::Light), Theme::Dark),
    ] {
        assert_eq!(preference.resolve(system), expected);
    }
    assert_eq!(ThemePreference::System.native_theme(), None);
    assert_eq!(ThemePreference::Light.native_theme(), Some(Theme::Light));
    assert_eq!(ThemePreference::Dark.native_theme(), Some(Theme::Dark));

    let mut context = Context::create();
    context.style_mut().frame_rounding = 7.0;
    for theme in [Theme::Light, Theme::Dark, Theme::Light] {
        apply_theme(context.style_mut(), theme);
        let colors = &context.style().colors;
        for background in [
            StyleColor::WindowBg,
            StyleColor::ChildBg,
            StyleColor::PopupBg,
            StyleColor::FrameBg,
        ] {
            let bg = colors[background as usize];
            assert_eq!(bg[0] < 0.5, theme == Theme::Dark, "{background:?}");
            for foreground in [StyleColor::Text, StyleColor::TextDisabled] {
                assert!(
                    contrast(colors[foreground as usize], bg) >= 4.5,
                    "{theme:?} {foreground:?} on {background:?}"
                );
            }
        }
        assert_eq!(
            context.style().frame_rounding,
            7.0,
            "switching colors must not mutate geometry"
        );
    }
}

fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
    let luminance = |color: [f32; 4]| {
        let linear = color.map(|c| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
