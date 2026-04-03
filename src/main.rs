mod api;
mod config;
mod error;
mod models;
mod services;
mod storage;

use std::net::SocketAddr;
use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use config::AppConfig;
use storage::Store;
use tracing::{error, info};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub store: Arc<Store>,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        error!(error = %error, "服务启动失败");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), error::AppError> {
    init_tracing();

    let config = AppConfig::from_env()?;
    services::ensure_tls_assets(&config).await?;

    let store = Arc::new(Store::new(&config)?);
    store.init_schema()?;

    let state = AppState {
        config: config.clone(),
        store,
    };

    let router = api::router(state.clone());
    let tls_config = RustlsConfig::from_pem_chain_file(&config.tls_cert_path, &config.tls_key_path)
        .await
        .map_err(|error| error::AppError::internal(format!("TLS 配置加载失败: {error}")))?;

    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .map_err(|error| error::AppError::internal(format!("监听地址无效: {error}")))?;

    info!(address = %addr, "CipherSubmit Server 已启动");

    axum_server::bind_rustls(addr, tls_config)
        .serve(router.into_make_service())
        .await
        .map_err(|error| error::AppError::internal(format!("HTTP 服务异常退出: {error}")))
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ciphersubmitsrv=info,tower_http=info".into()),
        )
        .compact()
        .init();
}
