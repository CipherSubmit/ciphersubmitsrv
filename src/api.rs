use std::path::Path;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path as AxumPath, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use tokio_util::io::ReaderStream;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::error::{AppError, AppResult};
use crate::models::{
    AdminLoginRequest, AdminLoginResponse, ChallengeResponse, CleanupResponse, DownloadDescriptor,
    E2EEnvelopeMetadata, FetchItem, ItemsResponse, RetentionPolicyView, RetentionStatus,
    RetrievalEventView, SubmissionAcceptedResponse, SubmissionDetailResponse, SubmissionMode,
    SubmissionOverviewItem, SubmissionStatus, TeacherActivityResponse, TeacherChallengeView,
    TeacherTokenView, VerifyResponse,
};
use crate::services;
use crate::storage::remove_file_if_exists;
use crate::AppState;

const HEADER_NAME_B64: &str = "x-cisub-name-b64";
const HEADER_STUDNUM_B64: &str = "x-cisub-studnum-b64";
const HEADER_FILE_NAME_B64: &str = "x-cisub-file-name-b64";
const HEADER_FILE_SHA256: &str = "x-cisub-file-sha256";
const HEADER_ENCRYPTED_KEY_B64: &str = "x-cisub-encrypted-key-b64";
const HEADER_NONCE_B64: &str = "x-cisub-nonce-b64";

#[derive(Debug)]
struct LinkSubmissionMetadata {
    name: String,
    studnum: String,
    file_name: String,
    file_sha256: String,
}

#[derive(Debug)]
struct E2ESubmissionMetadata {
    name: String,
    studnum: String,
    file_name: String,
    file_sha256: String,
    encrypted_key_b64: String,
    nonce_b64: String,
}

#[derive(Debug, Deserialize)]
struct TeacherChallengeRequest {
    public_key_pem: String,
}

#[derive(Debug, Deserialize)]
struct TeacherVerifyRequest {
    challenge_id: String,
    challenge_response_b64: String,
    public_key_pem: String,
}

