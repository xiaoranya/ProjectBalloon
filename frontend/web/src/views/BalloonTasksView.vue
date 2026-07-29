<template>
  <section class="judge-page">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Balloon Delivery Desk</p>
        <h1>气球配送工作台</h1>
        <p>领取首次 AC 任务，记录配送备注并确认送达。</p>
      </div>
      <div class="clarification-live-state" :class="{ connected: realtimeConnected }" aria-live="polite">
        <span />{{ realtimeConnected ? '实时更新' : '轮询更新' }}
      </div>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <div class="balloon-stats-grid">
      <div class="balloon-stat"><span>全部任务</span><strong>{{ stats?.total ?? 0 }}</strong></div>
      <div class="balloon-stat pending"><span>待领取</span><strong>{{ stats?.pending ?? 0 }}</strong></div>
      <div class="balloon-stat active"><span>配送中</span><strong>{{ stats?.claimed ?? 0 }}</strong></div>
      <div class="balloon-stat delivered"><span>已送达</span><strong>{{ stats?.delivered ?? 0 }}</strong></div>
      <div class="balloon-stat first-blood"><span>First Blood</span><strong>{{ stats?.firstBlood ?? 0 }}</strong></div>
    </div>

    <ElCard shadow="never" class="clarification-filter-card">
      <div class="balloon-toolbar">
        <ElSelect v-model="selectedContestId" filterable placeholder="选择比赛" @change="changeContest">
          <ElOption v-for="contest in contests" :key="contest.id" :label="contest.name" :value="contest.id" />
        </ElSelect>
        <ElSelect v-model="statusFilter" placeholder="状态" @change="changeFilter">
          <ElOption label="全部状态" value="ALL" />
          <ElOption v-for="status in balloonTaskStatuses" :key="status" :label="statusLabel(status)" :value="status" />
        </ElSelect>
        <ElSelect v-model="problemFilter" clearable placeholder="全部题目">
          <ElOption v-for="problem in problemOptions" :key="problem" :label="`题目 ${problem}`" :value="problem" />
        </ElSelect>
        <ElInput v-model="keyword" clearable placeholder="搜索队伍或座位号" :prefix-icon="Search" />
        <ElButton type="primary" :loading="action === 'dispatch'" :disabled="!selectedContestId" @click="dispatchTasks">智能领取</ElButton>
        <ElButton :icon="Refresh" :loading="loading" :disabled="!selectedContestId" @click="loadData(false)">刷新</ElButton>
      </div>
    </ElCard>

    <ElCard shadow="never">
      <ElTable v-loading="loading" :data="filteredTasks" row-key="id" :empty-text="loaded ? '当前筛选下暂无气球任务' : '气球任务加载失败，请重试'" @row-click="openDetail">
        <ElTableColumn label="队伍与座位" min-width="220">
          <template #default="{ row }"><div class="admin-primary-cell"><strong>{{ row.teamName }}</strong><small>座位 {{ row.seatNo ?? '—' }} · 任务 #{{ row.id }}</small></div></template>
        </ElTableColumn>
        <ElTableColumn label="题目与颜色" width="180">
          <template #default="{ row }"><div class="balloon-problem-cell"><span class="balloon-color" :style="{ backgroundColor: row.color }" /><div><strong>{{ row.problemAlias }}</strong><small>{{ row.color }}</small></div></div></template>
        </ElTableColumn>
        <ElTableColumn label="标记" width="130">
          <template #default="{ row }"><ElTag v-if="row.isFirstBlood" type="danger" effect="dark">First Blood</ElTag><span v-else class="muted-text">普通任务</span></template>
        </ElTableColumn>
        <ElTableColumn label="状态" width="110">
          <template #default="{ row }"><ElTag :type="statusType(row.status)">{{ statusLabel(row.status) }}</ElTag></template>
        </ElTableColumn>
        <ElTableColumn label="领取信息" min-width="180">
          <template #default="{ row }"><div class="admin-primary-cell"><span>{{ row.claimedByUserId ? `工作人员 #${row.claimedByUserId}` : '尚未领取' }}</span><small>{{ formatDateTime(row.claimedAt) }}</small></div></template>
        </ElTableColumn>
        <ElTableColumn label="生成时间" min-width="170"><template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template></ElTableColumn>
      </ElTable>
    </ElCard>

    <ElDrawer v-model="detailVisible" title="气球任务详情" size="min(650px, 95vw)" @closed="selected = null">
      <div v-if="selected" class="balloon-detail">
        <div class="announcement-detail-tags">
          <ElTag :type="statusType(selected.status)">{{ statusLabel(selected.status) }}</ElTag>
          <ElTag v-if="selected.isFirstBlood" type="danger" effect="dark">First Blood</ElTag>
          <ElTag effect="plain"><span class="balloon-color small" :style="{ backgroundColor: selected.color }" /> {{ selected.problemAlias }} · {{ selected.color }}</ElTag>
        </div>
        <ElDescriptions :column="1" border>
          <ElDescriptionsItem label="队伍">{{ selected.teamName }}</ElDescriptionsItem>
          <ElDescriptionsItem label="座位号">{{ selected.seatNo ?? '—' }}</ElDescriptionsItem>
          <ElDescriptionsItem label="关联提交">#{{ selected.submissionId }}</ElDescriptionsItem>
          <ElDescriptionsItem label="领取时间">{{ formatDateTime(selected.claimedAt) }}</ElDescriptionsItem>
          <ElDescriptionsItem label="送达时间">{{ formatDateTime(selected.deliveredAt) }}</ElDescriptionsItem>
          <ElDescriptionsItem v-if="selected.cancelledReason" label="取消原因"><span class="danger-text">{{ selected.cancelledReason }}</span></ElDescriptionsItem>
          <ElDescriptionsItem v-if="selected.reopenedCount" label="重新打开">{{ selected.reopenedCount }} 次</ElDescriptionsItem>
        </ElDescriptions>
        <div class="balloon-note-panel">
          <div><strong>配送备注</strong><small>入口、楼层、座位异常或交接信息，最多 1000 字</small></div>
          <ElInput v-model="note" type="textarea" :rows="4" maxlength="1000" show-word-limit />
          <ElButton :loading="action === 'note'" @click="saveNote">保存备注</ElButton>
        </div>
        <div class="clarification-actions balloon-actions">
          <ElButton v-if="selected.status === 'PENDING'" type="primary" :loading="action === 'claim'" @click="claimTask">领取任务</ElButton>
          <ElButton v-if="canDeliver" type="success" :loading="action === 'deliver'" @click="deliverTask">标记已送达</ElButton>
          <ElButton v-if="selected.status === 'CANCELLED'" type="warning" :loading="action === 'reopen'" @click="reopenTask">重新打开</ElButton>
          <ElButton v-if="selected.status === 'PENDING' || selected.status === 'CLAIMED'" type="danger" plain @click="openCancel">取消任务</ElButton>
        </div>
      </div>
    </ElDrawer>

    <ElDialog v-model="cancelVisible" title="取消气球任务" width="min(540px, 92vw)">
      <ElForm label-position="top"><ElFormItem label="取消原因" :error="cancelError || undefined"><ElInput v-model="cancelReason" type="textarea" :rows="4" maxlength="255" show-word-limit /></ElFormItem></ElForm>
      <template #footer><ElButton @click="cancelVisible = false">返回</ElButton><ElButton type="danger" :loading="action === 'cancel'" :disabled="!!cancelError" @click="cancelTask">确认取消</ElButton></template>
    </ElDialog>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { ElMessage, ElMessageBox, ElNotification } from 'element-plus';
