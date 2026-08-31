<template>
  <el-container direction="vertical" class="judge-page printer-page">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <h1>{{ t('打印请求') }}</h1>
        </div>
        <div
          class="clarification-live-state"
          :class="{ connected: realtimeConnected }"
          aria-live="polite"
        >
          <span />{{ realtimeConnected ? t('实时更新') : t('轮询更新') }}
        </div>
      </div>
    </el-header>

    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />
      <ElAlert
        v-if="healthLoaded"
        :title="cupsHealthText"
        :type="cupsHealthType"
        show-icon
        :closable="false"
        class="page-alert"
      />

      <ElCard shadow="never" class="clarification-filter-card">
        <ElSpace wrap :size="16" class="printer-toolbar">
          <ElSelect
            v-model="selectedContestId"
            filterable
            :placeholder="t('选择比赛')"
            @change="changeContest"
          >
            <ElOption
              v-for="contest in contests"
              :key="contest.id"
              :label="contest.name"
              :value="contest.id"
            />
          </ElSelect>
          <ElSelect v-model="statusFilter" :placeholder="t('状态')" @change="loadRequests(false)">
            <ElOption :label="t('全部状态')" value="ALL" />
            <ElOption
              v-for="status in printRequestStatuses"
              :key="status"
              :label="printStatusLabel(status)"
              :value="status"
            />
          </ElSelect>
          <ElButton
            :icon="Refresh"
            :loading="loading"
            :disabled="!selectedContestId"
            @click="refreshRequests"
            >{{ t('刷新') }}</ElButton
          >
        </ElSpace>
      </ElCard>

      <ElCard shadow="never" class="clarification-list-card">
        <ElTable
          v-loading="loading"
          :data="requests"
          row-key="id"
          :empty-text="queueEmptyText"
          @row-click="openDetail"
        >
          <ElTableColumn prop="id" label="#" width="76" />
          <ElTableColumn :label="t('状态')" width="110">
            <template #default="{ row }"
              ><ElTag :type="printStatusType(row.status)">{{
                printStatusLabel(row.status)
              }}</ElTag></template
            >
          </ElTableColumn>
          <ElTableColumn :label="t('队伍')" min-width="190">
            <template #default="{ row }">
              <div class="admin-primary-cell">
                <strong>{{ row.teamName ?? t('队伍 #{id}', { id: row.teamId }) }}</strong
                ><small>{{ t('座位 {seat}', { seat: row.seatNo ?? '—' }) }}</small>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn prop="pageCount" :label="t('页数')" width="80" />
          <ElTableColumn :label="t('打印信息')" min-width="180">
            <template #default="{ row }"
              ><span>{{ row.printerId ?? '—' }}</span
              ><small v-if="row.cupsJobId"> · CUPS {{ row.cupsJobId }}</small></template
            >
          </ElTableColumn>
          <ElTableColumn :label="t('申请时间')" min-width="170"
            ><template #default="{ row }">{{
              formatDateTime(row.createdAt)
            }}</template></ElTableColumn
          >
          <ElTableColumn :label="t('操作')" width="110" fixed="right">
            <template #default="{ row }"
              ><ElButton
                link
                type="primary"
                @click.stop="openDetail(row as PrintRequestResponse)"
                >{{ t('查看详情') }}</ElButton
              ></template
            >
          </ElTableColumn>
        </ElTable>
      </ElCard>

      <ElDrawer
        v-model="detailVisible"
        :title="t('打印请求详情')"
        size="min(620px, 94vw)"
        @closed="selected = null"
      >
        <div v-if="selected" class="print-detail">
          <div class="clarification-card-meta">
            <div>
              <ElTag :type="printStatusType(selected.status)">{{
                printStatusLabel(selected.status)
              }}</ElTag
              ><ElTag type="info" effect="plain">{{
                t('{pages} 页', { pages: selected.pageCount })
              }}</ElTag>
            </div>
            <span>{{ t('请求 #{id}', { id: selected.id }) }}</span>
          </div>
          <div class="admin-primary-cell">
            <strong>{{ selected.teamName ?? t('队伍 #{id}', { id: selected.teamId }) }}</strong
            ><small>{{
              t('座位 {seat} · 队伍 ID {id}', { seat: selected.seatNo ?? '—', id: selected.teamId })
            }}</small>
          </div>
          <ElDescriptions :column="1" border>
            <ElDescriptionsItem :label="t('申请时间')">{{
              formatDateTime(selected.createdAt)
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('完成时间')">{{
              formatDateTime(selected.completedAt)
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('打印机')">{{
              selected.printerId ?? '—'
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('CUPS 任务')">{{
              selected.cupsJobId ?? '—'
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('内容校验值')">{{
              selected.contentHash
            }}</ElDescriptionsItem>
            <ElDescriptionsItem v-if="selected.failedReason" :label="t('失败/拒绝原因')"
              ><span class="danger-text">{{ selected.failedReason }}</span></ElDescriptionsItem
            >
          </ElDescriptions>
          <div class="clarification-actions print-actions">
            <ElButton
              v-if="isPdfReady(selected)"
              :loading="downloading"
              @click="downloadPdf(selected)"
              >{{ t('下载 PDF') }}</ElButton
            >
            <span v-else class="muted-text">{{ t('PDF 正在生成') }}</span>
            <ElButton
              v-if="canRetry(selected.status)"
              type="primary"
              plain
              :loading="mutating"
              @click="retryRequest(selected)"
              >{{ t('重试') }}</ElButton
            >
            <ElButton
              v-if="canCancel(selected.status)"
              type="warning"
              plain
              :loading="mutating"
              @click="cancelRequest(selected)"
              >{{ t('取消') }}</ElButton
            >
            <ElButton
              v-if="canReject(selected.status)"
              type="danger"
              plain
              @click="openReject(selected)"
              >{{ t('拒绝') }}</ElButton
            >
          </div>
        </div>
      </ElDrawer>

      <ElDialog v-model="rejectVisible" :title="t('拒绝打印请求')" width="min(520px, 92vw)">
        <ElForm label-position="top">
          <ElFormItem :label="t('拒绝原因')" :error="rejectError || undefined">
            <ElInput v-model="rejectReason" type="textarea" :rows="4" />
            <small>{{
              t('{count} / 255 个字符', { count: rejectReasonLength(rejectReason) })
            }}</small>
          </ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="rejectVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="danger"
            :loading="mutating"
            :disabled="!!rejectError"
            @click="rejectRequest"
            >{{ t('确认拒绝') }}</ElButton
          >
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import {
  normalizeRejectReason,
  printingApi,
  printRequestStatuses,
  rejectReasonLength,
  type PrintRequestResponse,
  type PrintRequestStatus,
} from '../api/printing';
import type { ContestResponse, HealthResponse } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contests = ref<ContestResponse[]>([]);
const selectedContestId = ref<number | null>(null);
const statusFilter = ref<PrintRequestStatus | 'ALL'>('ALL');
const requests = ref<PrintRequestResponse[]>([]);
const selected = ref<PrintRequestResponse | null>(null);
const health = ref<HealthResponse | null>(null);
const healthLoaded = ref(false);
const loading = ref(false);
const mutating = ref(false);
const downloading = ref(false);
const detailVisible = ref(false);
const rejectVisible = ref(false);
const rejectReason = ref('');
const rejectTargetId = ref<number | null>(null);
const errorMessage = ref('');
const realtimeConnected = ref(false);
const queueLoaded = ref(false);
let realtime: ContestRealtimeSubscription | undefined;
let loadGeneration = 0;

