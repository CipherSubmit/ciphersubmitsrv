use std::fs;
use std::path::{Path, PathBuf};

use axum::body::Body;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt;
use rand::rngs::OsRng;
use rand::RngCore;
use rcgen::{generate_simple_self_signed, CertifiedKey};
use rsa::pkcs8::{DecodePublicKey, EncodePublicKey};
use rsa::{Oaep, RsaPublicKey};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use zip::ZipArchive;

use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::models::{
    AdminTokenRecord, Envelope, LinkInspectionRecord, SubmissionMode, SubmissionRecord,
    SubmissionStatus, TeacherChallengeRecord, TeacherTokenRecord,
};
use crate::storage::{remove_file_if_exists, Store};

pub async fn ensure_tls_assets(config: &AppConfig) -> AppResult<()> {
    if config.tls_cert_path.exists() && config.tls_key_path.exists() {
        return Ok(());
    }

    if let Some(parent) = config.tls_cert_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError::internal(format!("创建证书目录失败: {error}")))?;
    }

    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .map_err(|error| AppError::internal(format!("生成自签证书失败: {error}")))?;

    tokio::fs::write(&config.tls_cert_path, cert.pem())
        .await
        .map_err(|error| AppError::internal(format!("写入证书文件失败: {error}")))?;
    tokio::fs::write(&config.tls_key_path, key_pair.serialize_pem())
        .await
        .map_err(|error| AppError::internal(format!("写入私钥文件失败: {error}")))?;

    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn format_rfc3339(datetime: DateTime<Utc>) -> String {
    datetime.to_rfc3339()
}

pub fn parse_rfc3339(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|datetime| datetime.with_timezone(&Utc))
        .map_err(|error| AppError::internal(format!("时间格式解析失败: {error}")))
}

pub fn generate_submission_id() -> String {
    format!("sub-{}", Uuid::new_v4())
}

pub fn generate_challenge_id() -> String {
    format!("challenge-{}", Uuid::new_v4())
}

pub fn generate_token() -> String {
    let mut random = [0_u8; 32];
    OsRng.fill_bytes(&mut random);
    URL_SAFE_NO_PAD.encode(random)
}

pub fn decode_base64_field(field_name: &str, value: &str) -> AppResult<Vec<u8>> {
    STANDARD.decode(value).map_err(|error| {
        AppError::bad_request(format!("字段 {field_name} 不是合法 Base64: {error}"))
    })
}

pub fn validate_envelope(envelope: &Envelope) -> AppResult<()> {
    let encrypted_key = decode_base64_field("encrypted_key_b64", &envelope.encrypted_key_b64)?;
    let nonce = decode_base64_field("nonce_b64", &envelope.nonce_b64)?;
    let ciphertext = decode_base64_field("ciphertext_b64", &envelope.ciphertext_b64)?;

    if encrypted_key.is_empty() || nonce.is_empty() || ciphertext.is_empty() {
        return Err(AppError::bad_request("envelope 字段不能为空"));
    }

    if nonce.len() != 12 {
        return Err(AppError::bad_request("nonce_b64 解码后必须为 12 字节"));
    }

    Ok(())
}

pub fn validate_streamed_envelope_fields(
    encrypted_key_b64: &str,
    nonce_b64: &str,
) -> AppResult<()> {
    let encrypted_key = decode_base64_field("encrypted_key_b64", encrypted_key_b64)?;
    let nonce = decode_base64_field("nonce_b64", nonce_b64)?;

    if encrypted_key.is_empty() || nonce.is_empty() {
        return Err(AppError::bad_request("envelope 字段不能为空"));
    }

    if nonce.len() != 12 {
        return Err(AppError::bad_request("nonce_b64 解码后必须为 12 字节"));
    }

    Ok(())
}

pub fn save_link_file(config: &AppConfig, submission_id: &str, bytes: &[u8]) -> AppResult<PathBuf> {
    let path = config
        .data_dir
        .join("submissions")
        .join("link")
        .join(format!("{submission_id}.zip"));
    write_bytes(&path, bytes)?;
    Ok(path)
}

