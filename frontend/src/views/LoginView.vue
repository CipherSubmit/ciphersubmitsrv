<script setup lang="ts">
import { NAlert, NButton, NCard, NForm, NFormItem, NInput, useMessage } from 'naive-ui'
import { computed, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { normalizeApiError } from '@/services/http'
import { useAuthStore } from '@/stores/auth'

const authStore = useAuthStore()
const route = useRoute()
const router = useRouter()
const message = useMessage()

const loading = ref(false)
const errorMessage = ref('')

const redirectPath = computed(() => {
  const raw = route.query.redirect
  return typeof raw === 'string' && raw.startsWith('/') ? raw : '/dashboard/overview'
})

authStore.restore()

async function handleLogin() {
  loading.value = true
  errorMessage.value = ''

  try {
    await authStore.login()
    message.success('登录成功')
    await router.push(redirectPath.value)
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="grid min-h-[100dvh] gap-10 bg-slate-100 px-6 py-8 lg:grid-cols-[minmax(0,1.15fr)_minmax(360px,440px)] lg:items-center lg:px-12">
    <section class="max-w-[640px]">
      <p class="mb-2.5 inline-block text-[0.82rem] uppercase tracking-[0.18em] text-blue-600">Vue Naive Admin Style</p>
      <h1 class="m-0 font-['Manrope'] text-[clamp(2.8rem,5vw,4.6rem)] leading-[0.96] text-slate-950">更简洁的 CipherSubmit 管理入口。</h1>
      <p class="mt-[18px] max-w-[56ch] text-[1.02rem] leading-8 text-slate-600">
        这里默认面向开发和演示环境，启动后可直接使用预填的管理员账号密码登录；生产环境请覆盖服务端环境变量。
      </p>
    </section>

    <NCard class="rounded-[28px] shadow-[0_24px_60px_rgba(15,23,42,0.12)]" :bordered="false">
      <template #header>
        <div class="grid gap-1.5">
          <span class="inline-block text-[0.82rem] uppercase tracking-[0.18em] text-blue-600">Admin Login</span>
          <strong class="font-['Manrope'] text-[1.8rem] text-slate-950">登录后台控制台</strong>
        </div>
      </template>

      <NAlert type="info" :show-icon="false" class="mb-4">
        默认预填账号用于开发/演示：{{ authStore.defaultUsername }} / {{ authStore.defaultPassword }}
      </NAlert>

      <NAlert v-if="errorMessage" type="error" :show-icon="false" class="mb-4">
        {{ errorMessage }}
      </NAlert>

      <NForm @submit.prevent="handleLogin">
        <NFormItem label="管理员账号">
          <NInput v-model:value="authStore.username" placeholder="请输入管理员账号" />
        </NFormItem>
        <NFormItem label="管理员密码">
          <NInput
            v-model:value="authStore.password"
            type="password"
            show-password-on="click"
            placeholder="请输入管理员密码"
          />
        </NFormItem>

        <NButton type="primary" block size="large" :loading="loading" @click="handleLogin">
          进入控制台
        </NButton>
      </NForm>
    </NCard>
  </main>
</template>