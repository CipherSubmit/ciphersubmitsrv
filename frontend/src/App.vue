<script setup>
import { computed, onMounted, ref } from 'vue'

const tabs = [
  { key: 'overview', label: '提交总览' },
  { key: 'link', label: '链路模式详情' },
  { key: 'e2e', label: '端到端详情' },
  { key: 'teacher', label: '教师活动' },
]

const activeTab = ref('overview')
const loading = ref(false)
const refreshing = ref(false)
const errorMessage = ref('')
const submissions = ref([])
const teacherActivity = ref({
  recent_challenges: [],
  recent_tokens: [],
  recent_retrievals: [],
  retention_policy: null,
})
const detailCache = ref(new Map())
const selectedSubmissionId = ref('')
const selectedDetail = ref(null)
const studnumFilter = ref('')
const modeFilter = ref('all')
const tokenInput = ref('')
const cleanupMessage = ref('')
const apiBase = ref('')

const filteredSubmissions = computed(() => {
  return submissions.value.filter((item) => {
    const matchesStudnum =
      !studnumFilter.value || item.studnum.toLowerCase().includes(studnumFilter.value.toLowerCase())
    const matchesMode = modeFilter.value === 'all' || item.mode === modeFilter.value
    return matchesStudnum && matchesMode
  })
})

const linkCandidates = computed(() =>
  filteredSubmissions.value.filter((item) => item.mode === 'link'),
)

const e2eCandidates = computed(() =>
  filteredSubmissions.value.filter((item) => item.mode === 'e2e'),
)

const metrics = computed(() => {
  const total = submissions.value.length
  const linkCount = submissions.value.filter((item) => item.mode === 'link').length
  const e2eCount = submissions.value.filter((item) => item.mode === 'e2e').length
  const scheduledDelete = submissions.value.filter((item) => item.status === 'scheduled_delete').length
  return [
    { label: '总提交数', value: total, accent: 'var(--accent-amber)' },
    { label: '链路模式', value: linkCount, accent: 'var(--accent-coral)' },
    { label: '端到端模式', value: e2eCount, accent: 'var(--accent-cyan)' },
    { label: '待删除', value: scheduledDelete, accent: 'var(--accent-olive)' },
  ]
})

const selectedVisibility = computed(() => {
  if (!selectedDetail.value) {
    return []
  }

  if (selectedDetail.value.mode === 'link') {
    return [
      '服务端可读取 ZIP 内容和目录条目。',
      '服务端可执行 .git 痕迹检查与重复哈希比对。',
      '教师取件时返回原始 ZIP 文件内容。',
    ]
  }

  return [
    '服务端仅可见学号、文件名、时间、哈希和 envelope 元数据。',
    '服务端不会尝试解密正文。',
    '教师取件时返回完整 envelope，由本地私钥解密。',
  ]
})

function resolveApi(path) {
  return `${apiBase.value || ''}${path}`
}

async function requestJson(path, options = {}) {
  const response = await fetch(resolveApi(path), {
    ...options,
    headers: {
      Accept: 'application/json',
      ...(options.headers ?? {}),
    },
  })

  if (!response.ok) {
    const text = await response.text()
    throw new Error(text || `请求失败: ${response.status}`)
  }

  return response.json()
}

async function loadDashboard() {
  loading.value = true
  errorMessage.value = ''
  cleanupMessage.value = ''

  try {
    const [overview, activity] = await Promise.all([
      requestJson('/api/v1/admin/overview'),
      requestJson('/api/v1/admin/auth/activity'),
    ])
    submissions.value = overview
    teacherActivity.value = activity

    if (!selectedSubmissionId.value && overview.length > 0) {
      selectedSubmissionId.value = overview[0].submission_id
      await loadSubmissionDetail(overview[0].submission_id)
    } else if (selectedSubmissionId.value) {
      await loadSubmissionDetail(selectedSubmissionId.value)
    }
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '加载面板数据失败'
  } finally {
    loading.value = false
  }
}

async function refreshDashboard() {
  refreshing.value = true
  await loadDashboard()
  refreshing.value = false
}