pub async fn save_link_file_stream(
    config: &AppConfig,
    submission_id: &str,
    body: Body,
) -> AppResult<(PathBuf, String)> {
    let path = config
        .data_dir
        .join("submissions")
        .join("link")
        .join(format!("{submission_id}.zip"));
    let sha256 = stream_body_to_file(&path, body).await?;
    Ok((path, sha256))
}

pub fn save_e2e_envelope(
    config: &AppConfig,
    submission_id: &str,
    envelope: &Envelope,
) -> AppResult<PathBuf> {
    let path = config
        .data_dir
        .join("submissions")
        .join("e2e")
        .join(format!("{submission_id}.json"));
    let content = serde_json::to_vec_pretty(envelope)
        .map_err(|error| AppError::internal(format!("序列化 envelope 失败: {error}")))?;
    write_bytes(&path, &content)?;
    Ok(path)
}

pub async fn save_e2e_envelope_stream(
    config: &AppConfig,
    submission_id: &str,
    encrypted_key_b64: &str,
    nonce_b64: &str,
    body: Body,
) -> AppResult<PathBuf> {
    let metadata_path = config
        .data_dir
        .join("submissions")
        .join("e2e")
        .join(format!("{submission_id}.json"));
    let ciphertext_path = e2e_ciphertext_path(&metadata_path);
    let metadata = StoredEnvelopeMetadata {
        encrypted_key_b64: encrypted_key_b64.to_string(),
        nonce_b64: nonce_b64.to_string(),
    };
    let content = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| AppError::internal(format!("序列化 envelope 元数据失败: {error}")))?;

    write_bytes(&metadata_path, &content)?;
    if let Err(error) = stream_body_to_file(&ciphertext_path, body).await {
        let _ = remove_file_if_exists(&metadata_path);
        let _ = remove_file_if_exists(&ciphertext_path);
        return Err(error);
    }

    Ok(metadata_path)
}

pub fn inspect_link_submission(
    store: &Store,
    submission_id: &str,
    zip_path: &Path,
    server_sha256: &str,
) -> AppResult<LinkInspectionRecord> {
    let file = fs::File::open(zip_path)
        .map_err(|error| AppError::internal(format!("读取 ZIP 文件失败: {error}")))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| AppError::bad_request(format!("上传文件不是合法 ZIP: {error}")))?;

    let mut has_git_dir = false;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| AppError::bad_request(format!("读取 ZIP 条目失败: {error}")))?;
        let name = file.name().to_string();

        if is_git_trace(&name) {
            has_git_dir = true;
        }

        if entries.len() < 32 {
            entries.push(name);
        }
    }

    let duplicate_submission_ids =
        store.find_duplicate_submission_ids(server_sha256, submission_id)?;
    let duplicate_sha256 = if duplicate_submission_ids.is_empty() {
        None
    } else {
        Some(server_sha256.to_string())
    };

    Ok(LinkInspectionRecord {
        submission_id: submission_id.to_string(),
        has_git_dir,
        zip_entries_summary: entries,
        duplicate_sha256,
        duplicate_submission_ids,
        inspected_at: format_rfc3339(Utc::now()),
    })
}

pub fn encrypt_challenge(public_key_pem: &str, challenge_bytes: &[u8]) -> AppResult<Vec<u8>> {
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem).map_err(|error| {
        AppError::bad_request(format!("public_key_pem 不是合法 RSA 公钥: {error}"))
    })?;

    // 客户端当前使用 RSA-OAEP(SHA-256) 解密 challenge，服务端必须保持一致。
    public_key
        .encrypt(&mut OsRng, Oaep::new::<Sha256>(), challenge_bytes)
        .map_err(|error| AppError::internal(format!("加密挑战失败: {error}")))
}

pub fn public_key_fingerprint(public_key_pem: &str) -> AppResult<String> {
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem).map_err(|error| {
        AppError::bad_request(format!("public_key_pem 不是合法 RSA 公钥: {error}"))
    })?;
    let der = public_key
        .to_public_key_der()
        .map_err(|error| AppError::internal(format!("导出公钥 DER 失败: {error}")))?;
    let digest = Sha256::digest(der.as_bytes());
    let fingerprint = digest
        .iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<_>>()
        .join(":");

    Ok(format!("SHA256:{fingerprint}"))
}

