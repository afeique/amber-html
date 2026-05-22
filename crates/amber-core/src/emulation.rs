//! Device / locale / timezone / dark-mode emulation. See `Plans.md` (task 2.6).
//!
//! This is the pure command-construction layer: [`commands`] turns an
//! [`EmulationConfig`] into the ordered list of CDP `(method, params)` calls the
//! render path issues before navigating. Only configured knobs emit a command.

use serde_json::{json, Value};

/// Viewport / device metrics to emulate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// CSS-to-device pixel ratio (1.0 = standard, 2.0 = "retina").
    pub device_scale_factor: f64,
    /// Emulate a mobile device (affects viewport meta handling, etc.).
    pub mobile: bool,
}

impl Viewport {
    /// A standard desktop viewport at `width`×`height`.
    pub fn desktop(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            device_scale_factor: 1.0,
            mobile: false,
        }
    }
}

/// Emulation knobs for a capture. Unset knobs are left at the browser default.
#[derive(Debug, Clone, Default)]
pub struct EmulationConfig {
    pub viewport: Option<Viewport>,
    pub user_agent: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub dark_mode: bool,
}

/// Build the ordered CDP commands that apply `cfg`. Empty when nothing is set.
pub fn commands(cfg: &EmulationConfig) -> Vec<(&'static str, Value)> {
    let mut cmds = Vec::new();

    if let Some(v) = &cfg.viewport {
        cmds.push((
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": v.width,
                "height": v.height,
                "deviceScaleFactor": v.device_scale_factor,
                "mobile": v.mobile,
            }),
        ));
    }
    if let Some(ua) = &cfg.user_agent {
        let mut params = json!({ "userAgent": ua });
        if let Some(locale) = &cfg.locale {
            params["acceptLanguage"] = json!(locale);
        }
        cmds.push(("Emulation.setUserAgentOverride", params));
    }
    if let Some(locale) = &cfg.locale {
        cmds.push(("Emulation.setLocaleOverride", json!({ "locale": locale })));
    }
    if let Some(tz) = &cfg.timezone {
        cmds.push(("Emulation.setTimezoneOverride", json!({ "timezoneId": tz })));
    }
    if cfg.dark_mode {
        cmds.push((
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-color-scheme", "value": "dark" }] }),
        ));
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_emits_no_commands() {
        assert!(commands(&EmulationConfig::default()).is_empty());
    }

    #[test]
    fn viewport_builds_device_metrics() {
        let cfg = EmulationConfig {
            viewport: Some(Viewport::desktop(1280, 800)),
            ..Default::default()
        };
        let cmds = commands(&cfg);
        assert_eq!(cmds.len(), 1);
        let (method, params) = &cmds[0];
        assert_eq!(*method, "Emulation.setDeviceMetricsOverride");
        assert_eq!(params["width"], 1280);
        assert_eq!(params["height"], 800);
        assert_eq!(params["deviceScaleFactor"], 1.0);
        assert_eq!(params["mobile"], false);
    }

    #[test]
    fn user_agent_carries_accept_language_when_locale_set() {
        let cfg = EmulationConfig {
            user_agent: Some("AmberBot/1".to_string()),
            locale: Some("fr-FR".to_string()),
            ..Default::default()
        };
        let cmds = commands(&cfg);
        let ua = cmds
            .iter()
            .find(|(m, _)| *m == "Emulation.setUserAgentOverride")
            .unwrap();
        assert_eq!(ua.1["userAgent"], "AmberBot/1");
        assert_eq!(ua.1["acceptLanguage"], "fr-FR");
        // A locale override is also emitted.
        assert!(cmds.iter().any(|(m, _)| *m == "Emulation.setLocaleOverride"));
    }

    #[test]
    fn timezone_and_dark_mode() {
        let cfg = EmulationConfig {
            timezone: Some("America/New_York".to_string()),
            dark_mode: true,
            ..Default::default()
        };
        let cmds = commands(&cfg);
        let tz = cmds
            .iter()
            .find(|(m, _)| *m == "Emulation.setTimezoneOverride")
            .unwrap();
        assert_eq!(tz.1["timezoneId"], "America/New_York");
        let media = cmds
            .iter()
            .find(|(m, _)| *m == "Emulation.setEmulatedMedia")
            .unwrap();
        assert_eq!(media.1["features"][0]["name"], "prefers-color-scheme");
        assert_eq!(media.1["features"][0]["value"], "dark");
    }

    #[test]
    fn full_config_orders_commands() {
        let cfg = EmulationConfig {
            viewport: Some(Viewport::desktop(390, 844)),
            user_agent: Some("UA".to_string()),
            locale: Some("en-US".to_string()),
            timezone: Some("UTC".to_string()),
            dark_mode: true,
        };
        let methods: Vec<&str> = commands(&cfg).iter().map(|(m, _)| *m).collect();
        assert_eq!(
            methods,
            vec![
                "Emulation.setDeviceMetricsOverride",
                "Emulation.setUserAgentOverride",
                "Emulation.setLocaleOverride",
                "Emulation.setTimezoneOverride",
                "Emulation.setEmulatedMedia",
            ]
        );
    }
}
