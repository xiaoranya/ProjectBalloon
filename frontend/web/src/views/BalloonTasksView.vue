<template>
  <el-container direction="vertical" class="judge-page">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <h1>{{ t('气球配送工作台') }}</h1>
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

      <BalloonStatsRow :stats="stats" />
      <BalloonToolbar
        v-model:selected-contest-id="selectedContestId"
        v-model:status-filter="statusFilter"
        v-model:problem-filter="problemFilter"
        v-model:keyword="keyword"
        :contests="contests"
        :problem-options="problemOptions"
        :loading="loading"
        :action="action"
        @change-contest="changeContest"
        @change-filter="changeFilter"
        @dispatch="dispatchTasks"
        @refresh="loadData(false)"
      />
      <BalloonTasksTable
        :tasks="filteredTasks"
        :loading="loading"
        :loaded="loaded"
        @open-detail="openDetail"
      />
      <BalloonDetailDrawer
        v-model:selected="selected"
        v-model:detail-visible="detailVisible"
        v-model:note="note"
        :action="action"
        :can-deliver="canDeliver"
        @claim="claimTask"
        @deliver="deliverTask"
        @reopen="reopenTask"
        @open-cancel="openCancel"
        @save-note="saveNote"
      />
      <BalloonCancelDialog
        v-model:cancel-visible="cancelVisible"
        v-model:cancel-reason="cancelReason"
        :action="action"
        @cancel="cancelTask"
      />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { ElMessage, ElMessageBox, ElNotification } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { useSession } from '../auth/session';
import {
  balloonApi,
  balloonTaskStatuses,
  type BalloonStats,
  type BalloonTask,
  type BalloonTaskStatus,
} from '../api/balloons';
import { getErrorMessage } from '../api/client';
import { contestApi } from '../api/contest';
import type { ContestResponse } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
import BalloonStatsRow from '../components/balloon-tasks/BalloonStatsRow.vue';
import BalloonToolbar from '../components/balloon-tasks/BalloonToolbar.vue';
import BalloonTasksTable from '../components/balloon-tasks/BalloonTasksTable.vue';
import BalloonDetailDrawer from '../components/balloon-tasks/BalloonDetailDrawer.vue';
import BalloonCancelDialog from '../components/balloon-tasks/BalloonCancelDialog.vue';

const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const contests = ref<ContestResponse[]>([]);
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

const problemOptions = computed(() =>
  [...new Set(tasks.value.map((task) => task.problemAlias))].sort(),
);
const filteredTasks = computed(() => {
  const search = keyword.value.trim().toLocaleLowerCase();
  return tasks.value.filter(
    (task) =>
      (!problemFilter.value || task.problemAlias === problemFilter.value) &&
      (!search ||
        task.teamName.toLocaleLowerCase().includes(search) ||
        (task.seatNo ?? '').toLocaleLowerCase().includes(search)),
  );
});
const canDeliver = computed(
  () =>
    selected.value?.status === 'CLAIMED' &&
    (session.isSuperAdmin.value || selected.value.claimedByUserId === session.state.user?.id),
);

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
    if (
      !showLoading &&
      nextTasks.some((task) => task.status === 'PENDING' && !previous.has(task.id))
    ) {
      ElNotification.warning({ title: t('收到新气球任务'), message: t('有新的气球等待领取。') });
    }
    tasks.value = nextTasks;
    stats.value = nextStats;
    loaded.value = true;
    if (selected.value) {
      const current = nextTasks.find((task) => task.id === selected.value?.id);
      if (current) syncSelected(current);
      else {
        selected.value = null;
        detailVisible.value = false;
      }
    }
    errorMessage.value = '';
    return true;
  } catch (error) {
    if (requestGeneration === generation && contestId === selectedContestId.value && showLoading)
      errorMessage.value = getErrorMessage(error);
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
    contestId: selectedContestId.value,
    scope: 'STAFF',
    eventTypes: ['BALLOON_TASK_UPDATED'],
    onEvent: () => {
      void loadData(false);
    },
    onConnectionChange: (connected) => {
      realtimeConnected.value = connected;
    },
    poll: async () => {
      await loadData(false);
    },
  });
}