pub fn build_submission_record(
    submission_id: String,
    name: String,
    studnum: String,
    file_name: String,
    file_sha256: String,
    mode: SubmissionMode,
    storage_path: String,
    server_sha256: String,
    status: SubmissionStatus,
) -> SubmissionRecord {
    SubmissionRecord {
        submission_id,
        name,
        studnum,
        file_name,
        file_sha256,
        accepted_at: Utc::now(),
        payload_kind: mode.clone(),
        mode,
        storage_path,
        status,
        server_sha256,
        retrieved_at: None,
        scheduled_delete_at: None,
    }
}

pub fn build_teacher_challenge(
    challenge_id: String,
    public_key_fingerprint: String,
    challenge_bytes: &[u8],
    config: &AppConfig,
) -> TeacherChallengeRecord {
    let created_at = Utc::now();
    let expires_at = created_at + Duration::seconds(config.challenge_ttl_secs);

    TeacherChallengeRecord {
        challenge_id,
        public_key_fingerprint,
        challenge_b64: STANDARD.encode(challenge_bytes),
        created_at: format_rfc3339(created_at),
        expires_at: format_rfc3339(expires_at),
        used: false,
    }
}

pub fn build_teacher_token(fingerprint: String, config: &AppConfig) -> TeacherTokenRecord {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(config.token_ttl_secs);

    TeacherTokenRecord {
        token: generate_token(),
        issued_at: format_rfc3339(issued_at),
        expires_at: format_rfc3339(expires_at),
        bound_public_key_fingerprint: fingerprint,
    }
}

pub fn build_admin_token(username: &str, config: &AppConfig) -> AdminTokenRecord {
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::seconds(config.token_ttl_secs);

    AdminTokenRecord {
        token: generate_token(),
        username: username.to_string(),
        issued_at: format_rfc3339(issued_at),
        expires_at: format_rfc3339(expires_at),
    }
}

pub fn generate_random_challenge() -> Vec<u8> {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.to_vec()
}

pub fn calculate_scheduled_delete_at(config: &AppConfig) -> String {
    format_rfc3339(Utc::now() + Duration::seconds(config.retrieval_delete_delay_secs))
}

pub fn load_link_file_b64(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path)
        .map_err(|error| AppError::internal(format!("读取 ZIP 文件失败: {error}")))?;
    Ok(STANDARD.encode(bytes))
}

pub fn load_e2e_envelope_metadata(path: &Path) -> AppResult<(String, String)> {
    let content = fs::read_to_string(path)
        .map_err(|error| AppError::internal(format!("读取 envelope 元数据失败: {error}")))?;
    let metadata: StoredEnvelopeMetadata = serde_json::from_str(&content)
        .map_err(|error| AppError::internal(format!("解析 envelope 元数据失败: {error}")))?;

    Ok((metadata.encrypted_key_b64, metadata.nonce_b64))
}

pub fn submission_download_path(mode: &SubmissionMode, storage_path: &Path) -> PathBuf {
    match mode {
        SubmissionMode::Link => storage_path.to_path_buf(),
        SubmissionMode::E2e => e2e_ciphertext_path(storage_path),
    }
}

pub fn submission_download_file_name(mode: &SubmissionMode, original_file_name: &str) -> String {
    match mode {
        SubmissionMode::Link => original_file_name.to_string(),
        SubmissionMode::E2e => {
            let path = Path::new(original_file_name);
            if path.extension().and_then(|value| value.to_str()) == Some("bin") {
                return original_file_name.to_string();
            }

            match path.file_stem().and_then(|value| value.to_str()) {
                Some(stem) if !stem.is_empty() => format!("{stem}.bin"),
                _ => format!("{original_file_name}.bin"),
            }
        }
    }
}

pub fn load_e2e_envelope(path: &Path) -> AppResult<Envelope> {
    let content = fs::read_to_string(path)
        .map_err(|error| AppError::internal(format!("读取 envelope 元数据失败: {error}")))?;
    let metadata: StoredEnvelopeMetadata = serde_json::from_str(&content)
        .map_err(|error| AppError::internal(format!("解析 envelope 元数据失败: {error}")))?;
    let ciphertext = fs::read(e2e_ciphertext_path(path))
        .map_err(|error| AppError::internal(format!("读取密文文件失败: {error}")))?;

    Ok(Envelope {
        encrypted_key_b64: metadata.encrypted_key_b64,
        nonce_b64: metadata.nonce_b64,
        ciphertext_b64: STANDARD.encode(ciphertext),
    })
}

