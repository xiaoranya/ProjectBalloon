<template>
  <el-container direction="vertical" class="page-section printing-page">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <p class="eyebrow">Printing</p>
          <h1>赛中打印</h1>
        </div>
        <div class="clarification-live-state" :class="{ connected: realtimeConnected }" aria-live="polite">
          <span />{{ realtimeConnected ? '实时更新' : '轮询更新' }}
        </div>
      </div>
    </el-header>

    <el-main class="page-body">
      <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

      <ElRow :gutter="22" align="top" class="printing-team-grid">
        <ElCol :xs="24" :md="9">
          <ElCard shadow="never" class="print-compose-card">
            <template #header>
              <div class="card-header">
                <div><strong>新建打印请求</strong><small>最多 20 KiB、5 页；仅支持纯文本。</small></div>
              </div>
            </template>
            <ElForm label-position="top" @submit.prevent="submitPrintRequest">
              <ElFormItem label="打印内容" :error="validation.error ?? undefined">
                <ElInput
                  v-model="content"
                  type="textarea"
                  :rows="14"
                  resize="vertical"
                  placeholder="粘贴需要打印的纯文本内容"
                />
              </ElFormItem>
              <div class="printing-estimate" aria-live="polite">
                <span>{{ formatBytes(validation.bytes) }} / 20 KiB</span>
                <span>预计 {{ validation.pageCount }} / 5 页</span>
              </div>
              <ElButton type="primary" native-type="submit" class="wide-button" :loading="submitting" :disabled="validation.error !== null">
                提交打印
              </ElButton>
            </ElForm>
          </ElCard>
        </ElCol>
        <ElCol :xs="24" :md="15">
          <div class="clarification-list-column">
            <div class="clarification-list-heading">
              <div><h2>我的打印</h2><p>打印内容不会在历史记录中返回。</p></div>
              <ElButton :icon="Refresh" :loading="loading" @click="loadRequests(false)">刷新</ElButton>
            </div>
            <ElSkeleton v-if="loading && requests.length === 0" :rows="5" animated />
            <ElEmpty v-else-if="requests.length === 0" description="本队尚未提交打印请求" />
            <div v-else class="clarification-cards">
              <article v-for="item in requests" :key="item.id" class="clarification-card">
                <div class="clarification-card-meta">
                  <div>
                    <ElTag :type="printStatusType(item.status)">{{ printStatusLabel(item.status) }}</ElTag>
                    <ElTag type="info" effect="plain">{{ item.pageCount }} 页</ElTag>
                  </div>
                  <time>{{ formatDateTime(item.createdAt) }}</time>
                </div>
                <h3>打印请求 #{{ item.id }}</h3>
                <p v-if="item.failedReason" class="printing-failure">{{ item.failedReason }}</p>
                <small>内容校验值：{{ item.contentHash }}</small>
                <div class="clarification-actions print-actions">
                  <ElButton
                    v-if="isPdfReady(item)"
                    size="small"
                    :icon="Download"
                    :loading="downloadingId === item.id"
                    @click="downloadPdf(item)"
                  >
                    下载 PDF
                  </ElButton>
                  <span v-else class="muted-text">PDF 正在生成</span>
                </div>
              </article>
            </div>
          </div>
        </ElCol>
      </ElRow>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import { ElMessage } from 'element-plus';
import { Download, Refresh } from '@element-plus/icons-vue';
import { getErrorMessage } from '../api/client';
import {
  printingApi,
  validatePrintContent,
  type PrintRequestResponse,
  type PrintRequestStatus,
} from '../api/printing';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatBytes, formatDateTime } from '../utils/format';

const route = useRoute();
const contestId = computed(() => Number(route.params.contestId));
const content = ref('');
const requests = ref<PrintRequestResponse[]>([]);
const loading = ref(false);
const submitting = ref(false);
const downloadingId = ref<number | null>(null);
const errorMessage = ref('');
const realtimeConnected = ref(false);
let realtime: ContestRealtimeSubscription | undefined;
let loadGeneration = 0;

const validation = computed(() => validatePrintContent(content.value));

function printStatusLabel(status: PrintRequestStatus) {
  return {
    REQUESTED: '已申请', QUEUED: '排队中', PRINTING: '打印中', COMPLETED: '已完成',
    FAILED: '打印失败', CANCELLED: '已取消', REJECTED: '已拒绝',
  }[status];
}

function printStatusType(status: PrintRequestStatus): 'success' | 'danger' | 'warning' | 'info' | 'primary' {
  if (status === 'COMPLETED') return 'success';
  if (status === 'FAILED' || status === 'REJECTED') return 'danger';
  if (status === 'REQUESTED' || status === 'QUEUED' || status === 'PRINTING') return 'warning';
  return 'info';
}

function isPdfReady(item: PrintRequestResponse) { return item.status !== 'REQUESTED'; }

