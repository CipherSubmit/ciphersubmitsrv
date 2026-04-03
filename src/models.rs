use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SubmissionMode {
    Link,
    E2e,
}

impl SubmissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubmissionMode::Link => "link",
            SubmissionMode::E2e => "e2e",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "link" => Some(Self::Link),
            "e2e" => Some(Self::E2e),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    Accepted,
    Inspected,
    CiphertextOnly,
    ScheduledDelete,
    Deleted,
}

impl SubmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubmissionStatus::Accepted => "accepted",
            SubmissionStatus::Inspected => "inspected",
            SubmissionStatus::CiphertextOnly => "ciphertext_only",
            SubmissionStatus::ScheduledDelete => "scheduled_delete",
            SubmissionStatus::Deleted => "deleted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "inspected" => Some(Self::Inspected),
            "ciphertext_only" => Some(Self::CiphertextOnly),
            "scheduled_delete" => Some(Self::ScheduledDelete),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub encrypted_key_b64: String,
    pub nonce_b64: String,
    pub ciphertext_b64: String,
}

#[derive(Clone, Debug)]
pub struct SubmissionRecord {
    pub submission_id: String,
    pub name: String,
    pub studnum: String,
    pub file_name: String,
    pub file_sha256: String,
    pub accepted_at: DateTime<Utc>,
    pub mode: SubmissionMode,
    pub payload_kind: SubmissionMode,
    pub storage_path: String,
    pub status: SubmissionStatus,
    pub server_sha256: String,
    pub retrieved_at: Option<DateTime<Utc>>,
    pub scheduled_delete_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubmissionAcceptedResponse {
    pub submission_id: String,
    pub accepted_at: String,
    pub server_message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkInspectionRecord {
    pub submission_id: String,
    pub has_git_dir: bool,
    pub zip_entries_summary: Vec<String>,
    pub duplicate_sha256: Option<String>,
    pub duplicate_submission_ids: Vec<String>,
    pub inspected_at: String,
}

#[derive(Clone, Debug)]
pub struct TeacherChallengeRecord {
    pub challenge_id: String,
    pub public_key_fingerprint: String,
    pub challenge_b64: String,
    pub created_at: String,
    pub expires_at: String,
    pub used: bool,
}

#[derive(Clone, Debug)]
pub struct TeacherTokenRecord {
    pub token: String,
    pub issued_at: String,
    pub expires_at: String,
    pub bound_public_key_fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemsResponse {
    pub items: Vec<FetchItem>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FetchItem {
    pub submission_id: String,
    pub studnum: String,
    pub file_name: String,
    pub accepted_at: String,
    pub mode: SubmissionMode,
    pub payload: SubmissionPayload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SubmissionPayload {
    Link { file_b64: String },
    E2e { envelope: Envelope },
}

#[derive(Clone, Debug, Serialize)]
pub struct SubmissionOverviewItem {
    pub submission_id: String,
    pub name: String,
    pub studnum: String,
    pub file_name: String,
    pub file_sha256: String,
    pub accepted_at: String,
    pub mode: SubmissionMode,
    pub status: SubmissionStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct SubmissionDetailResponse {
    pub submission_id: String,
    pub name: String,
    pub studnum: String,
    pub file_name: String,
    pub file_sha256: String,
    pub accepted_at: String,
    pub mode: SubmissionMode,
    pub status: SubmissionStatus,
    pub server_can_read_content: bool,
    pub inspection: Option<LinkInspectionRecord>,
    pub envelope: Option<Envelope>,
    pub retention: RetentionStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct RetentionStatus {
    pub retrieved_at: Option<String>,
    pub scheduled_delete_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: String,
    pub encrypted_challenge_b64: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerifyResponse {
    pub access_token: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TeacherActivityResponse {
    pub recent_challenges: Vec<TeacherChallengeView>,
    pub recent_tokens: Vec<TeacherTokenView>,
    pub recent_retrievals: Vec<RetrievalEventView>,
    pub retention_policy: RetentionPolicyView,
}

#[derive(Clone, Debug, Serialize)]
pub struct TeacherChallengeView {
    pub challenge_id: String,
    pub public_key_fingerprint: String,
    pub created_at: String,
    pub expires_at: String,
    pub used: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TeacherTokenView {
    pub issued_at: String,
    pub expires_at: String,
    pub bound_public_key_fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RetrievalEventView {
    pub submission_id: String,
    pub studnum: String,
    pub retrieved_at: String,
    pub scheduled_delete_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RetentionPolicyView {
    pub strategy: String,
    pub delete_delay_seconds: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupResponse {
    pub deleted_submission_ids: Vec<String>,
    pub deleted_count: usize,
}
