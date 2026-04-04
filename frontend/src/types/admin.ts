export type SubmissionMode = 'link' | 'e2e'
export type SubmissionStatus =
  | 'accepted'
  | 'inspected'
  | 'ciphertext_only'
  | 'scheduled_delete'
  | 'deleted'

export interface SubmissionOverviewItem {
  submission_id: string
  name: string
  studnum: string
  file_name: string
  file_sha256: string
  accepted_at: string
  mode: SubmissionMode
  status: SubmissionStatus
}

export interface LinkInspectionRecord {
  submission_id: string
  has_git_dir: boolean
  zip_entries_summary: string[]
  duplicate_sha256: string | null
  duplicate_submission_ids: string[]
  inspected_at: string
}

export interface Envelope {
  encrypted_key_b64: string
  nonce_b64: string
}

export interface SubmissionDetailResponse {
  submission_id: string
  name: string
  studnum: string
  file_name: string
  file_sha256: string
  accepted_at: string
  mode: SubmissionMode
  status: SubmissionStatus
  server_can_read_content: boolean
  inspection: LinkInspectionRecord | null
  envelope: Envelope | null
  retention: {
    retrieved_at: string | null
    scheduled_delete_at: string | null
  }
}

export interface TeacherChallengeView {
  challenge_id: string
  public_key_fingerprint: string
  created_at: string
  expires_at: string
  used: boolean
}

export interface TeacherTokenView {
  issued_at: string
  expires_at: string
  bound_public_key_fingerprint: string
}

export interface RetrievalEventView {
  submission_id: string
  studnum: string
  retrieved_at: string
  scheduled_delete_at: string | null
}

export interface TeacherActivityResponse {
  recent_challenges: TeacherChallengeView[]
  recent_tokens: TeacherTokenView[]
  recent_retrievals: RetrievalEventView[]
  retention_policy: {
    strategy: string
    delete_delay_seconds: number
  }
}

export interface AdminLoginResponse {
  access_token: string
  expires_at: string
}

export interface CleanupResponse {
  deleted_submission_ids: string[]
  deleted_count: number
}

export interface AuthorizedTeacherKeyView {
  fingerprint: string
  file_name: string
}