pub fn cleanup_expired_submissions(store: &Store) -> AppResult<Vec<String>> {
    let now = format_rfc3339(Utc::now());
    let expired = store.list_expired_submission_paths(&now)?;
    let mut deleted = Vec::new();

    for (submission_id, storage_path) in expired {
        remove_submission_payload_files(Path::new(&storage_path))?;
        store.mark_submission_deleted(&submission_id)?;
        deleted.push(submission_id);
    }

    Ok(deleted)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
    }

    fs::write(path, bytes).map_err(|error| AppError::internal(format!("写入文件失败: {error}")))
}

async fn stream_body_to_file(path: &Path, body: Body) -> AppResult<String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
    }

    let temp_path = temporary_upload_path(path);
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|error| AppError::internal(format!("创建临时上传文件失败: {error}")))?;
    let mut stream = body.into_data_stream();
    let mut hasher = Sha256::new();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| AppError::bad_request(format!("读取请求体失败: {error}")))?;
        if chunk.is_empty() {
            continue;
        }

        file.write_all(&chunk)
            .await
            .map_err(|error| AppError::internal(format!("写入上传文件失败: {error}")))?;
        hasher.update(&chunk);
    }

    file.flush()
        .await
        .map_err(|error| AppError::internal(format!("刷新上传文件失败: {error}")))?;
    drop(file);

    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(|error| AppError::internal(format!("保存上传文件失败: {error}")))?;

    Ok(hex::encode(hasher.finalize()))
}

fn temporary_upload_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".part");
    PathBuf::from(os)
}

fn remove_submission_payload_files(path: &Path) -> AppResult<()> {
    remove_file_if_exists(path)?;
    if path.extension().and_then(|value| value.to_str()) == Some("json") {
        let ciphertext_path = e2e_ciphertext_path(path);
        remove_file_if_exists(&ciphertext_path)?;
    }
    Ok(())
}

fn e2e_ciphertext_path(metadata_path: &Path) -> PathBuf {
    if metadata_path.extension().and_then(|value| value.to_str()) == Some("json") {
        metadata_path.with_extension("bin")
    } else {
        metadata_path.to_path_buf()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredEnvelopeMetadata {
    encrypted_key_b64: String,
    nonce_b64: String,
}

fn is_git_trace(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == ".git"
        || lower.starts_with(".git/")
        || lower.contains("/.git/")
        || lower.contains("/.git")
        || lower.ends_with(".git")
}

#[cfg(test)]
mod tests {
    use super::{is_git_trace, sha256_hex, submission_download_file_name};
    use rsa::pkcs8::{EncodePublicKey, LineEnding};
    use rsa::rand_core::OsRng;
    use rsa::{Oaep, RsaPrivateKey};
    use sha2::Sha256;

    use crate::services::encrypt_challenge;

    #[test]
    fn sha256_hex_returns_lowercase_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn detect_git_related_entries() {
        assert!(is_git_trace(".git/config"));
        assert!(is_git_trace("project/.git/HEAD"));
        assert!(!is_git_trace("src/main.rs"));
    }

    #[test]
    fn challenge_encryption_is_cli_compatible() {
        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("能生成测试私钥");
        let public_key_pem = private_key
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("能导出测试公钥");
        let challenge = b"cipher-submit-test-challenge";

        let encrypted = encrypt_challenge(&public_key_pem, challenge).expect("能加密挑战");
        let decrypted = private_key
            .decrypt(Oaep::new::<Sha256>(), &encrypted)
            .expect("客户端能用 OAEP-SHA256 解密 challenge");

        assert_eq!(decrypted, challenge);
    }

    #[test]
    fn submission_download_file_name_matches_mode() {
        assert_eq!(
            submission_download_file_name(&crate::models::SubmissionMode::Link, "target.zip"),
            "target.zip"
        );
        assert_eq!(
            submission_download_file_name(&crate::models::SubmissionMode::E2e, "target.zip"),
            "target.bin"
        );
    }
}