pub fn router(state: AppState) -> Router {
    let router = Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/submissions/link", post(submit_link))
        .route("/api/v1/submissions/e2e", post(submit_e2e))
        .route("/api/v1/submissions", get(fetch_all_submissions))
        .route(
            "/api/v1/submissions/download/{submission_id}",
            get(download_submission_payload),
        )
        .route(
            "/api/v1/submissions/{studnum}",
            get(fetch_submissions_by_studnum),
        )
        .route(
            "/api/v1/auth/teacher/challenge",
            post(request_teacher_challenge),
        )
        .route(
            "/api/v1/auth/teacher/verify",
            post(verify_teacher_challenge),
        )
        .route("/api/v1/admin/auth/login", post(admin_login))
        .route("/api/v1/admin/overview", get(admin_overview))
        .route(
            "/api/v1/admin/submissions/download/{submission_id}",
            get(admin_download_submission_payload),
        )
        .route(
            "/api/v1/admin/submissions/{submission_id}",
            get(admin_submission_detail),
        )
        .route("/api/v1/admin/auth/activity", get(admin_teacher_activity))
        .route("/api/v1/admin/cleanup", post(admin_cleanup))
        .layer(PropagateRequestIdLayer::new(
            header::HeaderName::from_static("x-request-id"),
        ))
        .layer(SetRequestIdLayer::new(
            header::HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(DefaultBodyLimit::disable())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    attach_frontend(router, &state)
}

fn attach_frontend(router: Router, state: &AppState) -> Router {
    let dist_dir = state.config.frontend_dist_dir.clone();
    let index_file = dist_dir.join("index.html");

    if index_file.exists() {
        let static_files = ServeDir::new(dist_dir).not_found_service(ServeFile::new(index_file));
        router.fallback_service(static_files)
    } else {
        router.route("/", get(frontend_not_built))
    }
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn frontend_not_built() -> impl IntoResponse {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "前端静态资源尚未构建，请先在 frontend 目录执行 npm run build。",
    )
}

async fn submit_link(
    State(state): State<AppState>,
    request: Request,
) -> AppResult<Json<SubmissionAcceptedResponse>> {
    let (parts, body) = request.into_parts();
    let payload = parse_link_metadata(&parts.headers)?;

    ensure_non_empty(&payload.name, "name")?;
    ensure_non_empty(&payload.studnum, "studnum")?;
    ensure_non_empty(&payload.file_name, "file_name")?;
    ensure_non_empty(&payload.file_sha256, "file_sha256")?;

    let submission_id = services::generate_submission_id();
    let (storage_path, server_sha256) =
        services::save_link_file_stream(&state.config, &submission_id, body).await?;

    if server_sha256 != payload.file_sha256 {
        let _ = remove_file_if_exists(&storage_path);
        return Err(AppError::bad_request(
            "file_sha256 与服务端收到的 ZIP 原文不一致",
        ));
    }

    let record = services::build_submission_record(
        submission_id.clone(),
        payload.name,
        payload.studnum,
        payload.file_name,
        payload.file_sha256,
        SubmissionMode::Link,
        storage_path.display().to_string(),
        server_sha256.clone(),
        SubmissionStatus::Accepted,
    );
    state.store.insert_submission(&record)?;

    let inspection = match services::inspect_link_submission(
        &state.store,
        &submission_id,
        &storage_path,
        &server_sha256,
    ) {
        Ok(inspection) => inspection,
        Err(error) => {
            let _ = remove_file_if_exists(&storage_path);
            return Err(error);
        }
    };
    state.store.insert_link_inspection(&inspection)?;
    state
        .store
        .update_submission_status(&submission_id, SubmissionStatus::Inspected)?;

    Ok(Json(SubmissionAcceptedResponse {
        submission_id,
        accepted_at: services::format_rfc3339(record.accepted_at),
        server_message: "已接收明文 ZIP".to_string(),
    }))
}

async fn submit_e2e(
    State(state): State<AppState>,
    request: Request,
) -> AppResult<Json<SubmissionAcceptedResponse>> {
    let (parts, body) = request.into_parts();
    let payload = parse_e2e_metadata(&parts.headers)?;

    ensure_non_empty(&payload.name, "name")?;
    ensure_non_empty(&payload.studnum, "studnum")?;
    ensure_non_empty(&payload.file_name, "file_name")?;
    ensure_non_empty(&payload.file_sha256, "file_sha256")?;
    services::validate_streamed_envelope_fields(&payload.encrypted_key_b64, &payload.nonce_b64)?;

    let submission_id = services::generate_submission_id();
    let storage_path = services::save_e2e_envelope_stream(
        &state.config,
        &submission_id,
        &payload.encrypted_key_b64,
        &payload.nonce_b64,
        body,
    )
    .await?;
    let server_sha256 = payload.file_sha256.clone();

    let record = services::build_submission_record(
        submission_id.clone(),
        payload.name,
        payload.studnum,
        payload.file_name,
        payload.file_sha256,
        SubmissionMode::E2e,
        storage_path.display().to_string(),
        server_sha256,
        SubmissionStatus::CiphertextOnly,
    );
    state.store.insert_submission(&record)?;

    Ok(Json(SubmissionAcceptedResponse {
        submission_id,
        accepted_at: services::format_rfc3339(record.accepted_at),
        server_message: "已接收密文文件".to_string(),
    }))
}

async fn request_teacher_challenge(
    State(state): State<AppState>,
    Json(payload): Json<TeacherChallengeRequest>,
) -> AppResult<Json<ChallengeResponse>> {
    ensure_non_empty(&payload.public_key_pem, "public_key_pem")?;

    let fingerprint = services::public_key_fingerprint(&payload.public_key_pem)?;
    let challenge_id = services::generate_challenge_id();
    let challenge_bytes = services::generate_random_challenge();
    let encrypted = services::encrypt_challenge(&payload.public_key_pem, &challenge_bytes)?;
    let record = services::build_teacher_challenge(
        challenge_id.clone(),
        fingerprint,
        &challenge_bytes,
        &state.config,
    );
    state.store.insert_teacher_challenge(&record)?;

    Ok(Json(ChallengeResponse {
        challenge_id,
        encrypted_challenge_b64: base64::engine::general_purpose::STANDARD.encode(encrypted),
    }))
}

async fn verify_teacher_challenge(
    State(state): State<AppState>,
    Json(payload): Json<TeacherVerifyRequest>,
) -> AppResult<Json<VerifyResponse>> {
    ensure_non_empty(&payload.challenge_id, "challenge_id")?;
    ensure_non_empty(&payload.challenge_response_b64, "challenge_response_b64")?;
    ensure_non_empty(&payload.public_key_pem, "public_key_pem")?;

    let challenge = state
        .store
        .get_teacher_challenge(&payload.challenge_id)?
        .ok_or_else(|| AppError::not_found("challenge_id 不存在"))?;

    if challenge.used {
        return Err(AppError::conflict("challenge_id 已被使用"));
    }

    let expires_at = services::parse_rfc3339(&challenge.expires_at)?;
    if expires_at <= Utc::now() {
        return Err(AppError::forbidden("挑战已过期"));
    }

    let current_fingerprint = services::public_key_fingerprint(&payload.public_key_pem)?;
    if current_fingerprint != challenge.public_key_fingerprint {
        return Err(AppError::forbidden("公钥指纹与挑战记录不匹配"));
    }

    let challenge_response =
        services::decode_base64_field("challenge_response_b64", &payload.challenge_response_b64)?;
    let original_challenge =
        services::decode_base64_field("challenge_bytes", &challenge.challenge_b64)?;

    if challenge_response != original_challenge {
        return Err(AppError::forbidden("挑战应答内容不正确"));
    }

    state
        .store
        .mark_teacher_challenge_used(&payload.challenge_id)?;
    let token = services::build_teacher_token(current_fingerprint, &state.config);
    state.store.insert_teacher_token(&token)?;

    Ok(Json(VerifyResponse {
        access_token: token.token,
    }))
}

async fn admin_login(
    State(state): State<AppState>,
    Json(payload): Json<AdminLoginRequest>,
) -> AppResult<Json<AdminLoginResponse>> {
    ensure_non_empty(&payload.username, "username")?;
    ensure_non_empty(&payload.password, "password")?;

    if payload.username != state.config.admin_username
        || payload.password != state.config.admin_password
    {
        return Err(AppError::unauthorized("用户名或密码错误"));
    }

    let token = services::build_admin_token(&payload.username, &state.config);
    state.store.insert_admin_token(&token)?;

    Ok(Json(AdminLoginResponse {
        access_token: token.token,
        expires_at: token.expires_at,
    }))
}

async fn fetch_all_submissions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<ItemsResponse>> {
    let _token = authorize_teacher(&state, &headers)?;
    let records = state.store.list_submissions(None)?;

    if records.is_empty() {
        return Err(AppError::not_found("当前没有可取件作业"));
    }

    let response = build_items_response(&state, &records)?;

    Ok(Json(response))
}

async fn fetch_submissions_by_studnum(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(studnum): AxumPath<String>,
) -> AppResult<Json<ItemsResponse>> {
    let _token = authorize_teacher(&state, &headers)?;
    let records = state.store.list_submissions(Some(&studnum))?;

    if records.is_empty() {
        return Err(AppError::not_found("指定学号没有可取件作业"));
    }

    let response = build_items_response(&state, &records)?;

    Ok(Json(response))
}

async fn download_submission_payload(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(submission_id): AxumPath<String>,
) -> AppResult<impl IntoResponse> {
    let _token = authorize_teacher(&state, &headers)?;
    let record = state
        .store
        .get_submission_by_id(&submission_id)?
        .ok_or_else(|| AppError::not_found("submission_id 不存在"))?;
    build_download_response(&state, &record, true).await
}

async fn admin_download_submission_payload(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(submission_id): AxumPath<String>,
) -> AppResult<impl IntoResponse> {
    let _token = authorize_admin(&state, &headers)?;
    let record = state
        .store
        .get_submission_by_id(&submission_id)?
        .ok_or_else(|| AppError::not_found("submission_id 不存在"))?;

    build_download_response(&state, &record, false).await
}

async fn admin_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<SubmissionOverviewItem>>> {
    let _token = authorize_admin(&state, &headers)?;
    let records = state.store.list_submissions(None)?;
    let items = records
        .into_iter()
        .map(|record| SubmissionOverviewItem {
            submission_id: record.submission_id,
            name: record.name,
            studnum: record.studnum,
            file_name: record.file_name,
            file_sha256: record.file_sha256,
            accepted_at: services::format_rfc3339(record.accepted_at),
            mode: record.mode,
            status: record.status,
        })
        .collect::<Vec<_>>();

    Ok(Json(items))
}

async fn admin_submission_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(submission_id): AxumPath<String>,
) -> AppResult<Json<SubmissionDetailResponse>> {
    let _token = authorize_admin(&state, &headers)?;
    let record = state
        .store
        .get_submission_by_id(&submission_id)?
        .ok_or_else(|| AppError::not_found("submission_id 不存在"))?;

    let inspection = state.store.get_link_inspection(&record.submission_id)?;
    let envelope = if record.mode == SubmissionMode::E2e {
        let (encrypted_key_b64, nonce_b64) =
            services::load_e2e_envelope_metadata(Path::new(&record.storage_path))?;
        Some(E2EEnvelopeMetadata {
            encrypted_key_b64,
            nonce_b64,
        })
    } else {
        None
    };

    Ok(Json(SubmissionDetailResponse {
        submission_id: record.submission_id,
        name: record.name,
        studnum: record.studnum,
        file_name: record.file_name,
        file_sha256: record.file_sha256,
        accepted_at: services::format_rfc3339(record.accepted_at),
        mode: record.mode.clone(),
        status: record.status.clone(),
        server_can_read_content: record.mode == SubmissionMode::Link,
        inspection,
        envelope,
        retention: RetentionStatus {
            retrieved_at: record.retrieved_at.map(services::format_rfc3339),
            scheduled_delete_at: record.scheduled_delete_at.map(services::format_rfc3339),
        },
    }))
}

async fn admin_teacher_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<TeacherActivityResponse>> {
    let _token = authorize_admin(&state, &headers)?;
    let challenges = state
        .store
        .list_recent_challenges(10)?
        .into_iter()
        .map(|item| TeacherChallengeView {
            challenge_id: item.challenge_id,
            public_key_fingerprint: item.public_key_fingerprint,
            created_at: item.created_at,
            expires_at: item.expires_at,
            used: item.used,
        })
        .collect::<Vec<_>>();

    let tokens = state
        .store
        .list_recent_tokens(10)?
        .into_iter()
        .map(|item| TeacherTokenView {
            issued_at: item.issued_at,
            expires_at: item.expires_at,
            bound_public_key_fingerprint: item.bound_public_key_fingerprint,
        })
        .collect::<Vec<_>>();

    let retrievals = state
        .store
        .list_recent_retrievals(20)?
        .into_iter()
        .map(|item| RetrievalEventView {
            submission_id: item.submission_id,
            studnum: item.studnum,
            retrieved_at: item.retrieved_at,
            scheduled_delete_at: item.scheduled_delete_at,
        })
        .collect::<Vec<_>>();

    Ok(Json(TeacherActivityResponse {
        recent_challenges: challenges,
        recent_tokens: tokens,
        recent_retrievals: retrievals,
        retention_policy: RetentionPolicyView {
            strategy: "delayed_delete".to_string(),
            delete_delay_seconds: state.config.retrieval_delete_delay_secs,
        },
    }))
}

async fn admin_cleanup(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<CleanupResponse>> {
    let _token = authorize_admin(&state, &headers)?;
    let deleted_submission_ids = services::cleanup_expired_submissions(&state.store)?;

    Ok(Json(CleanupResponse {
        deleted_count: deleted_submission_ids.len(),
        deleted_submission_ids,
    }))
}

fn authorize_admin(state: &AppState, headers: &HeaderMap) -> AppResult<String> {
    let token = bearer_token(headers)?;
    let record = state
        .store
        .get_admin_token(&token)?
        .ok_or_else(|| AppError::unauthorized("管理员访问令牌无效"))?;

    let expires_at = services::parse_rfc3339(&record.expires_at)?;
    if expires_at <= Utc::now() {
        return Err(AppError::unauthorized("管理员访问令牌已过期"));
    }

    Ok(record.token)
}

fn authorize_teacher(state: &AppState, headers: &HeaderMap) -> AppResult<String> {
    let token = bearer_token(headers)?;
    let record = state
        .store
        .get_teacher_token(&token)?
        .ok_or_else(|| AppError::unauthorized("Bearer Token 无效"))?;

    let expires_at = services::parse_rfc3339(&record.expires_at)?;
    if expires_at <= Utc::now() {
        return Err(AppError::unauthorized("Bearer Token 已过期"));
    }

    Ok(record.token)
}

fn bearer_token(headers: &HeaderMap) -> AppResult<String> {
    let raw = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("缺少 Authorization: Bearer <token>"))?
        .to_str()
        .map_err(|_| AppError::unauthorized("Authorization 头不是合法 UTF-8"))?;

    raw.strip_prefix("Bearer ")
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::unauthorized("Authorization 头格式必须为 Bearer <token>"))
}

