<script setup lang="ts">
import { NAlert, NButton, NCard, NEmpty, NList, NListItem, NTag } from 'naive-ui'
import { computed, onMounted, ref } from 'vue'

import StatCard from '@/components/StatCard.vue'
import { fetchTeacherActivity } from '@/services/admin'
import { normalizeApiError } from '@/services/http'
import type { TeacherActivityResponse } from '@/types/admin'

const loading = ref(false)
const errorMessage = ref('')
const activity = ref<TeacherActivityResponse | null>(null)

const metrics = computed(() => {
  const value = activity.value

  return [
    { title: '最近挑战', value: value?.recent_challenges.length ?? 0, hint: '教师挑战发放记录', tone: 'blue' as const },
    { title: '最近令牌', value: value?.recent_tokens.length ?? 0, hint: '教师认证后签发令牌', tone: 'green' as const },
    { title: '最近取件', value: value?.recent_retrievals.length ?? 0, hint: '下载与延迟删除记录', tone: 'amber' as const },
  ]
})

async function loadActivity() {
  loading.value = true
  errorMessage.value = ''

  try {
    activity.value = await fetchTeacherActivity()
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    loading.value = false
  }
}

onMounted(loadActivity)
</script>

<template>
  <section class="grid min-h-full gap-5">
    <header class="flex flex-col justify-between gap-5 xl:flex-row xl:items-start">
      <div>
        <p class="m-0 text-[0.8rem] uppercase tracking-[0.16em] text-blue-600">Activity</p>
        <h2 class="my-2 font-['Manrope'] text-[2rem] text-slate-950">教师活动</h2>
        <p class="m-0 leading-7 text-slate-600">把 challenge、token 和 retrieval 拆开看，方便定位当前认证与取件节奏。</p>
      </div>
      <NButton type="primary" :loading="loading" @click="loadActivity">刷新活动</NButton>
    </header>

    <section class="grid grid-cols-1 gap-4 xl:grid-cols-3">
      <StatCard
        v-for="metric in metrics"
        :key="metric.title"
        :title="metric.title"
        :value="metric.value"
        :hint="metric.hint"
        :tone="metric.tone"
      />
    </section>

    <NAlert v-if="activity" type="info" :show-icon="false">
      保留策略：{{ activity.retention_policy.strategy }}，删除延迟 {{ activity.retention_policy.delete_delay_seconds }} 秒。
    </NAlert>

    <NAlert v-if="errorMessage" type="error" :show-icon="false">
      {{ errorMessage }}
    </NAlert>

    <section class="grid grid-cols-1 gap-5 xl:grid-cols-2" v-if="activity">
      <NCard title="最近 Challenge">
        <NList bordered>
          <NListItem v-for="item in activity.recent_challenges" :key="item.challenge_id">
            <div class="list-row">
              <div>
                <strong>{{ item.challenge_id }}</strong>
                <p>{{ item.public_key_fingerprint }}</p>
              </div>
              <div class="list-meta">
                <NTag :type="item.used ? 'success' : 'warning'" :bordered="false">{{ item.used ? '已使用' : '未使用' }}</NTag>
                <span>{{ item.created_at }}</span>
              </div>
            </div>
          </NListItem>
        </NList>
      </NCard>

      <NCard title="最近 Token">
        <NList bordered>
          <NListItem v-for="item in activity.recent_tokens" :key="`${item.issued_at}-${item.bound_public_key_fingerprint}`">
            <div class="list-row">
              <div>
                <strong>{{ item.issued_at }}</strong>
                <p>{{ item.bound_public_key_fingerprint }}</p>
              </div>
              <div class="list-meta">
                <span>过期：{{ item.expires_at }}</span>
              </div>
            </div>
          </NListItem>
        </NList>
      </NCard>
    </section>

    <NCard v-if="activity" title="最近取件">
      <NEmpty v-if="activity.recent_retrievals.length === 0" description="暂时没有取件记录" />
      <NList v-else bordered>
        <NListItem v-for="item in activity.recent_retrievals" :key="`${item.submission_id}-${item.retrieved_at}`">
          <div class="list-row">
            <div>
              <strong>{{ item.submission_id }}</strong>
              <p>学号：{{ item.studnum }}</p>
            </div>
            <div class="list-meta">
              <span>取件：{{ item.retrieved_at }}</span>
              <span>删除：{{ item.scheduled_delete_at || '未安排' }}</span>
            </div>
          </div>
        </NListItem>
      </NList>
    </NCard>
  </section>
</template>

<style scoped>
.list-row p,
.list-row p {
  margin: 0;
  color: #475569;
  line-height: 1.7;
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.list-row strong {
  color: #0f172a;
}

.list-meta {
  display: grid;
  justify-items: end;
  gap: 8px;
  color: #64748b;
  font-size: 0.9rem;
}
</style>