<script setup lang="ts">
import {
  NAlert,
  NButton,
  NCard,
  NCode,
  NDataTable,
  NDescriptions,
  NDescriptionsItem,
  NDrawer,
  NDrawerContent,
  NInput,
  NList,
  NListItem,
  NSelect,
  NSpace,
  NSpin,
  NTag,
  useMessage,
  type DataTableColumns,
} from 'naive-ui'
import { computed, h, onMounted, ref } from 'vue'

import StatCard from '@/components/StatCard.vue'
import { downloadSubmissionPayload, fetchOverview, fetchSubmissionDetail } from '@/services/admin'
import { normalizeApiError } from '@/services/http'
import type { SubmissionDetailResponse, SubmissionOverviewItem } from '@/types/admin'

const message = useMessage()

const loading = ref(false)
const detailLoading = ref(false)
const downloadLoading = ref(false)
const errorMessage = ref('')
const detailOpen = ref(false)
const selectedDetail = ref<SubmissionDetailResponse | null>(null)
const studnumFilter = ref('')
const modeFilter = ref<'all' | 'link' | 'e2e'>('all')
const rows = ref<SubmissionOverviewItem[]>([])

const modeOptions = [
  { label: '全部模式', value: 'all' },
  { label: '链路模式', value: 'link' },
  { label: '端到端模式', value: 'e2e' },
]

const filteredRows = computed(() =>
  rows.value.filter((item) => {
    const byStudnum = !studnumFilter.value || item.studnum.includes(studnumFilter.value.trim())
    const byMode = modeFilter.value === 'all' || item.mode === modeFilter.value
    return byStudnum && byMode
  }),
)

const metrics = computed(() => {
  const total = rows.value.length
  const linkCount = rows.value.filter((item) => item.mode === 'link').length
  const e2eCount = rows.value.filter((item) => item.mode === 'e2e').length
  const scheduledDelete = rows.value.filter((item) => item.status === 'scheduled_delete').length

  return [
    { title: '总提交数', value: total, hint: '当前数据库中的有效记录', tone: 'blue' as const },
    { title: '链路模式', value: linkCount, hint: '服务端可读取 ZIP 内容', tone: 'green' as const },
    {
      title: '端到端模式',
      value: e2eCount,
      hint: '服务端仅存密文与元数据',
      tone: 'amber' as const,
    },
    { title: '待删除', value: scheduledDelete, hint: '已取件且等待清理', tone: 'slate' as const },
  ]
})

const columns: DataTableColumns<SubmissionOverviewItem> = [
  { title: '提交编号', key: 'submission_id', width: 180, ellipsis: { tooltip: true } },
  { title: '姓名', key: 'name', width: 120 },
  { title: '学号', key: 'studnum', width: 120 },
  { title: '文件名', key: 'file_name', minWidth: 180, ellipsis: { tooltip: true } },
  {
    title: '模式',
    key: 'mode',
    width: 120,
    render: (row) =>
      h(
        NTag,
        { type: row.mode === 'e2e' ? 'warning' : 'success', bordered: false, round: true },
        { default: () => row.mode.toUpperCase() },
      ),
  },
  {
    title: '状态',
    key: 'status',
    width: 150,
    render: (row) => h(NTag, { bordered: false, round: true }, { default: () => row.status }),
  },
  { title: '时间', key: 'accepted_at', width: 210 },
  {
    title: '操作',
    key: 'actions',
    width: 110,
    render: (row) =>
      h(
        NButton,
        { tertiary: true, type: 'primary', onClick: () => openDetail(row.submission_id) },
        { default: () => '查看' },
      ),
  },
]

async function loadOverview() {
  loading.value = true
  errorMessage.value = ''

  try {
    rows.value = await fetchOverview()
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    loading.value = false
  }
}

async function openDetail(submissionId: string) {
  detailOpen.value = true
  detailLoading.value = true
  selectedDetail.value = null

  try {
    selectedDetail.value = await fetchSubmissionDetail(submissionId)
  } catch (error) {
    message.error(normalizeApiError(error))
    detailOpen.value = false
  } finally {
    detailLoading.value = false
  }
}

function buildAdminDownloadName(detail: SubmissionDetailResponse) {
  if (detail.mode === 'link') {
    return detail.file_name
  }

  if (detail.file_name.endsWith('.bin')) {
    return detail.file_name
  }

  const lastDotIndex = detail.file_name.lastIndexOf('.')
  if (lastDotIndex > 0) {
    return `${detail.file_name.slice(0, lastDotIndex)}.bin`
  }

  return `${detail.file_name}.bin`
}

async function handleDownload() {
  if (!selectedDetail.value) {
    return
  }

  downloadLoading.value = true

  try {
    const blob = await downloadSubmissionPayload(selectedDetail.value.submission_id)
    const objectUrl = window.URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = objectUrl
    link.download = buildAdminDownloadName(selectedDetail.value)
    document.body.appendChild(link)
    link.click()
    link.remove()
    window.URL.revokeObjectURL(objectUrl)
    message.success('下载已开始，不会计入取件记录')
  } catch (error) {
    message.error(normalizeApiError(error))
  } finally {
    downloadLoading.value = false
  }
}

onMounted(loadOverview)
</script>

