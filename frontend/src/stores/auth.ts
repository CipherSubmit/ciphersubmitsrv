import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { loginAdmin } from '@/services/admin'
import { clearStoredToken, getStoredToken, setStoredToken } from '@/services/http'

const defaultUsername = import.meta.env.VITE_DEFAULT_ADMIN_USERNAME || 'admin'
const defaultPassword = import.meta.env.VITE_DEFAULT_ADMIN_PASSWORD || 'admin123'

export const useAuthStore = defineStore('auth', () => {
  const token = ref(getStoredToken())
  const username = ref(defaultUsername)
  const password = ref(defaultPassword)
  const expiresAt = ref('')

  const isAuthenticated = computed(() => Boolean(token.value))

  async function login() {
    const response = await loginAdmin(username.value, password.value)
    token.value = response.access_token
    expiresAt.value = response.expires_at
    setStoredToken(response.access_token)
  }

  function logout() {
    token.value = ''
    expiresAt.value = ''
    clearStoredToken()
  }

  function restore() {
    token.value = getStoredToken()
  }

  return {
    defaultUsername,
    defaultPassword,
    expiresAt,
    isAuthenticated,
    login,
    logout,
    password,
    restore,
    token,
    username,
  }
})