async function updateQuery() {
  await router.replace({
    query: {
      contestId: selectedContestId.value ? String(selectedContestId.value) : undefined,
      status: statusFilter.value === 'ALL' ? undefined : statusFilter.value,
    },
  });
}
async function changeContest() {
  generation += 1;
  tasks.value = [];
  stats.value = null;
  selected.value = null;
  detailVisible.value = false;
  loaded.value = false;
  problemFilter.value = '';
  await updateQuery();
  connectRealtime();
  await loadData();
}
async function changeFilter() {
  generation += 1;
  tasks.value = [];
  selected.value = null;
  detailVisible.value = false;
  loaded.value = false;
  problemFilter.value = '';
  await updateQuery();
  await loadData();
}
function syncSelected(task: BalloonTask) {
  selected.value = task;
  note.value = task.note ?? '';
}
function openDetail(task: BalloonTask) {
  syncSelected(task);
  detailVisible.value = true;
}
function syncMutation(task: BalloonTask) {
  syncSelected(task);
  tasks.value = [task, ...tasks.value.filter((item) => item.id !== task.id)];
}

async function runMutation(
  kind: typeof action.value,
  operation: () => Promise<BalloonTask>,
  success: string,
): Promise<boolean> {
  action.value = kind;
  try {
    syncMutation(await operation());
    const refreshed = await loadData(false);
    if (refreshed) {
      ElMessage.success(success);
    } else {
      ElMessage.warning(t('{message}，但刷新任务列表失败', { message: success }));
    }
    return true;
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    return false;
  } finally {
    action.value = '';
  }
}
async function claimTask() {
  if (selected.value)
    await runMutation(
      'claim',
      () => balloonApi.claim(selected.value!.id, selected.value!.version),
      t('任务已领取'),
    );
}
async function deliverTask() {
  if (!selected.value) return;
  try {
    await ElMessageBox.confirm(
      t('确认气球已送达 {team}（座位 {seat}）？', {
        team: selected.value.teamName,
        seat: selected.value.seatNo ?? '—',
      }),
      t('确认送达'),
      { type: 'success' },
    );
  } catch {
    return;
  }
  await runMutation(
    'deliver',
    () => balloonApi.deliver(selected.value!.id, selected.value!.version),
    t('任务已标记为送达'),
  );
}
function openCancel() {
  cancelReason.value = '';
  cancelVisible.value = true;
}
async function cancelTask() {
  if (!selected.value || !cancelReason.value.trim()) return;
  if (
    await runMutation(
      'cancel',
      () =>
        balloonApi.cancel(selected.value!.id, selected.value!.version, cancelReason.value.trim()),
      t('任务已取消'),
    )
  )
    cancelVisible.value = false;
}
async function reopenTask() {
  if (selected.value)
    await runMutation(
      'reopen',
      () => balloonApi.reopen(selected.value!.id, selected.value!.version),
      t('任务已重新打开'),
    );
}
async function saveNote() {
  if (selected.value)
    await runMutation(
      'note',
      () => balloonApi.note(selected.value!.id, selected.value!.version, note.value.trim() || null),
      t('配送备注已保存'),
    );
}
async function dispatchTasks() {
  if (!selectedContestId.value) return;
  action.value = 'dispatch';
  try {
    const claimed = await balloonApi.dispatch(selectedContestId.value);
    await loadData(false);
    if (claimed.length)
      ElMessage.success(t('已按调度策略领取 {count} 个任务', { count: claimed.length }));
    else ElMessage.info(t('当前没有可调度任务'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    action.value = '';
  }
}

onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const queryContest = Number(route.query.contestId);
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContest)
      ? queryContest
      : (contests.value[0]?.id ?? null);
    const queryStatus = route.query.status;
    if (
      typeof queryStatus === 'string' &&
      balloonTaskStatuses.includes(queryStatus as BalloonTaskStatus)
    )
      statusFilter.value = queryStatus as BalloonTaskStatus;
    if (selectedContestId.value) {
      connectRealtime();
      await loadData();
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
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
  height: auto;
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
.page-alert {
  margin-bottom: 20px;
}
@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