async function loadSubmissionDetail(submissionId) {
  selectedSubmissionId.value = submissionId

  if (detailCache.value.has(submissionId)) {
    selectedDetail.value = detailCache.value.get(submissionId)
    return
  }

  try {
    const detail = await requestJson(`/api/v1/admin/submissions/${submissionId}`)
    detailCache.value.set(submissionId, detail)
    selectedDetail.value = detail
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : '加载提交详情失败'
  }
}

async function runCleanup() {
  cleanupMessage.value = ''

  if (!tokenInput.value.trim()) {
    cleanupMessage.value = '请输入教师 Bearer Token 后再执行清理。'
    return
  }

  try {
    const response = await requestJson('/api/v1/admin/cleanup', {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${tokenInput.value.trim()}`,
      },
    })
    cleanupMessage.value = `本次清理 ${response.deleted_count} 条记录。`
    await refreshDashboard()
  } catch (error) {
    cleanupMessage.value = error instanceof Error ? error.message : '执行清理失败'
  }
}

function openTab(tabKey) {
  activeTab.value = tabKey

  const collection = tabKey === 'link' ? linkCandidates.value : tabKey === 'e2e' ? e2eCandidates.value : []
  if (collection.length > 0 && !collection.some((item) => item.submission_id === selectedSubmissionId.value)) {
    loadSubmissionDetail(collection[0].submission_id)
  }
}

onMounted(loadDashboard)
</script>

<template>
  <div class="shell">
    <div class="hero">
      <div>
        <p class="eyebrow">CipherSubmit Server Console</p>
        <h1>让服务端能看见的内容，一眼分清。</h1>
        <p class="hero-copy">
          面板直接基于服务端管理接口，重点展示链路模式与端到端模式的可见性差异、教师活动与保留策略执行状态。
        </p>
      </div>

      <div class="hero-actions">
        <label class="api-input">
          <span>API 前缀</span>
          <input v-model="apiBase" placeholder="开发环境留空，或填写 https://host:8443" />
        </label>
        <button class="primary-button" :disabled="refreshing" @click="refreshDashboard">
          {{ refreshing ? '刷新中...' : '刷新面板' }}
        </button>
      </div>
    </div>

    <div class="metrics-grid">
      <article v-for="metric in metrics" :key="metric.label" class="metric-card">
        <span class="metric-dot" :style="{ background: metric.accent }"></span>
        <p>{{ metric.label }}</p>
        <strong>{{ metric.value }}</strong>
      </article>
    </div>

    <section class="control-bar">
      <div class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="tab-button"
          :class="{ active: activeTab === tab.key }"
          @click="openTab(tab.key)"
        >
          {{ tab.label }}
        </button>
      </div>

      <div class="filters">
        <input v-model="studnumFilter" placeholder="按学号筛选" />
        <select v-model="modeFilter">
          <option value="all">全部模式</option>
          <option value="link">链路模式</option>
          <option value="e2e">端到端模式</option>
        </select>
      </div>
    </section>

    <p v-if="errorMessage" class="error-banner">{{ errorMessage }}</p>
    <p v-if="loading" class="loading-copy">正在从服务端读取最新状态...</p>

    <section v-if="activeTab === 'overview'" class="panel-grid panel-overview">
      <article class="panel card-xl">
        <header class="panel-header">
          <div>
            <p class="eyebrow">Page 1</p>
            <h2>提交总览</h2>
          </div>
          <span class="badge neutral">{{ filteredSubmissions.length }} 条</span>
        </header>

        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>提交编号</th>
                <th>姓名</th>
                <th>学号</th>
                <th>文件名</th>
                <th>时间</th>
                <th>模式</th>
                <th>哈希</th>
                <th>状态</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="item in filteredSubmissions"
                :key="item.submission_id"
                @click="loadSubmissionDetail(item.submission_id)"
              >
                <td>{{ item.submission_id }}</td>
                <td>{{ item.name }}</td>
                <td>{{ item.studnum }}</td>
                <td>{{ item.file_name }}</td>
                <td>{{ item.accepted_at }}</td>
                <td>
                  <span class="badge" :class="item.mode === 'link' ? 'link' : 'e2e'">
                    {{ item.mode }}
                  </span>
                </td>
                <td class="hash-cell">{{ item.file_sha256 }}</td>
                <td>{{ item.status }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </article>

      <article class="panel detail-card" v-if="selectedDetail">
        <header class="panel-header compact">
          <div>
            <p class="eyebrow">聚焦对象</p>
            <h2>{{ selectedDetail.file_name }}</h2>
          </div>
          <span class="badge" :class="selectedDetail.mode === 'link' ? 'link' : 'e2e'">
            {{ selectedDetail.mode }}
          </span>
        </header>

        <dl class="detail-list">
          <div>
            <dt>学号</dt>
            <dd>{{ selectedDetail.studnum }}</dd>
          </div>
          <div>
            <dt>提交编号</dt>
            <dd>{{ selectedDetail.submission_id }}</dd>
          </div>
          <div>
            <dt>服务端可读正文</dt>
            <dd>{{ selectedDetail.server_can_read_content ? '是' : '否' }}</dd>
          </div>
          <div>
            <dt>保留计划</dt>
            <dd>{{ selectedDetail.retention.scheduled_delete_at || '尚未排期' }}</dd>
          </div>
        </dl>

        <div class="visibility-box">
          <h3>当前服务端能看到什么</h3>
          <ul>
            <li v-for="item in selectedVisibility" :key="item">{{ item }}</li>
          </ul>
        </div>
      </article>
    </section>

    <section v-else-if="activeTab === 'link'" class="panel-grid panel-detail">
      <article class="panel list-panel">
        <header class="panel-header">
          <div>
            <p class="eyebrow">Page 2</p>
            <h2>链路模式详情</h2>
          </div>
          <span class="badge link">{{ linkCandidates.length }} 条</span>
        </header>

        <div class="stack-list">
          <button
            v-for="item in linkCandidates"
            :key="item.submission_id"
            class="list-item"
            :class="{ active: selectedSubmissionId === item.submission_id }"
            @click="loadSubmissionDetail(item.submission_id)"
          >
            <strong>{{ item.file_name }}</strong>
            <span>{{ item.studnum }} · {{ item.accepted_at }}</span>
          </button>
        </div>
      </article>

      <article class="panel detail-card" v-if="selectedDetail && selectedDetail.mode === 'link'">
        <header class="panel-header">
          <div>
            <p class="eyebrow">审查视图</p>
            <h2>{{ selectedDetail.file_name }}</h2>
          </div>
          <span class="badge link">可审查</span>
        </header>

        <dl class="detail-list">
          <div>
            <dt>ZIP 哈希</dt>
            <dd class="hash-cell">{{ selectedDetail.file_sha256 }}</dd>
          </div>
          <div>
            <dt>.git 痕迹</dt>
            <dd>{{ selectedDetail.inspection?.has_git_dir ? '命中' : '未命中' }}</dd>
          </div>
          <div>
            <dt>重复提交</dt>
            <dd>{{ selectedDetail.inspection?.duplicate_submission_ids?.length || 0 }} 个</dd>
          </div>
          <div>
            <dt>审查时间</dt>
            <dd>{{ selectedDetail.inspection?.inspected_at || '未生成' }}</dd>
          </div>
        </dl>

        <div class="chips-block">
          <h3>ZIP 内容摘要</h3>
          <div class="chips">
            <span v-for="entry in selectedDetail.inspection?.zip_entries_summary || []" :key="entry" class="chip">
              {{ entry }}
            </span>
          </div>
        </div>

        <div class="chips-block">
          <h3>重复对象列表</h3>
          <div class="chips">
            <span
              v-for="dup in selectedDetail.inspection?.duplicate_submission_ids || []"
              :key="dup"
              class="chip warning"
            >
              {{ dup }}
            </span>
          </div>
        </div>
      </article>
    </section>

    <section v-else-if="activeTab === 'e2e'" class="panel-grid panel-detail">
      <article class="panel list-panel">
        <header class="panel-header">
          <div>
            <p class="eyebrow">Page 3</p>
            <h2>端到端详情</h2>
          </div>
          <span class="badge e2e">{{ e2eCandidates.length }} 条</span>
        </header>

        <div class="stack-list">
          <button
            v-for="item in e2eCandidates"
            :key="item.submission_id"
            class="list-item"
            :class="{ active: selectedSubmissionId === item.submission_id }"
            @click="loadSubmissionDetail(item.submission_id)"
          >
            <strong>{{ item.file_name }}</strong>
            <span>{{ item.studnum }} · {{ item.accepted_at }}</span>
          </button>
        </div>
      </article>

      <article class="panel detail-card" v-if="selectedDetail && selectedDetail.mode === 'e2e'">
        <header class="panel-header">
          <div>
            <p class="eyebrow">密文视图</p>
            <h2>{{ selectedDetail.file_name }}</h2>
          </div>
          <span class="badge e2e">正文不可见</span>
        </header>

        <dl class="detail-list">
          <div>
            <dt>学号</dt>
            <dd>{{ selectedDetail.studnum }}</dd>
          </div>
          <div>
            <dt>时间</dt>
            <dd>{{ selectedDetail.accepted_at }}</dd>
          </div>
          <div>
            <dt>哈希</dt>
            <dd class="hash-cell">{{ selectedDetail.file_sha256 }}</dd>
          </div>
          <div>
            <dt>服务端说明</dt>
            <dd>服务端不可解密，只保存 envelope 和元数据。</dd>
          </div>
        </dl>

        <div class="envelope-grid">
          <article>
            <h3>encrypted_key_b64</h3>
            <p>{{ selectedDetail.envelope?.encrypted_key_b64 }}</p>
          </article>
          <article>
            <h3>nonce_b64</h3>
            <p>{{ selectedDetail.envelope?.nonce_b64 }}</p>
          </article>
          <article>
            <h3>ciphertext_b64</h3>
            <p>{{ selectedDetail.envelope?.ciphertext_b64 }}</p>
          </article>
        </div>
      </article>
    </section>

    <section v-else class="panel-grid teacher-grid">
      <article class="panel card-xl">
        <header class="panel-header">
          <div>
            <p class="eyebrow">Page 4</p>
            <h2>教师认证与取件状态</h2>
          </div>
          <span class="badge neutral">最近活动</span>
        </header>

        <div class="teacher-columns">
          <div>
            <h3>最近挑战记录</h3>
            <div class="activity-list">
              <article v-for="item in teacherActivity.recent_challenges" :key="item.challenge_id" class="activity-card">
                <strong>{{ item.challenge_id }}</strong>
                <p>{{ item.public_key_fingerprint }}</p>
                <span>{{ item.created_at }} → {{ item.expires_at }}</span>
                <span class="mini-badge" :class="item.used ? 'used' : 'fresh'">{{ item.used ? '已使用' : '待验证' }}</span>
              </article>
            </div>
          </div>

          <div>
            <h3>Token 发放记录</h3>
            <div class="activity-list">
              <article v-for="item in teacherActivity.recent_tokens" :key="`${item.issued_at}-${item.bound_public_key_fingerprint}`" class="activity-card">
                <strong>{{ item.issued_at }}</strong>
                <p>{{ item.bound_public_key_fingerprint }}</p>
                <span>到期时间：{{ item.expires_at }}</span>
              </article>
            </div>
          </div>
        </div>
      </article>

      <article class="panel detail-card">
        <header class="panel-header compact">
          <div>
            <p class="eyebrow">保留策略</p>
            <h2>{{ teacherActivity.retention_policy?.strategy || 'delayed_delete' }}</h2>
          </div>
          <span class="badge neutral">
            {{ teacherActivity.retention_policy?.delete_delay_seconds || 0 }} 秒
          </span>
        </header>

        <div class="activity-list retrieval-list">
          <article
            v-for="item in teacherActivity.recent_retrievals"
            :key="`${item.submission_id}-${item.retrieved_at}`"
            class="activity-card"
          >
            <strong>{{ item.submission_id }}</strong>
            <p>学号：{{ item.studnum }}</p>
            <span>取件时间：{{ item.retrieved_at }}</span>
            <span>计划删除：{{ item.scheduled_delete_at || '未安排' }}</span>
          </article>
        </div>

        <div class="cleanup-box">
          <label>
            <span>教师 Bearer Token</span>
            <input v-model="tokenInput" placeholder="用于触发 /api/v1/admin/cleanup" />
          </label>
          <button class="secondary-button" @click="runCleanup">执行过期清理</button>
          <p v-if="cleanupMessage" class="helper-copy">{{ cleanupMessage }}</p>
        </div>
      </article>
    </section>
  </div>
</template>