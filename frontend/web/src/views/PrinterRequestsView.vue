<template>
  <section class="judge-page printer-page">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Print Request Desk</p>
        <h1>打印请求</h1>
        <p>按比赛和状态处理最近 1000 条打印任务。</p>
      </div>
      <div class="clarification-live-state" :class="{ connected: realtimeConnected }" aria-live="polite">
        <span />{{ realtimeConnected ? '实时更新' : '轮询更新' }}
      </div>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />
    <ElAlert
      v-if="healthLoaded"
      :title="cupsHealthText"
      :type="cupsHealthType"
      show-icon
      :closable="false"
      class="page-alert"
    />

    <ElCard shadow="never" class="clarification-filter-card">
      <div class="clarification-toolbar printer-toolbar">
        <ElSelect v-model="selectedContestId" filterable placeholder="选择比赛" @change="changeContest">
          <ElOption v-for="contest in contests" :key="contest.id" :label="contest.name" :value="contest.id" />
        </ElSelect>
        <ElSelect v-model="statusFilter" placeholder="状态" @change="loadRequests(false)">
          <ElOption label="全部状态" value="ALL" />
          <ElOption v-for="status in printRequestStatuses" :key="status" :label="printStatusLabel(status)" :value="status" />
        </ElSelect>
        <ElButton :icon="Refresh" :loading="loading" :disabled="!selectedContestId" @click="refreshRequests">刷新</ElButton>
      </div>
    </ElCard>

    <ElCard shadow="never" class="clarification-list-card">
      <ElTable v-loading="loading" :data="requests" row-key="id" :empty-text="queueEmptyText" @row-click="openDetail">
        <ElTableColumn prop="id" label="#" width="76" />
        <ElTableColumn label="状态" width="110">
          <template #default="{ row }"><ElTag :type="printStatusType(row.status)">{{ printStatusLabel(row.status) }}</ElTag></template>
        </ElTableColumn>
        <ElTableColumn label="队伍" min-width="190">
          <template #default="{ row }">
            <div class="admin-primary-cell"><strong>{{ row.teamName ?? `队伍 #${row.teamId}` }}</strong><small>座位 {{ row.seatNo ?? '—' }}</small></div>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="pageCount" label="页数" width="80" />
        <ElTableColumn label="打印信息" min-width="180">
          <template #default="{ row }"><span>{{ row.printerId ?? '—' }}</span><small v-if="row.cupsJobId"> · CUPS {{ row.cupsJobId }}</small></template>
        </ElTableColumn>
        <ElTableColumn label="申请时间" min-width="170"><template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template></ElTableColumn>
        <ElTableColumn label="操作" width="110" fixed="right">
          <template #default="{ row }"><ElButton link type="primary" @click.stop="openDetail(row as PrintRequestResponse)">查看详情</ElButton></template>
        </ElTableColumn>
      </ElTable>
    </ElCard>

    <ElDrawer v-model="detailVisible" title="打印请求详情" size="min(620px, 94vw)" @closed="selected = null">
      <div v-if="selected" class="print-detail">
        <div class="clarification-card-meta">
          <div><ElTag :type="printStatusType(selected.status)">{{ printStatusLabel(selected.status) }}</ElTag><ElTag type="info" effect="plain">{{ selected.pageCount }} 页</ElTag></div>
          <span>请求 #{{ selected.id }}</span>
        </div>
        <div class="admin-primary-cell"><strong>{{ selected.teamName ?? `队伍 #${selected.teamId}` }}</strong><small>座位 {{ selected.seatNo ?? '—' }} · 队伍 ID {{ selected.teamId }}</small></div>
        <ElDescriptions :column="1" border>
          <ElDescriptionsItem label="申请时间">{{ formatDateTime(selected.createdAt) }}</ElDescriptionsItem>
          <ElDescriptionsItem label="完成时间">{{ formatDateTime(selected.completedAt) }}</ElDescriptionsItem>
          <ElDescriptionsItem label="打印机">{{ selected.printerId ?? '—' }}</ElDescriptionsItem>
          <ElDescriptionsItem label="CUPS 任务">{{ selected.cupsJobId ?? '—' }}</ElDescriptionsItem>
          <ElDescriptionsItem label="内容校验值">{{ selected.contentHash }}</ElDescriptionsItem>
          <ElDescriptionsItem v-if="selected.failedReason" label="失败/拒绝原因"><span class="danger-text">{{ selected.failedReason }}</span></ElDescriptionsItem>
        </ElDescriptions>
        <div class="clarification-actions print-actions">
          <ElButton v-if="isPdfReady(selected)" :loading="downloading" @click="downloadPdf(selected)">下载 PDF</ElButton>
          <span v-else class="muted-text">PDF 正在生成</span>
          <ElButton v-if="canRetry(selected.status)" type="primary" plain :loading="mutating" @click="retryRequest(selected)">重试</ElButton>
          <ElButton v-if="canCancel(selected.status)" type="warning" plain :loading="mutating" @click="cancelRequest(selected)">取消</ElButton>
          <ElButton v-if="canReject(selected.status)" type="danger" plain @click="openReject(selected)">拒绝</ElButton>
        </div>
      </div>
    </ElDrawer>

    <ElDialog v-model="rejectVisible" title="拒绝打印请求" width="min(520px, 92vw)">
      <ElForm label-position="top">
        <ElFormItem label="拒绝原因" :error="rejectError || undefined">
          <ElInput v-model="rejectReason" type="textarea" :rows="4" />
          <small>{{ rejectReasonLength(rejectReason) }} / 255 个字符</small>
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="rejectVisible = false">取消</ElButton>
        <ElButton type="danger" :loading="mutating" :disabled="!!rejectError" @click="rejectRequest">确认拒绝</ElButton>
      </template>
    </ElDialog>
  </section>
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
import type { Contest, HealthResponse } from '../api/types';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';

