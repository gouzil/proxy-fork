rust_i18n::i18n!("locales", fallback = "en");
pub mod app;
pub mod event;
pub mod ime;
pub mod logging;
pub mod platform;
pub mod preferences;
pub mod render;
pub mod updater;
pub mod view;

pub fn translation_diagnostics() {
    #[cfg(debug_assertions)]
    {
        let resources: serde_json::Value = serde_json::from_str(include_str!("../locales/app.yml"))
            .expect("bundled translations must be valid");
        for (key, locale) in missing_translations(&resources) {
            tracing::debug!(
                translation_key = key,
                locale,
                "Translation missing; using English fallback"
            );
        }
    }
}

#[cfg(debug_assertions)]
fn missing_translations(resources: &serde_json::Value) -> Vec<(&str, &str)> {
    resources
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(_, values)| values.is_object())
        .flat_map(|(key, values)| {
            ["en", "zh-CN"].into_iter().filter_map(move |locale| {
                values
                    .get(locale)
                    .and_then(serde_json::Value::as_str)
                    .filter(|text| !text.is_empty())
                    .is_none()
                    .then_some((key.as_str(), locale))
            })
        })
        .collect()
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_identify_missing_locale_without_user_text() {
        let resource = serde_json::json!({"_version":2, "label":{"en":"Label"}});
        assert_eq!(missing_translations(&resource), vec![("label", "zh-CN")]);
    }
}