const rejectError = computed(() => {
  const length = rejectReasonLength(rejectReason.value);
  if (length === 0) return t('请输入拒绝原因');
  return length > 255 ? t('拒绝原因不能超过 255 个字符') : '';
});
const cupsHealthText = computed(() =>
  health.value?.cups
    ? t(health.value.cups.status === 'up' ? 'CUPS 连接正常' : 'CUPS 连接不可用')
    : t('CUPS 未配置或连接状态未知'),
);
const cupsHealthType = computed<'success' | 'warning'>(() =>
  health.value?.cups?.status === 'up' ? 'success' : 'warning',
);
const queueEmptyText = computed(() =>
  queueLoaded.value ? t('当前筛选下没有打印请求') : t('打印队列加载失败，请重试'),
);

function printStatusLabel(status: PrintRequestStatus) {
  return t(
    {
      REQUESTED: '已申请',
      QUEUED: '排队中',
      PRINTING: '打印中',
      COMPLETED: '已完成',
      FAILED: '打印失败',
      CANCELLED: '已取消',
      REJECTED: '已拒绝',
    }[status],
  );
}
function printStatusType(
  status: PrintRequestStatus,
): 'success' | 'danger' | 'warning' | 'info' | 'primary' {
  if (status === 'COMPLETED') return 'success';
  if (status === 'FAILED' || status === 'REJECTED') return 'danger';
  if (status === 'REQUESTED' || status === 'QUEUED' || status === 'PRINTING') return 'warning';
  return 'info';
}
function canRetry(status: PrintRequestStatus) {
  return status === 'FAILED' || status === 'QUEUED';
}
function canCancel(status: PrintRequestStatus) {
  return !['COMPLETED', 'CANCELLED', 'REJECTED'].includes(status);
}
function canReject(status: PrintRequestStatus) {
  return status === 'REQUESTED' || status === 'QUEUED';
}
function isPdfReady(item: PrintRequestResponse) {
  return item.status !== 'REQUESTED';
}

async function loadHealth() {
  try {
    health.value = await adminApi.getHealth();
  } catch {
    health.value = null;
  } finally {
    healthLoaded.value = true;
  }
}

async function refreshRequests() {
  await Promise.all([loadRequests(false), loadHealth()]);
}