async fn build_download_response(
    state: &AppState,
    record: &crate::models::SubmissionRecord,
    track_retrieval: bool,
) -> AppResult<impl IntoResponse> {
    if record.status == SubmissionStatus::Deleted {
        return Err(AppError::not_found("提交内容已删除，无法下载"));
    }

    let download_path =
        services::submission_download_path(&record.mode, Path::new(&record.storage_path));
    let file = tokio::fs::File::open(&download_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::not_found("提交内容已删除或文件不存在")
        } else {
            AppError::internal(format!("打开下载文件失败: {error}"))
        }
    })?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| AppError::internal(format!("读取下载文件元数据失败: {error}")))?;

    if track_retrieval {
        let scheduled_delete_at = services::calculate_scheduled_delete_at(&state.config);
        let retrieved_at = services::format_rfc3339(Utc::now());
        let submission_ids = vec![record.submission_id.clone()];
        state.store.schedule_submission_deletion(
            &submission_ids,
            &retrieved_at,
            &scheduled_delete_at,
        )?;
    }

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let download_name = services::submission_download_file_name(&record.mode, &record.file_name);

    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_LENGTH, metadata.len().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", download_name.replace('"', "_")),
            ),
        ],
        body,
    ))
}

fn build_items_response(
    state: &AppState,
    records: &[crate::models::SubmissionRecord],
) -> AppResult<ItemsResponse> {
    let mut items = Vec::with_capacity(records.len());

    for record in records {
        let download = match record.mode {
            SubmissionMode::Link => DownloadDescriptor::Link,
            SubmissionMode::E2e => {
                let (encrypted_key_b64, nonce_b64) =
                    services::load_e2e_envelope_metadata(Path::new(&record.storage_path))?;
                DownloadDescriptor::E2e {
                    encrypted_key_b64,
                    nonce_b64,
                }
            }
        };

        let item = FetchItem {
            submission_id: record.submission_id.clone(),
            studnum: record.studnum.clone(),
            file_name: record.file_name.clone(),
            accepted_at: services::format_rfc3339(record.accepted_at),
            mode: record.mode.clone(),
            download,
        };

        items.push(item);
    }

    let _ = state;
    Ok(ItemsResponse { items })
}

