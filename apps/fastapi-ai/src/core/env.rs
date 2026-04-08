//! Environment loading.
//!
//! Mirrors `app/core/env.py` field-for-field. Loaded once at startup
//! via figment + dotenvy and threaded through `AppState`.

use anyhow::Result;
use figment::Figment;
use figment::providers::Env as FigEnv;
use serde::Deserialize;

/// Application settings, populated from env vars.
///
/// Field names mirror the Python `Env` class:
/// - `ML_PREFIX_API` → `ml_prefix_api`
/// - `APP_NAME` → `app_name` (default `"fastapi-ai"`)
/// - `APP_ENVIRONMENT` → `app_environment` (default `"development"`)
/// - `DATABASE_URL` → `database_url`
/// - `OPENAI_API_KEY` → `openai_api_key`
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` → `otel_exporter_otlp_endpoint`
#[derive(Debug, Clone, Deserialize)]
pub struct Env {
    #[serde(rename = "ML_PREFIX_API")]
    pub ml_prefix_api: String,

    #[serde(rename = "APP_NAME", default = "default_app_name")]
    pub app_name: String,

    #[serde(rename = "APP_ENVIRONMENT", default = "default_app_environment")]
    pub app_environment: String,

    #[serde(rename = "DATABASE_URL")]
    pub database_url: String,

    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: String,

    #[serde(rename = "OTEL_EXPORTER_OTLP_ENDPOINT")]
    pub otel_exporter_otlp_endpoint: String,
}

fn default_app_name() -> String {
    "fastapi-ai".to_string()
}

fn default_app_environment() -> String {
    "development".to_string()
}

impl Env {
    /// Load from process environment.
    pub fn from_environment() -> Result<Self> {
        let figment = Figment::new().merge(FigEnv::raw());
        let env: Self = figment.extract()?;
        Ok(env)
    }

    /// Convenience: are we running in production?
    pub fn is_production(&self) -> bool {
        self.app_environment == "production"
    }
}
