<script setup lang="ts">
import { NAlert, NButton, NCard, NSpace, NSpin, useMessage } from 'naive-ui'
import { onMounted, ref } from 'vue'

import { fetchTeacherActivity, runCleanup } from '@/services/admin'
import { getApiBaseLabel, normalizeApiError } from '@/services/http'
import { useAuthStore } from '@/stores/auth'

const authStore = useAuthStore()
const message = useMessage()

const loading = ref(false)
const cleanupLoading = ref(false)
const errorMessage = ref('')
const cleanupMessage = ref('')
const deleteDelaySeconds = ref<number | null>(null)

async function loadPolicy() {
  loading.value = true
  errorMessage.value = ''

  try {
    const activity = await fetchTeacherActivity()
    deleteDelaySeconds.value = activity.retention_policy.delete_delay_seconds
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    loading.value = false
  }
}

async function handleCleanup() {
  cleanupLoading.value = true
  cleanupMessage.value = ''

  try {
    const result = await runCleanup()
    cleanupMessage.value = `本次清理 ${result.deleted_count} 条记录。`
    message.success('清理完成')
    await loadPolicy()
  } catch (error) {
    cleanupMessage.value = normalizeApiError(error)
  } finally {
    cleanupLoading.value = false
  }
}

onMounted(loadPolicy)
</script>

<template>
  <section class="grid min-h-full gap-5">
    <header class="page-header">
      <div>
        <p class="m-0 text-[0.8rem] uppercase tracking-[0.16em] text-blue-600">Maintenance</p>
        <h2 class="my-2 font-['Manrope'] text-[2rem] text-slate-950">维护与清理</h2>
        <p class="m-0 leading-7 text-slate-600">这里把演示环境默认登录信息、当前 API 连接位置和延迟删除策略集中展示。</p>
      </div>
    </header>

    <section class="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <NCard title="当前登录态">
        <NSpace vertical>
          <span>管理员账号：{{ authStore.username || authStore.defaultUsername }}</span>
          <span>认证令牌：{{ authStore.isAuthenticated ? '已写入本地存储' : '未登录' }}</span>
          <span>API Base：{{ getApiBaseLabel() }}</span>
        </NSpace>
      </NCard>

      <NCard title="演示环境提示">
        <NAlert type="warning" :show-icon="false">
          当前前端默认预填 admin / admin123。生产环境请通过服务端环境变量覆盖管理员账号密码。
        </NAlert>
      </NCard>
    </section>

    <NCard title="延迟删除策略">
      <NSpin :show="loading">
        <NSpace vertical>
          <span>当前删除延迟：{{ deleteDelaySeconds ?? '--' }} 秒</span>
          <span>说明：文件被教师取件后，服务端会进入延迟删除状态，等待清理任务移除过期负载。</span>
        </NSpace>
      </NSpin>
    </NCard>

    <NCard title="执行清理">
      <NSpace vertical>
        <span>该操作会删除已经超过保留期的提交负载，并将数据库状态更新为 deleted。</span>
        <NButton type="primary" :loading="cleanupLoading" @click="handleCleanup">立即清理</NButton>
        <NAlert v-if="cleanupMessage" type="success" :show-icon="false">{{ cleanupMessage }}</NAlert>
        <NAlert v-if="errorMessage" type="error" :show-icon="false">{{ errorMessage }}</NAlert>
      </NSpace>
    </NCard>
  </section>
</template>