fn ensure_non_empty(value: &str, field_name: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::bad_request(format!("字段 {field_name} 不能为空")));
    }

    Ok(())
}

fn parse_link_metadata(headers: &HeaderMap) -> AppResult<LinkSubmissionMetadata> {
    Ok(LinkSubmissionMetadata {
        name: decode_b64_header(headers, HEADER_NAME_B64)?,
        studnum: decode_b64_header(headers, HEADER_STUDNUM_B64)?,
        file_name: decode_b64_header(headers, HEADER_FILE_NAME_B64)?,
        file_sha256: required_header(headers, HEADER_FILE_SHA256)?,
    })
}

fn parse_e2e_metadata(headers: &HeaderMap) -> AppResult<E2ESubmissionMetadata> {
    Ok(E2ESubmissionMetadata {
        name: decode_b64_header(headers, HEADER_NAME_B64)?,
        studnum: decode_b64_header(headers, HEADER_STUDNUM_B64)?,
        file_name: decode_b64_header(headers, HEADER_FILE_NAME_B64)?,
        file_sha256: required_header(headers, HEADER_FILE_SHA256)?,
        encrypted_key_b64: required_header(headers, HEADER_ENCRYPTED_KEY_B64)?,
        nonce_b64: required_header(headers, HEADER_NONCE_B64)?,
    })
}