async function loadRequests(silent = true, surfaceError = false): Promise<boolean> {
  if (!selectedContestId.value) return false;
  const contestId = selectedContestId.value;
  const generation = ++loadGeneration;
  if (!silent) loading.value = true;
  try {
    const loaded = await printingApi.listAll(
      contestId,
      statusFilter.value === 'ALL' ? undefined : statusFilter.value,
    );
    if (generation !== loadGeneration || contestId !== selectedContestId.value) return false;
    requests.value = loaded;
    queueLoaded.value = true;
    if (selected.value) {
      const changed = loaded.find((item) => item.id === selected.value?.id);
      selected.value = changed ?? null;
      if (!changed) detailVisible.value = false;
    }
    errorMessage.value = '';
    return true;
  } catch (error) {
    if (
      generation === loadGeneration &&
      contestId === selectedContestId.value &&
      (!silent || surfaceError)
    ) {
      errorMessage.value = getErrorMessage(error);
    }
    return false;
  } finally {
    if (generation === loadGeneration && !silent) loading.value = false;
  }
}

function connectRealtime() {
  realtime?.stop();
  realtimeConnected.value = false;
  if (!selectedContestId.value) return;
  realtime = subscribeContestEvents({
    contestId: selectedContestId.value,
    scope: 'STAFF',
    eventTypes: ['PRINT_REQUEST_UPDATED'],
    onEvent: () => void loadRequests(),
    onConnectionChange: (connected) => {
      realtimeConnected.value = connected;
    },
    poll: async () => {
      await loadRequests();
    },
  });
}

async function changeContest() {
  loadGeneration += 1;
  requests.value = [];
  queueLoaded.value = false;
  selected.value = null;
  detailVisible.value = false;
  errorMessage.value = '';
  await router.replace({
    query: selectedContestId.value ? { contestId: String(selectedContestId.value) } : {},
  });
  connectRealtime();
  await loadRequests(false);
}
function openDetail(row: PrintRequestResponse) {
  selected.value = row;
  detailVisible.value = true;
}
async function refreshAfterMutation(updated: PrintRequestResponse, message: string) {
  loadGeneration += 1;
  selected.value = updated;
  requests.value = [updated, ...requests.value.filter((item) => item.id !== updated.id)];
  const refreshed = await loadRequests(true, true);
  if (refreshed) ElMessage.success(message);
  else ElMessage.warning(t('{message}，但刷新队列失败，请手动重试', { message }));
}
async function retryRequest(item: PrintRequestResponse) {
  mutating.value = true;
  try {
    await refreshAfterMutation(await printingApi.retry(item.id), t('打印请求已重试'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function cancelRequest(item: PrintRequestResponse) {
  try {
    await ElMessageBox.confirm(t('确定取消该打印请求吗？'), t('取消打印'), { type: 'warning' });
  } catch {
    return;
  }
  mutating.value = true;
  try {
    await refreshAfterMutation(await printingApi.cancel(item.id), t('打印请求已取消'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
function openReject(item: PrintRequestResponse) {
  rejectTargetId.value = item.id;
  rejectReason.value = '';
  rejectVisible.value = true;
}
async function rejectRequest() {
  if (!rejectTargetId.value || rejectError.value) return;
  mutating.value = true;
  try {
    const updated = await printingApi.reject(
      rejectTargetId.value,
      normalizeRejectReason(rejectReason.value),
    );
    rejectVisible.value = false;
    await refreshAfterMutation(updated, t('打印请求已拒绝'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutating.value = false;
  }
}
async function downloadPdf(item: PrintRequestResponse) {
  downloading.value = true;
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
    downloading.value = false;
  }
}

onMounted(async () => {
  loading.value = true;
  try {
    const page = await contestApi.listContests();
    contests.value = page.content;
    await loadHealth();
    const queryContestId = Number(route.query.contestId);
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContestId)
      ? queryContestId
      : (contests.value[0]?.id ?? null);
    if (selectedContestId.value) {
      connectRealtime();
      await loadRequests(false);
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => realtime?.stop());
</script>

<style scoped>
.judge-page {
  width: min(1440px, 100%);
  margin: 0 auto;
}

.page-head {
  padding: clamp(28px, 5vw, 58px) clamp(28px, 5vw, 58px) 0;
}

.page-body {
  padding: 0 clamp(28px, 5vw, 58px) clamp(28px, 5vw, 58px);
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


.clarification-filter-card {
  margin-bottom: 22px;
}

.clarification-list-card {
  overflow: hidden;
  border: 1px solid var(--border);
}

.printer-toolbar {
  display: flex;
  width: 100%;
}

.printer-toolbar :deep(.el-space__item) {
  display: flex;
  align-items: center;
}

.printer-toolbar :deep(.el-space__item:first-child) {
  flex: 0 0 min(420px, 100%);
}

.printer-toolbar :deep(.el-space__item:nth-child(2)) {
  flex: 0 0 170px;
}

.printer-toolbar :deep(.el-space__item .el-select) {
  width: 100%;
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

.clarification-card-meta > span {
  color: var(--muted);
  font-size: 12px;
}

.print-detail {
  display: grid;
  gap: 22px;
}

.print-actions {
  flex-wrap: wrap;
}

.muted-text {
  color: var(--muted);
}

.danger-text {
  color: #dc2626;
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }

  .printer-toolbar :deep(.el-space__item) {
    flex: 1 1 100% !important;
  }
}
</style>
