use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ciphersubmitsrv::api;
use ciphersubmitsrv::config::AppConfig;
use ciphersubmitsrv::models::Envelope;
use ciphersubmitsrv::services;
use ciphersubmitsrv::storage::Store;
use ciphersubmitsrv::AppState;
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::rand_core::OsRng;
use rsa::{Oaep, RsaPrivateKey};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use tempfile::tempdir;
use tower::util::ServiceExt;
use zip::write::SimpleFileOptions;

#[derive(Debug, Deserialize)]
struct SubmissionAcceptedResponse {
    submission_id: String,
}

#[derive(Debug, Deserialize)]
struct ChallengeResponse {
    challenge_id: String,
    encrypted_challenge_b64: String,
}

#[derive(Debug, Deserialize)]
struct VerifyResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct ItemsResponse {
    items: Vec<FetchItem>,
}

#[derive(Debug, Deserialize)]
struct FetchItem {
    submission_id: String,
    studnum: String,
    file_name: String,
    mode: String,
    payload: SubmissionPayload,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum SubmissionPayload {
    Link { file_b64: String },
    E2e { envelope: Envelope },
}

#[derive(Debug, Deserialize)]
struct SubmissionDetailResponse {
    submission_id: String,
    status: String,
    server_can_read_content: bool,
    envelope: Option<Envelope>,
}

fn test_config() -> (tempfile::TempDir, AppConfig) {
    let temp_dir = tempdir().expect("应该能创建临时目录");
    let data_dir = temp_dir.path().join("data");

    let config = AppConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        db_path: data_dir.join("cipher_submit.db"),
        data_dir: data_dir.clone(),
        frontend_dist_dir: temp_dir.path().join("frontend-dist"),
        tls_cert_path: data_dir.join("tls/server-cert.pem"),
        tls_key_path: data_dir.join("tls/server-key.pem"),
        challenge_ttl_secs: 300,
        token_ttl_secs: 1800,
        retrieval_delete_delay_secs: 3600,
    };

    (temp_dir, config)
}

fn test_app() -> (tempfile::TempDir, axum::Router) {
    let (temp_dir, config) = test_config();
    let store = Arc::new(Store::new(&config).expect("应该能创建测试存储"));
    store.init_schema().expect("应该能初始化数据表");

    let app = api::router(AppState { config, store });
    (temp_dir, app)
}

fn build_zip() -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    writer
        .start_file("homework.txt", SimpleFileOptions::default())
        .expect("应该能创建 ZIP 条目");
    writer
        .write_all(b"cipher submit integration test")
        .expect("应该能写入 ZIP 内容");

    writer
        .finish()
        .expect("应该能完成 ZIP 写入")
        .into_inner()
}

async fn request_json<T: DeserializeOwned>(
    app: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, T) {
    let response = app.clone().oneshot(request).await.expect("请求应该成功执行");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("应该能读取响应体");
    let payload = serde_json::from_slice(&body).expect("响应体应该是合法 JSON");
    (status, payload)
}

async fn issue_teacher_token(app: &axum::Router) -> String {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("应该能生成教师私钥");
    let public_key_pem = private_key
        .to_public_key()
        .to_public_key_pem(LineEnding::LF)
        .expect("应该能导出教师公钥");

    let challenge_request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/teacher/challenge")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "public_key_pem": public_key_pem }).to_string(),
        ))
        .expect("应该能构造 challenge 请求");

    let (status, challenge): (StatusCode, ChallengeResponse) =
        request_json(app, challenge_request).await;
    assert_eq!(status, StatusCode::OK);

    let encrypted = STANDARD
        .decode(challenge.encrypted_challenge_b64)
        .expect("challenge 密文应该是合法 base64");
    let plaintext = private_key
        .decrypt(Oaep::new::<Sha256>(), &encrypted)
        .expect("应该能解密服务端 challenge");

    let verify_request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/teacher/verify")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "challenge_id": challenge.challenge_id,
                "challenge_response_b64": STANDARD.encode(plaintext),
                "public_key_pem": public_key_pem,
            })
            .to_string(),
        ))
        .expect("应该能构造 verify 请求");

    let (status, verify): (StatusCode, VerifyResponse) = request_json(app, verify_request).await;
    assert_eq!(status, StatusCode::OK);
    verify.access_token
}

#[tokio::test]
async fn link_submission_auth_and_fetch_flow() {
    let (_temp_dir, app) = test_app();
    let zip_bytes = build_zip();
    let zip_sha256 = services::sha256_hex(&zip_bytes);

    let submit_request = Request::builder()
        .method("POST")
        .uri("/api/v1/submissions/link")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Alice",
                "studnum": "20260001",
                "file_name": "homework.zip",
                "file_sha256": zip_sha256,
                "file_b64": STANDARD.encode(&zip_bytes),
            })
            .to_string(),
        ))
        .expect("应该能构造提交请求");

    let (status, accepted): (StatusCode, SubmissionAcceptedResponse) =
        request_json(&app, submit_request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(accepted.submission_id.starts_with("sub-"));

    let token = issue_teacher_token(&app).await;
    let fetch_request = Request::builder()
        .method("GET")
        .uri("/api/v1/submissions/20260001")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("应该能构造取件请求");

    let (status, response): (StatusCode, ItemsResponse) = request_json(&app, fetch_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response.items.len(), 1);

    let item = &response.items[0];
    assert_eq!(item.submission_id, accepted.submission_id);
    assert_eq!(item.studnum, "20260001");
    assert_eq!(item.file_name, "homework.zip");
    assert_eq!(item.mode, "link");

    match &item.payload {
        SubmissionPayload::Link { file_b64 } => {
            assert_eq!(STANDARD.decode(file_b64).expect("返回文件应可解码"), zip_bytes);
        }
        SubmissionPayload::E2e { .. } => panic!("link 提交不应返回 e2e payload"),
    }
}

#[tokio::test]
async fn e2e_submission_is_visible_in_admin_detail() {
    let (_temp_dir, app) = test_app();
    let envelope = Envelope {
        encrypted_key_b64: STANDARD.encode(b"encrypted-key"),
        nonce_b64: STANDARD.encode(b"123456789012"),
        ciphertext_b64: STANDARD.encode(b"ciphertext"),
    };

    let submit_request = Request::builder()
        .method("POST")
        .uri("/api/v1/submissions/e2e")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "name": "Bob",
                "studnum": "20260002",
                "file_name": "project.zip",
                "file_sha256": "dummy-client-side-sha256",
                "envelope": envelope,
            })
            .to_string(),
        ))
        .expect("应该能构造 e2e 提交请求");

    let (status, accepted): (StatusCode, SubmissionAcceptedResponse) =
        request_json(&app, submit_request).await;
    assert_eq!(status, StatusCode::OK);

    let detail_request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/admin/submissions/{}", accepted.submission_id))
        .body(Body::empty())
        .expect("应该能构造详情请求");

    let (status, detail): (StatusCode, SubmissionDetailResponse) =
        request_json(&app, detail_request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail.submission_id, accepted.submission_id);
    assert_eq!(detail.status, "ciphertext_only");
    assert!(!detail.server_can_read_content);
    assert_eq!(detail.envelope.expect("应该包含 envelope").ciphertext_b64, STANDARD.encode(b"ciphertext"));
}