fn required_header(headers: &HeaderMap, header_name: &str) -> AppResult<String> {
    headers
        .get(header_name)
        .ok_or_else(|| AppError::bad_request(format!("缺少请求头 {header_name}")))?
        .to_str()
        .map(|value| value.to_string())
        .map_err(|_| AppError::bad_request(format!("请求头 {header_name} 不是合法 UTF-8")))
}

fn decode_b64_header(headers: &HeaderMap, header_name: &str) -> AppResult<String> {
    let value = required_header(headers, header_name)?;
    let bytes = services::decode_base64_field(header_name, &value)?;
    String::from_utf8(bytes)
        .map_err(|_| AppError::bad_request(format!("请求头 {header_name} 不是合法 UTF-8 数据")))
}

#[cfg(test)]
mod tests {
    use super::build_items_response;
    use crate::config::AppConfig;
    use crate::models::{
        DownloadDescriptor, FetchItem, SubmissionMode, SubmissionRecord, SubmissionStatus,
    };
    use crate::storage::Store;
    use crate::AppState;
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn build_items_response_includes_e2e_download_metadata() {
        let temp_dir = tempdir().expect("能创建临时目录");
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(data_dir.join("submissions/e2e")).expect("能创建目录");
        let storage_path = data_dir.join("submissions/e2e/sub-1.json");
        std::fs::write(
            &storage_path,
            r#"{
  "encrypted_key_b64": "ZW5jcnlwdGVk",
  "nonce_b64": "MTIzNDU2Nzg5MDEy"
}"#,
        )
        .expect("能写入元数据");