const route = useRoute();
const router = useRouter();
const contests = ref<Contest[]>([]);
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
  if (length === 0) return '请输入拒绝原因';
  return length > 255 ? '拒绝原因不能超过 255 个字符' : '';
});
const cupsHealthText = computed(() => health.value?.cups
  ? `CUPS 连接${health.value.cups.status === 'up' ? '正常' : '不可用'}`
  : 'CUPS 未配置或连接状态未知');
const cupsHealthType = computed<'success' | 'warning'>(() => health.value?.cups?.status === 'up' ? 'success' : 'warning');
const queueEmptyText = computed(() => queueLoaded.value ? '当前筛选下没有打印请求' : '打印队列加载失败，请重试');

function printStatusLabel(status: PrintRequestStatus) {
  return { REQUESTED: '已申请', QUEUED: '排队中', PRINTING: '打印中', COMPLETED: '已完成', FAILED: '打印失败', CANCELLED: '已取消', REJECTED: '已拒绝' }[status];
}
function printStatusType(status: PrintRequestStatus): 'success' | 'danger' | 'warning' | 'info' | 'primary' {
  if (status === 'COMPLETED') return 'success';
  if (status === 'FAILED' || status === 'REJECTED') return 'danger';
  if (status === 'REQUESTED' || status === 'QUEUED' || status === 'PRINTING') return 'warning';
  return 'info';
}
function canRetry(status: PrintRequestStatus) { return status === 'FAILED' || status === 'QUEUED'; }
function canCancel(status: PrintRequestStatus) { return !['COMPLETED', 'CANCELLED', 'REJECTED'].includes(status); }
function canReject(status: PrintRequestStatus) { return status === 'REQUESTED' || status === 'QUEUED'; }
function isPdfReady(item: PrintRequestResponse) { return item.status !== 'REQUESTED'; }

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
    const loaded = await printingApi.listAll(contestId, statusFilter.value === 'ALL' ? undefined : statusFilter.value);
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
    if (generation === loadGeneration && contestId === selectedContestId.value && (!silent || surfaceError)) {
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
    onConnectionChange: (connected) => { realtimeConnected.value = connected; },
    poll: async () => { await loadRequests(); },
  });
}

async function changeContest() {
  loadGeneration += 1;
  requests.value = [];
  queueLoaded.value = false;
  selected.value = null;
  detailVisible.value = false;
  errorMessage.value = '';
  await router.replace({ query: selectedContestId.value ? { contestId: String(selectedContestId.value) } : {} });
  connectRealtime();
  await loadRequests(false);
}
function openDetail(row: PrintRequestResponse) { selected.value = row; detailVisible.value = true; }
async function refreshAfterMutation(updated: PrintRequestResponse, message: string) {
  loadGeneration += 1;
  selected.value = updated;
  requests.value = [updated, ...requests.value.filter((item) => item.id !== updated.id)];
  const refreshed = await loadRequests(true, true);
  if (refreshed) ElMessage.success(message);
  else ElMessage.warning(`${message}，但刷新队列失败，请手动重试`);
}
async function retryRequest(item: PrintRequestResponse) {
  mutating.value = true;
  try { await refreshAfterMutation(await printingApi.retry(item.id), '打印请求已重试'); }
  catch (error) { ElMessage.error(getErrorMessage(error)); }
  finally { mutating.value = false; }
}
async function cancelRequest(item: PrintRequestResponse) {
  try { await ElMessageBox.confirm('确定取消该打印请求吗？', '取消打印', { type: 'warning' }); } catch { return; }
  mutating.value = true;
  try { await refreshAfterMutation(await printingApi.cancel(item.id), '打印请求已取消'); }
  catch (error) { ElMessage.error(getErrorMessage(error)); }
  finally { mutating.value = false; }
}
function openReject(item: PrintRequestResponse) { rejectTargetId.value = item.id; rejectReason.value = ''; rejectVisible.value = true; }
async function rejectRequest() {
  if (!rejectTargetId.value || rejectError.value) return;
  mutating.value = true;
  try {
    const updated = await printingApi.reject(rejectTargetId.value, normalizeRejectReason(rejectReason.value));
    rejectVisible.value = false;
    await refreshAfterMutation(updated, '打印请求已拒绝');
  } catch (error) { ElMessage.error(getErrorMessage(error)); }
  finally { mutating.value = false; }
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
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContestId) ? queryContestId : (contests.value[0]?.id ?? null);
    if (selectedContestId.value) { connectRealtime(); await loadRequests(false); }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally { loading.value = false; }
});

onUnmounted(() => realtime?.stop());
</script>
