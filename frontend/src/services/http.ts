import axios from 'axios'

const TOKEN_KEY = 'cisub.admin.token'
let isRedirectingForAuth = false

export const http = axios.create({
  baseURL: import.meta.env.VITE_API_BASE || '',
  headers: {
    Accept: 'application/json',
  },
})

http.interceptors.request.use((config) => {
  const token = getStoredToken()
  if (token) {
    config.headers = config.headers ?? {}
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

http.interceptors.response.use(
  (response) => response,
  (error) => {
    if (axios.isAxiosError(error) && error.response?.status === 401) {
      if (!isRedirectingForAuth && window.location.pathname !== '/login') {
        isRedirectingForAuth = true
        clearStoredToken()
        const redirect = `${window.location.pathname}${window.location.search}${window.location.hash}`
        const loginUrl = `/login?redirect=${encodeURIComponent(redirect)}`
        window.location.assign(loginUrl)
      } else {
        clearStoredToken()
      }
    }

    return Promise.reject(error)
  },
)

export function getStoredToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? ''
}

export function setStoredToken(token: string) {
  isRedirectingForAuth = false
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearStoredToken() {
  localStorage.removeItem(TOKEN_KEY)
}

export function getApiBaseLabel(): string {
  return import.meta.env.VITE_API_BASE || 'same-origin'
}

export function normalizeApiError(error: unknown): string {
  if (axios.isAxiosError(error)) {
    const bodyMessage = typeof error.response?.data === 'string'
      ? error.response.data
      : error.response?.data?.message

    return bodyMessage || error.message || '请求失败'
  }

  if (error instanceof Error) {
    return error.message
  }

  return '请求失败'
}