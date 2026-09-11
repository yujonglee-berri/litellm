use std::env;

use crate::Error;

const ENABLED: &str = "LITELLM_OTEL_ENABLED";
const ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const TRACES_ENDPOINT: &str = "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT";
const HEADERS: &str = "OTEL_EXPORTER_OTLP_HEADERS";
const SERVICE_NAME: &str = "OTEL_SERVICE_NAME";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    endpoint: Option<String>,
    traces_endpoint: Option<String>,
    headers: Option<String>,
    service_name: String,
}

impl Config {
    pub fn from_env() -> Result<Option<Self>, Error> {
        let Some(enabled) = env::var_os(ENABLED) else {
            return Ok(None);
        };
        let enabled = enabled.to_string_lossy().into_owned();
        match enabled.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Some(Self {
                endpoint: nonempty_env(ENDPOINT),
                traces_endpoint: nonempty_env(TRACES_ENDPOINT),
                headers: nonempty_env(HEADERS),
                service_name: nonempty_env(SERVICE_NAME)
                    .unwrap_or_else(|| "litellm-gateway".to_string()),
            })),
            "0" | "false" | "no" | "off" => Ok(None),
            _ => Err(Error::InvalidEnvironment {
                name: ENABLED,
                value: enabled,
            }),
        }
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    pub fn traces_endpoint(&self) -> Option<&str> {
        self.traces_endpoint.as_deref()
    }

    pub fn headers(&self) -> Option<&str> {
        self.headers.as_deref()
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }
}

fn nonempty_env(name: &'static str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{Config, ENABLED, ENDPOINT, HEADERS, SERVICE_NAME, TRACES_ENDPOINT};

    static ENV: Mutex<()> = Mutex::new(());

    #[test]
    fn absent_enablement_is_disabled() {
        let _guard = ENV.lock().expect("environment lock");
        clear();

        assert_eq!(Config::from_env().expect("configuration resolves"), None);
    }

    #[test]
    fn enabled_configuration_reads_standard_otel_environment() {
        let _guard = ENV.lock().expect("environment lock");
        clear();
        unsafe {
            std::env::set_var(ENABLED, "true");
            std::env::set_var(ENDPOINT, "https://collector.example.com");
            std::env::set_var(SERVICE_NAME, "litellm-test");
        }

        let config = Config::from_env()
            .expect("configuration resolves")
            .expect("configuration is enabled");

        assert_eq!(config.endpoint(), Some("https://collector.example.com"));
        assert_eq!(config.service_name(), "litellm-test");
        clear();
    }

    #[test]
    fn invalid_enablement_is_rejected() {
        let _guard = ENV.lock().expect("environment lock");
        clear();
        unsafe { std::env::set_var(ENABLED, "sometimes") };

        assert!(Config::from_env().is_err());
        clear();
    }

    fn clear() {
        for name in [ENABLED, ENDPOINT, TRACES_ENDPOINT, HEADERS, SERVICE_NAME] {
            unsafe { std::env::remove_var(name) };
        }
    }
}
