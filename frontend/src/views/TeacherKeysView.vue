<script setup lang="ts">
import {
    NAlert,
    NButton,
    NCard,
    NCode,
    NDataTable,
    NEmpty,
    NForm,
    NFormItem,
    NInput,
    NPopconfirm,
    NSpace,
    NTag,
    useMessage,
    type DataTableColumns,
} from 'naive-ui'
import { computed, h, onMounted, ref } from 'vue'

import StatCard from '@/components/StatCard.vue'
import {
    addAuthorizedTeacherKey,
    deleteAuthorizedTeacherKey,
    fetchAuthorizedTeacherKeys,
} from '@/services/admin'
import { normalizeApiError } from '@/services/http'
import type { AuthorizedTeacherKeyView } from '@/types/admin'

const message = useMessage()

const loading = ref(false)
const submitting = ref(false)
const deletingFingerprint = ref('')
const errorMessage = ref('')
const publicKeyPem = ref('')
const rows = ref<AuthorizedTeacherKeyView[]>([])

const metrics = computed(() => {
  const total = rows.value.length
  const rsaCount = rows.value.filter((item) => item.file_name.endsWith('.pem')).length
  const latest = rows.value[0]?.fingerprint.slice(0, 20) ?? '--'

  return [
    { title: '白名单数量', value: total, hint: '当前被授权的教师公钥', tone: 'blue' as const },
    { title: 'PEM 文件', value: rsaCount, hint: '落盘到服务端白名单目录', tone: 'green' as const },
    { title: '最近指纹', value: latest, hint: '按指纹排序后的首项预览', tone: 'amber' as const },
  ]
})

const columns: DataTableColumns<AuthorizedTeacherKeyView> = [
  {
    title: '指纹',
    key: 'fingerprint',
    minWidth: 320,
    render: (row) => h(NCode, { wordWrap: true }, { default: () => row.fingerprint }),
  },
  {
    title: '文件名',
    key: 'file_name',
    width: 260,
    ellipsis: { tooltip: true },
  },
  {
    title: '状态',
    key: 'status',
    width: 120,
    render: () => h(NTag, { type: 'success', bordered: false, round: true }, { default: () => '已授权' }),
  },
  {
    title: '操作',
    key: 'actions',
    width: 120,
    render: (row) =>
      h(
        NPopconfirm,
        {
          onPositiveClick: () => handleDelete(row.fingerprint),
        },
        {
          trigger: () =>
            h(
              NButton,
              {
                tertiary: true,
                type: 'error',
                loading: deletingFingerprint.value === row.fingerprint,
              },
              { default: () => '移除' },
            ),
          default: () => '确认从教师白名单中移除这把公钥？',
        },
      ),
  },
]

async function loadTeacherKeys() {
  loading.value = true
  errorMessage.value = ''

  try {
    rows.value = await fetchAuthorizedTeacherKeys()
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    loading.value = false
  }
}

async function handleAddKey() {
  const trimmed = publicKeyPem.value.trim()
  if (!trimmed) {
    errorMessage.value = '请先粘贴教师公钥 PEM。'
    return
  }

  submitting.value = true
  errorMessage.value = ''

  try {
    const item = await addAuthorizedTeacherKey(trimmed)
    publicKeyPem.value = ''
    message.success(`已加入白名单：${item.fingerprint}`)
    await loadTeacherKeys()
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    submitting.value = false
  }
}

async function handleDelete(fingerprint: string) {
  deletingFingerprint.value = fingerprint
  errorMessage.value = ''

  try {
    const removed = await deleteAuthorizedTeacherKey(fingerprint)
    message.success(`已移除白名单：${removed.fingerprint}`)
    await loadTeacherKeys()
  } catch (error) {
    errorMessage.value = normalizeApiError(error)
  } finally {
    deletingFingerprint.value = ''
  }
}

onMounted(loadTeacherKeys)
</script>

<template>
  <section class="grid min-h-full gap-5">
    <header class="flex flex-col justify-between gap-5 xl:flex-row xl:items-start">
      <div>
        <p class="m-0 text-[0.8rem] uppercase tracking-[0.16em] text-blue-600">Teacher Keys</p>
        <h2 class="my-2 font-['Manrope'] text-[2rem] text-slate-950">教师白名单</h2>
        <p class="m-0 max-w-[72ch] leading-7 text-slate-600">
          这里管理服务端允许发起教师 challenge 的 RSA 公钥。移出白名单后，这把公钥对应的教师 token 也会在后续请求里失效。
        </p>
      </div>
      <NButton type="primary" :loading="loading" @click="loadTeacherKeys">刷新列表</NButton>
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

    <NAlert type="info" :show-icon="false">
      只接受 PEM 公钥文本。服务端会基于公钥 DER 计算 SHA256 指纹，并把它保存到教师白名单目录中。
    </NAlert>

    <NAlert v-if="errorMessage" type="error" :show-icon="false">
      {{ errorMessage }}
    </NAlert>

    <section class="grid grid-cols-1 gap-5 2xl:grid-cols-[minmax(0,1.2fr)_420px]">
      <NCard title="已授权公钥">
        <NEmpty v-if="!loading && rows.length === 0" description="当前还没有授权教师公钥" />
        <NDataTable
          v-else
          :columns="columns"
          :data="rows"
          :loading="loading"
          :pagination="{ pageSize: 8 }"
          :bordered="false"
        />
      </NCard>

      <NCard title="添加教师公钥">
        <NForm @submit.prevent="handleAddKey">
          <NFormItem label="教师公钥 PEM">
            <NInput
              v-model:value="publicKeyPem"
              type="textarea"
              placeholder="粘贴教师 RSA 公钥 PEM，例如 -----BEGIN PUBLIC KEY-----"
              :autosize="{ minRows: 12, maxRows: 18 }"
            />
          </NFormItem>

          <NSpace vertical>
            <NButton type="primary" block :loading="submitting" @click="handleAddKey">
              加入白名单
            </NButton>
            <span class="text-sm leading-6 text-slate-500">
              同一把公钥重复加入会收到冲突错误；移除后再次加入会生成同一指纹的新文件。
            </span>
          </NSpace>
        </NForm>
      </NCard>
    </section>
  </section>
</template>