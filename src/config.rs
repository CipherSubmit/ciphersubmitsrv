use std::env;
use std::fs;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub data_dir: PathBuf,
    pub frontend_dist_dir: PathBuf,
    pub tls_cert_path: PathBuf,
    pub tls_key_path: PathBuf,
    pub authorized_teacher_keys_dir: PathBuf,
    pub admin_username: String,
    pub admin_password: String,
    pub challenge_ttl_secs: i64,
    pub token_ttl_secs: i64,
    pub retrieval_delete_delay_secs: i64,
}

impl AppConfig {
    pub fn from_env() -> AppResult<Self> {
        let data_dir =
            PathBuf::from(env::var("CISUB_DATA_DIR").unwrap_or_else(|_| "data".to_string()));
        fs::create_dir_all(&data_dir)
            .map_err(|error| AppError::internal(format!("创建数据目录失败: {error}")))?;

        let tls_dir = data_dir.join("tls");
        fs::create_dir_all(&tls_dir)
            .map_err(|error| AppError::internal(format!("创建 TLS 目录失败: {error}")))?;

        let authorized_teacher_keys_dir = PathBuf::from(
            env::var("CISUB_TEACHER_KEYS_DIR")
                .unwrap_or_else(|_| data_dir.join("teacher_keys").display().to_string()),
        );
        fs::create_dir_all(&authorized_teacher_keys_dir).map_err(|error| {
            AppError::internal(format!("创建教师公钥目录失败: {error}"))
        })?;

        Ok(Self {
            bind_addr: env::var("CISUB_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8443".to_string()),
            db_path: PathBuf::from(
                env::var("CISUB_DB_PATH")
                    .unwrap_or_else(|_| data_dir.join("cipher_submit.db").display().to_string()),
            ),
            data_dir,
            frontend_dist_dir: PathBuf::from(env::var("CISUB_FRONTEND_DIST").unwrap_or_else(
                |_| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("frontend")
                        .join("dist")
                        .display()
                        .to_string()
                },
            )),
            tls_cert_path: PathBuf::from(
                env::var("CISUB_TLS_CERT")
                    .unwrap_or_else(|_| tls_dir.join("server-cert.pem").display().to_string()),
            ),
            tls_key_path: PathBuf::from(
                env::var("CISUB_TLS_KEY")
                    .unwrap_or_else(|_| tls_dir.join("server-key.pem").display().to_string()),
            ),
            authorized_teacher_keys_dir,
            admin_username: env::var("CISUB_ADMIN_USERNAME")
                .unwrap_or_else(|_| "admin".to_string()),
            admin_password: env::var("CISUB_ADMIN_PASSWORD")
                .unwrap_or_else(|_| "admin123".to_string()),
            challenge_ttl_secs: read_i64("CISUB_CHALLENGE_TTL_SECS", 300)?,
            token_ttl_secs: read_i64("CISUB_TOKEN_TTL_SECS", 1800)?,
            retrieval_delete_delay_secs: read_i64("CISUB_DELETE_DELAY_SECS", 3600)?,
        })
    }
}

fn read_i64(key: &str, default_value: i64) -> AppResult<i64> {
    match env::var(key) {
        Ok(value) => value
            .parse::<i64>()
            .map_err(|error| AppError::internal(format!("环境变量 {key} 不是合法整数: {error}"))),
        Err(_) => Ok(default_value),
    }
}