        let config = AppConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            db_path: data_dir.join("db.sqlite3"),
            data_dir: data_dir.clone(),
            frontend_dist_dir: temp_dir.path().join("frontend-dist"),
            tls_cert_path: data_dir.join("tls/server-cert.pem"),
            tls_key_path: data_dir.join("tls/server-key.pem"),
            admin_username: "admin".to_string(),
            admin_password: "admin123".to_string(),
            challenge_ttl_secs: 300,
            token_ttl_secs: 1800,
            retrieval_delete_delay_secs: 3600,
        };
        let state = AppState {
            config: config.clone(),
            store: Arc::new(Store::new(&config).expect("能创建存储")),
        };
        let records = vec![SubmissionRecord {
            submission_id: "sub-1".to_string(),
            name: "Bob".to_string(),
            studnum: "20260002".to_string(),
            file_name: "project.zip".to_string(),
            file_sha256: "dummy".to_string(),
            accepted_at: Utc::now(),
            mode: SubmissionMode::E2e,
            payload_kind: SubmissionMode::E2e,
            storage_path: storage_path.display().to_string(),
            status: SubmissionStatus::CiphertextOnly,
            server_sha256: "dummy".to_string(),
            retrieved_at: None,
            scheduled_delete_at: None,
        }];

        let response = build_items_response(&state, &records).expect("能构建响应");
        let item: &FetchItem = &response.items[0];
        match &item.download {
            DownloadDescriptor::E2e {
                encrypted_key_b64,
                nonce_b64,
            } => {
                assert_eq!(encrypted_key_b64, "ZW5jcnlwdGVk");
                assert_eq!(nonce_b64, "MTIzNDU2Nzg5MDEy");
            }
            DownloadDescriptor::Link => panic!("e2e 项不应返回 link 下载描述"),
        }
    }
}
