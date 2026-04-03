pub mod api;
pub mod config;
pub mod error;
pub mod models;
pub mod services;
pub mod storage;

use std::sync::Arc;

use config::AppConfig;
use storage::Store;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: Arc<Store>,
}