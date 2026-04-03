use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::error::{AppError, AppResult};
use crate::models::{
    ChallengeResponse, CleanupResponse, Envelope, FetchItem, ItemsResponse, RetentionPolicyView,
    RetentionStatus, RetrievalEventView, SubmissionAcceptedResponse, SubmissionDetailResponse,
    SubmissionMode, SubmissionOverviewItem, SubmissionPayload, SubmissionStatus,
    TeacherActivityResponse, TeacherChallengeView, TeacherTokenView, VerifyResponse,
};
use crate::services;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct LinkSubmissionRequest {
    name: String,
    studnum: String,
    file_name: String,
    file_sha256: String,
    file_b64: String,
}

#[derive(Debug, Deserialize)]
struct E2ESubmissionRequest {
    name: String,
    studnum: String,
    file_name: String,
    file_sha256: String,
    envelope: Envelope,
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
        .route("/api/v1/admin/overview", get(admin_overview))
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
    Json(payload): Json<LinkSubmissionRequest>,
) -> AppResult<Json<SubmissionAcceptedResponse>> {
    ensure_non_empty(&payload.name, "name")?;
    ensure_non_empty(&payload.studnum, "studnum")?;
    ensure_non_empty(&payload.file_name, "file_name")?;
    ensure_non_empty(&payload.file_sha256, "file_sha256")?;
    ensure_non_empty(&payload.file_b64, "file_b64")?;

    let zip_bytes = services::decode_base64_field("file_b64", &payload.file_b64)?;
    let server_sha256 = services::sha256_hex(&zip_bytes);

    if server_sha256 != payload.file_sha256 {
        return Err(AppError::bad_request(
            "file_sha256 与服务端收到的 ZIP 原文不一致",
        ));
    }

    let submission_id = services::generate_submission_id();
    let storage_path = services::save_link_file(&state.config, &submission_id, &zip_bytes)?;

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

    let inspection = services::inspect_link_submission(
        &state.store,
        &submission_id,
        &zip_bytes,
        &server_sha256,
    )?;
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
    Json(payload): Json<E2ESubmissionRequest>,
) -> AppResult<Json<SubmissionAcceptedResponse>> {
    ensure_non_empty(&payload.name, "name")?;
    ensure_non_empty(&payload.studnum, "studnum")?;
    ensure_non_empty(&payload.file_name, "file_name")?;
    ensure_non_empty(&payload.file_sha256, "file_sha256")?;
    services::validate_envelope(&payload.envelope)?;

    let submission_id = services::generate_submission_id();
    let storage_path =
        services::save_e2e_envelope(&state.config, &submission_id, &payload.envelope)?;
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
    schedule_retrieval_if_needed(&state, &response.items)?;

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
    schedule_retrieval_if_needed(&state, &response.items)?;

    Ok(Json(response))
}

async fn admin_overview(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<SubmissionOverviewItem>>> {
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
    AxumPath(submission_id): AxumPath<String>,
) -> AppResult<Json<SubmissionDetailResponse>> {
    let record = state
        .store
        .get_submission_by_id(&submission_id)?
        .ok_or_else(|| AppError::not_found("submission_id 不存在"))?;

    let inspection = state.store.get_link_inspection(&record.submission_id)?;
    let envelope = if record.mode == SubmissionMode::E2e {
        Some(services::load_e2e_envelope(Path::new(
            &record.storage_path,
        ))?)
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
) -> AppResult<Json<TeacherActivityResponse>> {
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
    let _token = authorize_teacher(&state, &headers)?;
    let deleted_submission_ids = services::cleanup_expired_submissions(&state.store)?;

    Ok(Json(CleanupResponse {
        deleted_count: deleted_submission_ids.len(),
        deleted_submission_ids,
    }))
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

fn build_items_response(
    state: &AppState,
    records: &[crate::models::SubmissionRecord],
) -> AppResult<ItemsResponse> {
    let mut items = Vec::with_capacity(records.len());

    for record in records {
        let payload = match record.mode {
            SubmissionMode::Link => SubmissionPayload::Link {
                file_b64: services::load_link_file_b64(Path::new(&record.storage_path))?,
            },
            SubmissionMode::E2e => SubmissionPayload::E2e {
                envelope: services::load_e2e_envelope(Path::new(&record.storage_path))?,
            },
        };

        let item = FetchItem {
            submission_id: record.submission_id.clone(),
            studnum: record.studnum.clone(),
            file_name: record.file_name.clone(),
            accepted_at: services::format_rfc3339(record.accepted_at),
            mode: record.mode.clone(),
            payload,
        };

        if !mode_matches_payload(&item) {
            return Err(AppError::internal("mode 与 payload.kind 不一致"));
        }

        items.push(item);
    }

    let _ = state;
    Ok(ItemsResponse { items })
}

fn schedule_retrieval_if_needed(state: &AppState, items: &[FetchItem]) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }

    let retrieved_at = services::format_rfc3339(Utc::now());
    let scheduled_delete_at = services::calculate_scheduled_delete_at(&state.config);
    let submission_ids = items
        .iter()
        .map(|item| item.submission_id.clone())
        .collect::<Vec<_>>();

    state
        .store
        .schedule_submission_deletion(&submission_ids, &retrieved_at, &scheduled_delete_at)
}

fn mode_matches_payload(item: &FetchItem) -> bool {
    matches!(
        (&item.mode, &item.payload),
        (SubmissionMode::Link, SubmissionPayload::Link { .. })
            | (SubmissionMode::E2e, SubmissionPayload::E2e { .. })
    )
}

fn ensure_non_empty(value: &str, field_name: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::bad_request(format!("字段 {field_name} 不能为空")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::mode_matches_payload;
    use crate::models::{FetchItem, SubmissionMode, SubmissionPayload};

    #[test]
    fn payload_kind_must_match_mode() {
        let item = FetchItem {
            submission_id: "sub-1".to_string(),
            studnum: "20260001".to_string(),
            file_name: "homework.zip".to_string(),
            accepted_at: "2026-04-03T12:00:00Z".to_string(),
            mode: SubmissionMode::Link,
            payload: SubmissionPayload::Link {
                file_b64: "UEsDBA==".to_string(),
            },
        };
        assert!(mode_matches_payload(&item));
    }
}