import { Refresh, Search } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { useSession } from '../auth/session';
import { balloonApi, balloonTaskStatuses, type BalloonStats, type BalloonTask, type BalloonTaskStatus } from '../api/balloons';
import { getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';

const route = useRoute();
const router = useRouter();
const session = useSession();
const contests = ref<Contest[]>([]);
const selectedContestId = ref<number | null>(null);
const statusFilter = ref<BalloonTaskStatus | 'ALL'>('ALL');
const problemFilter = ref('');
const keyword = ref('');
const tasks = ref<BalloonTask[]>([]);
const stats = ref<BalloonStats | null>(null);
const selected = ref<BalloonTask | null>(null);
const note = ref('');
const cancelReason = ref('');
const loading = ref(false);
const loaded = ref(false);
const action = ref<'claim' | 'deliver' | 'cancel' | 'reopen' | 'note' | 'dispatch' | ''>('');
const errorMessage = ref('');
const detailVisible = ref(false);
const cancelVisible = ref(false);
const realtimeConnected = ref(false);
let realtime: ContestRealtimeSubscription | undefined;
let generation = 0;

const problemOptions = computed(() => [...new Set(tasks.value.map((task) => task.problemAlias))].sort());
const filteredTasks = computed(() => {
  const search = keyword.value.trim().toLocaleLowerCase();
  return tasks.value.filter((task) => (!problemFilter.value || task.problemAlias === problemFilter.value)
    && (!search || task.teamName.toLocaleLowerCase().includes(search) || (task.seatNo ?? '').toLocaleLowerCase().includes(search)));
});
const canDeliver = computed(() => selected.value?.status === 'CLAIMED'
  && (session.isSuperAdmin.value || selected.value.claimedByUserId === session.state.user?.id));
const cancelError = computed(() => cancelReason.value.trim() ? '' : '请输入取消原因');

function statusLabel(status: BalloonTaskStatus) { return { PENDING: '待领取', CLAIMED: '配送中', DELIVERED: '已送达', CANCELLED: '已取消' }[status]; }
function statusType(status: BalloonTaskStatus): 'success' | 'warning' | 'info' | 'danger' { return status === 'DELIVERED' ? 'success' : status === 'CANCELLED' ? 'danger' : 'warning'; }

async function loadData(showLoading = true): Promise<boolean> {
  if (!selectedContestId.value) return false;
  const contestId = selectedContestId.value;
  const requestGeneration = ++generation;
  if (showLoading) loading.value = true;
  try {
    const previous = new Set(tasks.value.map((task) => task.id));
    const [nextTasks, nextStats] = await Promise.all([
      balloonApi.list(contestId, statusFilter.value === 'ALL' ? undefined : statusFilter.value),
      balloonApi.stats(contestId),
    ]);
    if (requestGeneration !== generation || contestId !== selectedContestId.value) return false;
    if (!showLoading && nextTasks.some((task) => task.status === 'PENDING' && !previous.has(task.id))) {
      ElNotification.warning({ title: '收到新气球任务', message: '有新的气球等待领取。' });
    }
    tasks.value = nextTasks;
    stats.value = nextStats;
    loaded.value = true;
    if (selected.value) {
      const current = nextTasks.find((task) => task.id === selected.value?.id);
      if (current) syncSelected(current); else { selected.value = null; detailVisible.value = false; }
    }
    errorMessage.value = '';
    return true;
  } catch (error) {
    if (requestGeneration === generation && contestId === selectedContestId.value && showLoading) errorMessage.value = getErrorMessage(error);
    return false;
  } finally {
    if (requestGeneration === generation && showLoading) loading.value = false;
  }
}

function connectRealtime() {
  realtime?.stop();
  realtimeConnected.value = false;
  if (!selectedContestId.value) return;
  realtime = subscribeContestEvents({
    contestId: selectedContestId.value, scope: 'STAFF', eventTypes: ['BALLOON_TASK_UPDATED'],
    onEvent: () => { void loadData(false); },
    onConnectionChange: (connected) => { realtimeConnected.value = connected; },
    poll: async () => { await loadData(false); },
  });
}

async function updateQuery() {
  await router.replace({ query: { contestId: selectedContestId.value ? String(selectedContestId.value) : undefined, status: statusFilter.value === 'ALL' ? undefined : statusFilter.value } });
}
async function changeContest() {
  generation += 1; tasks.value = []; stats.value = null; selected.value = null; detailVisible.value = false; loaded.value = false; problemFilter.value = '';
  await updateQuery(); connectRealtime(); await loadData();
}
async function changeFilter() {
  generation += 1; tasks.value = []; selected.value = null; detailVisible.value = false; loaded.value = false; problemFilter.value = '';
  await updateQuery(); await loadData();
}
function syncSelected(task: BalloonTask) { selected.value = task; note.value = task.note ?? ''; }
function openDetail(task: BalloonTask) { syncSelected(task); detailVisible.value = true; }
function syncMutation(task: BalloonTask) { syncSelected(task); tasks.value = [task, ...tasks.value.filter((item) => item.id !== task.id)]; }

async function runMutation(kind: typeof action.value, operation: () => Promise<BalloonTask>, success: string): Promise<boolean> {
  action.value = kind;
  try { syncMutation(await operation()); const refreshed = await loadData(false); refreshed ? ElMessage.success(success) : ElMessage.warning(`${success}，但刷新任务列表失败`); return true; }
  catch (error) { ElMessage.error(getErrorMessage(error)); return false; }
  finally { action.value = ''; }
}
async function claimTask() { if (selected.value) await runMutation('claim', () => balloonApi.claim(selected.value!.id, selected.value!.version), '任务已领取'); }
async function deliverTask() {
  if (!selected.value) return;
  try { await ElMessageBox.confirm(`确认气球已送达 ${selected.value.teamName}（座位 ${selected.value.seatNo ?? '—'}）？`, '确认送达', { type: 'success' }); } catch { return; }
  await runMutation('deliver', () => balloonApi.deliver(selected.value!.id, selected.value!.version), '任务已标记为送达');
}
function openCancel() { cancelReason.value = ''; cancelVisible.value = true; }
async function cancelTask() { if (!selected.value || cancelError.value) return; if (await runMutation('cancel', () => balloonApi.cancel(selected.value!.id, selected.value!.version, cancelReason.value.trim()), '任务已取消')) cancelVisible.value = false; }
async function reopenTask() { if (selected.value) await runMutation('reopen', () => balloonApi.reopen(selected.value!.id, selected.value!.version), '任务已重新打开'); }
async function saveNote() { if (selected.value) await runMutation('note', () => balloonApi.note(selected.value!.id, selected.value!.version, note.value.trim() || null), '配送备注已保存'); }
async function dispatchTasks() { if (!selectedContestId.value) return; action.value = 'dispatch'; try { const claimed = await balloonApi.dispatch(selectedContestId.value); await loadData(false); if (claimed.length) ElMessage.success(`已按调度策略领取 ${claimed.length} 个任务`); else ElMessage.info('当前没有可调度任务'); } catch (error) { ElMessage.error(getErrorMessage(error)); } finally { action.value = ''; } }

onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const queryContest = Number(route.query.contestId);
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContest) ? queryContest : (contests.value[0]?.id ?? null);
    const queryStatus = route.query.status;
    if (typeof queryStatus === 'string' && balloonTaskStatuses.includes(queryStatus as BalloonTaskStatus)) statusFilter.value = queryStatus as BalloonTaskStatus;
    if (selectedContestId.value) { connectRealtime(); await loadData(); }
  } catch (error) { errorMessage.value = getErrorMessage(error); }
});

onUnmounted(() => realtime?.stop());
</script>