async function loadRequests(silent = true) {
  const activeContestId = contestId.value;
  const generation = ++loadGeneration;
  if (!silent) loading.value = true;
  try {
    const loaded = await printingApi.listMine(activeContestId);
    if (generation !== loadGeneration || activeContestId !== contestId.value) return;
    requests.value = loaded;
    errorMessage.value = '';
  } catch (error) {
    if (generation === loadGeneration && !silent) errorMessage.value = getErrorMessage(error);
  } finally {
    if (generation === loadGeneration && !silent) loading.value = false;
  }
}

async function submitPrintRequest() {
  const checked = validation.value;
  if (checked.error) return;
  submitting.value = true;
  try {
    const activeContestId = contestId.value;
    const created = await printingApi.create(activeContestId, checked.content);
    if (activeContestId !== contestId.value) return;
    loadGeneration += 1;
    loading.value = false;
    requests.value = [created, ...requests.value.filter((item) => item.id !== created.id)];
    content.value = '';
    errorMessage.value = '';
    ElMessage.success('打印请求已提交');
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    submitting.value = false;
  }
}

async function downloadPdf(item: PrintRequestResponse) {
  downloadingId.value = item.id;
  let url: string | null = null;
  let anchor: HTMLAnchorElement | null = null;
  try {
    const blob = await printingApi.pdf(item.id);
    url = URL.createObjectURL(blob);
    anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `print-request-${item.id}.pdf`;
    document.body.append(anchor);
    anchor.click();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    anchor?.remove();
    if (url) window.setTimeout(() => URL.revokeObjectURL(url!), 0);
    downloadingId.value = null;
  }
}

async function activateContest() {
  loadGeneration += 1;
  realtime?.stop();
  realtime = undefined;
  realtimeConnected.value = false;
  requests.value = [];
  errorMessage.value = '';
  const activeContestId = contestId.value;
  if (!Number.isSafeInteger(activeContestId) || activeContestId <= 0) {
    errorMessage.value = '比赛 ID 不正确';
    return;
  }
  await loadRequests(false);
  if (activeContestId !== contestId.value) return;
  realtime = subscribeContestEvents({
    contestId: activeContestId,
    scope: 'TEAM',
    eventTypes: ['PRINT_REQUEST_UPDATED'],
    onEvent: () => void loadRequests(),
    onConnectionChange: (connected) => { realtimeConnected.value = connected; },
    poll: () => loadRequests(),
  });
}

watch(() => route.params.contestId, () => { void activateContest(); }, { immediate: true });

onUnmounted(() => realtime?.stop());
</script>

<style scoped>
.page-section {
  min-height: 50vh;
}

.page-head {
  height: auto;
  padding: 0;
}

.page-body {
  padding: 0;
}

.page-title-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 32px;
}

.page-title-row h1 {
  margin-bottom: 8px;
  font-size: clamp(32px, 4vw, 48px);
  letter-spacing: -0.035em;
}

.page-title-row p {
  display: none;
  margin-bottom: 0;
  color: var(--muted);
}

.eyebrow {
  display: none;
  margin: 0 0 8px;
  color: var(--primary);
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.page-alert,
.form-alert {
  margin-bottom: 20px;
}

.clarification-live-state {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 14px;
  border-radius: 0;
  padding: 8px 12px;
  color: var(--muted);
  background: #e9eef5;
  font-size: 12px;
}

.clarification-live-state span {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #94a3b8;
}

.clarification-live-state.connected {
  color: #166534;
  background: #dcfce7;
}

.clarification-live-state.connected span {
  background: #22c55e;
}

.wide-button {
  width: 100%;
}

.print-compose-card {
  border-radius: 0;
}

.print-compose-card :deep(.el-textarea__inner) {
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  line-height: 1.55;
}

.clarification-list-column,
.clarification-cards {
  display: grid;
  gap: 16px;
}

.clarification-list-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.clarification-list-heading h2,
.clarification-list-heading p {
  margin-bottom: 4px;
}

.clarification-list-heading p {
  color: var(--muted);
  font-size: 12px;
}

.clarification-card {
  padding: 22px;
  border: 1px solid var(--border);
  border-radius: 0;
  background: white;
}

.clarification-card-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}

.clarification-card-meta > div {
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
}

.clarification-card-meta time {
  color: var(--muted);
  font-size: 12px;
}

.clarification-card h3 {
  margin: 18px 0;
  color: #1f2c42;
  font-size: 16px;
  font-weight: 650;
  line-height: 1.7;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.clarification-card small {
  color: var(--muted);
  font-size: 12px;
}

.clarification-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-top: 30px;
  padding-top: 22px;
  border-top: 1px solid #e5eaf2;
}

.print-actions {
  flex-wrap: wrap;
}

.muted-text {
  color: var(--muted);
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.card-header > div {
  min-width: 0;
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
