<script setup lang="ts">
import {
  NButton,
  NLayout,
  NLayoutContent,
  NLayoutHeader,
  NLayoutSider,
  NMenu,
  NTag,
} from 'naive-ui'
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { useAuthStore } from '@/stores/auth'

const route = useRoute()
const router = useRouter()
const authStore = useAuthStore()

const menuOptions = [
  { label: '提交总览', key: '/dashboard/overview' },
  { label: '教师活动', key: '/dashboard/activity' },
  { label: '维护操作', key: '/dashboard/maintenance' },
]

const selectedKey = computed(() => route.path)

function handleSelect(key: string) {
  router.push(key)
}

function handleLogout() {
  authStore.logout()
  router.push('/login')
}
</script>

<template>
  <NLayout has-sider class="min-h-[100dvh] bg-slate-100">
    <NLayoutSider
      bordered
      collapse-mode="width"
      :collapsed-width="84"
      :width="248"
      class="!bg-slate-950"
    >
      <div class="px-6 pt-7 pb-6">
        <span class="mb-2 inline-block text-[0.78rem] uppercase tracking-[0.16em] text-blue-200/80"
          >CipherSubmit</span
        >
        <strong class="block font-['Manrope'] text-2xl text-slate-50">Admin Console</strong>
        <p class="mt-2.5 leading-7 text-slate-300/70">更清楚地看见提交、活动和保留策略。</p>
      </div>

      <NMenu :options="menuOptions" :value="selectedKey" @update:value="handleSelect" />
    </NLayoutSider>

    <NLayout class="min-h-[100dvh] bg-slate-100">
      <NLayoutHeader
        bordered
        class="sticky top-0 z-10 flex min-h-[88px] flex-col items-start justify-between gap-5 border-b border-slate-200/80 bg-white/92 px-4 py-5 backdrop-blur md:flex-row md:items-center md:px-7"
      >
        <div>
          <p class="mb-2 inline-block text-[0.78rem] uppercase tracking-[0.16em] text-slate-500">
            Operations
          </p>
          <h1 class="m-0 font-['Manrope'] text-[1.75rem] text-slate-950">CipherSubmit 后台管理</h1>
        </div>

        <div class="flex items-center gap-3">
          <NTag type="info" round>
            {{ authStore.username || authStore.defaultUsername }}
          </NTag>
          <NButton tertiary type="primary" @click="handleLogout">退出登录</NButton>
        </div>
      </NLayoutHeader>

      <NLayoutContent class="bg-slate-100">
        <div class="min-h-[calc(100dvh-88px)] bg-slate-100 px-4 py-5 md:px-7">
          <RouterView />
        </div>
      </NLayoutContent>
    </NLayout>
  </NLayout>
</template>
