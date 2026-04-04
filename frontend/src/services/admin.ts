import { http } from '@/services/http'
import type {
    AdminLoginResponse,
    CleanupResponse,
    SubmissionDetailResponse,
    SubmissionOverviewItem,
    TeacherActivityResponse,
} from '@/types/admin'

export async function loginAdmin(username: string, password: string) {
  const { data } = await http.post<AdminLoginResponse>('/api/v1/admin/auth/login', {
    username,
    password,
  })
  return data
}

export async function fetchOverview() {
  const { data } = await http.get<SubmissionOverviewItem[]>('/api/v1/admin/overview')
  return data
}

export async function fetchSubmissionDetail(submissionId: string) {
  const { data } = await http.get<SubmissionDetailResponse>(`/api/v1/admin/submissions/${submissionId}`)
  return data
}

export async function downloadSubmissionPayload(submissionId: string) {
  const { data } = await http.get<Blob>(`/api/v1/admin/submissions/download/${submissionId}`, {
    responseType: 'blob',
  })
  return data
}

export async function fetchTeacherActivity() {
  const { data } = await http.get<TeacherActivityResponse>('/api/v1/admin/auth/activity')
  return data
}

export async function runCleanup() {
  const { data } = await http.post<CleanupResponse>('/api/v1/admin/cleanup')
  return data
}