<template>
  <section class="grid min-h-full gap-5">
    <header class="flex flex-col justify-between gap-5 xl:flex-row xl:items-start">
      <div>
        <p class="m-0 text-[0.8rem] uppercase tracking-[0.16em] text-blue-600">Overview</p>
        <h2 class="my-2 font-['Manrope'] text-[2rem] text-slate-950">提交总览</h2>
        <p class="m-0 max-w-[70ch] leading-7 text-slate-600">
          列表、模式、状态和明密文边界放在一个入口里查看，减少页面的信息散射。
        </p>
      </div>
      <NButton type="primary" :loading="loading" @click="loadOverview">刷新数据</NButton>
    </header>

    <section class="grid grid-cols-1 gap-4 md:grid-cols-2 2xl:grid-cols-4">
      <StatCard
        v-for="metric in metrics"
        :key="metric.title"
        :title="metric.title"
        :value="metric.value"
        :hint="metric.hint"
        :tone="metric.tone"
      />
    </section>

    <NCard>
      <template #header>
        <div class="flex flex-col justify-between gap-4 xl:flex-row xl:items-center">
          <div>
            <strong class="block text-[1.05rem] text-slate-950">提交列表</strong>
            <span class="text-[0.92rem] text-slate-500">支持按学号和模式快速筛选</span>
          </div>
          <div class="flex w-full flex-col gap-3 xl:w-auto xl:min-w-[360px] xl:flex-row">
            <NInput v-model:value="studnumFilter" clearable placeholder="按学号筛选" />
            <NSelect v-model:value="modeFilter" :options="modeOptions" />
          </div>
        </div>
      </template>

      <NAlert v-if="errorMessage" type="error" :show-icon="false" class="mb-4">
        {{ errorMessage }}
      </NAlert>

      <NDataTable
        :columns="columns"
        :data="filteredRows"
        :loading="loading"
        :pagination="{ pageSize: 8 }"
        :bordered="false"
      />
    </NCard>

    <NDrawer v-model:show="detailOpen" width="720">
      <NDrawerContent title="提交详情" closable>
        <NSpin :show="detailLoading">
          <template v-if="selectedDetail">
            <div class="detail-stack">
              <div class="flex justify-end">
                <NButton
                  type="primary"
                  secondary
                  :loading="downloadLoading"
                  @click="handleDownload"
                >
                  直接下载
                </NButton>
              </div>

              <NCard size="small">
                <NDescriptions bordered label-placement="left" :column="1">
                  <NDescriptionsItem label="提交编号">{{
                    selectedDetail.submission_id
                  }}</NDescriptionsItem>
                  <NDescriptionsItem label="姓名">{{ selectedDetail.name }}</NDescriptionsItem>
                  <NDescriptionsItem label="学号">{{ selectedDetail.studnum }}</NDescriptionsItem>
                  <NDescriptionsItem label="文件名">{{
                    selectedDetail.file_name
                  }}</NDescriptionsItem>
                  <NDescriptionsItem label="模式">{{ selectedDetail.mode }}</NDescriptionsItem>
                  <NDescriptionsItem label="状态">{{ selectedDetail.status }}</NDescriptionsItem>
                  <NDescriptionsItem label="时间">{{
                    selectedDetail.accepted_at
                  }}</NDescriptionsItem>
                </NDescriptions>
              </NCard>

              <NCard size="small" title="可见性边界">
                <NAlert
                  :type="selectedDetail.server_can_read_content ? 'warning' : 'success'"
                  :show-icon="false"
                >
                  {{
                    selectedDetail.server_can_read_content
                      ? '当前为链路模式，服务端可读取 ZIP 内容。'
                      : '当前为端到端模式，服务端只可见 envelope 元数据。'
                  }}
                </NAlert>
              </NCard>

              <NCard v-if="selectedDetail.inspection" size="small" title="链路模式检查结果">
                <NSpace vertical>
                  <span
                    >发现 .git 痕迹：{{ selectedDetail.inspection.has_git_dir ? '是' : '否' }}</span
                  >
                  <span
                    >重复提交数：{{
                      selectedDetail.inspection.duplicate_submission_ids.length
                    }}</span
                  >
                  <NList bordered>
                    <NListItem
                      v-for="entry in selectedDetail.inspection.zip_entries_summary"
                      :key="entry"
                    >
                      {{ entry }}
                    </NListItem>
                  </NList>
                </NSpace>
              </NCard>

              <NCard v-if="selectedDetail.envelope" size="small" title="E2E Envelope 元数据">
                <NSpace vertical>
                  <div>
                    <div class="mb-2 text-[0.85rem] text-slate-600">encrypted_key_b64</div>
                    <NCode :code="selectedDetail.envelope.encrypted_key_b64" word-wrap />
                  </div>
                  <div>
                    <div class="mb-2 text-[0.85rem] text-slate-600">nonce_b64</div>
                    <NCode :code="selectedDetail.envelope.nonce_b64" word-wrap />
                  </div>
                </NSpace>
              </NCard>

              <NCard size="small" title="保留策略状态">
                <NDescriptions bordered :column="1">
                  <NDescriptionsItem label="retrieved_at">
                    {{ selectedDetail.retention.retrieved_at || '尚未取件' }}
                  </NDescriptionsItem>
                  <NDescriptionsItem label="scheduled_delete_at">
                    {{ selectedDetail.retention.scheduled_delete_at || '未安排删除' }}
                  </NDescriptionsItem>
                </NDescriptions>
              </NCard>
            </div>
          </template>
        </NSpin>
      </NDrawerContent>
    </NDrawer>
  </section>
</template>

<style scoped>
.detail-stack {
  display: grid;
  gap: 20px;
}
</style>
