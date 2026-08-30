<template>
  <el-container direction="vertical" class="admin-page rejudge-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <div>
          <ElButton link :icon="ArrowLeft" @click="router.push(`/admin/contests/${contestId}`)">
            {{ t('返回比赛详情') }}
          </ElButton>
          <h1>{{ t('批量重判工作台') }}</h1>
        </div>
        <div class="admin-page-actions">
          <ElButton :icon="Refresh" :loading="tasksLoading" @click="loadTasks(false)">{{
            t('刷新任务')
          }}</ElButton>
        </div>
      </div>
    </el-header>
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        class="page-alert"
        type="error"
        show-icon
        :closable="false"
        :title="errorMessage"
      />

      <ElRow :gutter="20" class="rejudge-workbench-row">
        <ElCol :xs="24" :md="16">
          <RejudgeFilterPanel
            v-model:filter="filter"
            :contest-problems="contestProblems"
            :contest-teams="contestTeams"
            :verdict-options="verdictOptions"
            :previewing="previewing"
            :preview-result="previewResult"
            :preview-stale="previewStale"
            @preview="preview"
          />
        </ElCol>
        <ElCol :xs="24" :md="8">
          <RejudgeConfirmPanel
            v-model:idempotency-key="idempotencyKey"
            v-model:confirmation-text="confirmationText"
            :preview-result="previewResult"
            :creating="creating"
            :can-create="canCreate"
            :confirmation-requirement="confirmationRequirement"
            @create="createTask"
            @regenerate-key="regenerateIdempotencyKey"
          />
        </ElCol>
      </ElRow>

      <RejudgeTasksTable
        :tasks="tasks"
        :tasks-loading="tasksLoading"
        :polling="polling"
        :mutating-task-id="mutatingTaskId"
        @select="selectTask"
        @pause="pauseTask"
        @resume="resumeTask"
      />

      <RejudgeTaskDetailDialog
        v-model:detail-visible="detailVisible"
        :detail-loading="detailLoading"
        :selected-task="selectedTask"
        @closed="selectedTask = null"
      />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ArrowLeft, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import RejudgeConfirmPanel from '../components/bulk-rejudge/RejudgeConfirmPanel.vue';
import RejudgeFilterPanel from '../components/bulk-rejudge/RejudgeFilterPanel.vue';
import RejudgeTaskDetailDialog from '../components/bulk-rejudge/RejudgeTaskDetailDialog.vue';
import RejudgeTasksTable from '../components/bulk-rejudge/RejudgeTasksTable.vue';
import { adminContestApi } from '../api/admin-contests';
import {
  bulkRejudgeApi,
  type BatchRejudgeFilter,
  type BatchRejudgePreview,
  type BatchRejudgeTask,
  type BatchRejudgeTaskStatus,
  type BatchRejudgeVerdict,
} from '../api/bulk-rejudge';
import { ApiError, getErrorMessage } from '../api/client';
import type { ContestResponse, ContestProblem, ContestTeamResponse } from '../api/types';
import { submissionStatusLabel } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contestId = computed(() => Number(route.params.contestId));
const contest = ref<ContestResponse | null>(null);
const contestProblems = ref<ContestProblem[]>([]);
const contestTeams = ref<ContestTeamResponse[]>([]);
let filter = reactive({
  problemId: null as number | null,
  teamId: null as number | null,
  language: null as string | null,
  verdict: null as BatchRejudgeVerdict | null,
  submittedRange: null as [Date, Date] | null,
});
const previewResult = ref<BatchRejudgePreview | null>(null);
const previewFingerprint = ref('');
const idempotencyKey = ref('');
const confirmationText = ref('');
const tasks = ref<BatchRejudgeTask[]>([]);
const selectedTask = ref<BatchRejudgeTask | null>(null);
const detailVisible = ref(false);
const detailLoading = ref(false);
const previewing = ref(false);
const creating = ref(false);
const tasksLoading = ref(false);
const mutatingTaskId = ref<number | null>(null);
const errorMessage = ref('');
let pollTimer: number | undefined;

const verdictOptions = computed(() =>
  [
    'ACCEPTED',
    'WRONG_ANSWER',
    'COMPILE_ERROR',
    'RUNTIME_ERROR',
    'TIME_LIMIT_EXCEEDED',
    'MEMORY_LIMIT_EXCEEDED',
    'OUTPUT_LIMIT_EXCEEDED',
    'SYSTEM_ERROR',
    'CANCELLED',
  ].map((value) => ({ value: value as BatchRejudgeVerdict, label: submissionStatusLabel(value) })),
);

const filterRequest = computed<BatchRejudgeFilter>(() => ({
  problemId: filter.problemId,
  teamId: filter.teamId,
  language: filter.language,
  verdict: filter.verdict,
  submittedFrom: filter.submittedRange?.[0]?.toISOString() ?? null,
  submittedTo: filter.submittedRange?.[1]?.toISOString() ?? null,
}));
const currentFingerprint = computed(() => JSON.stringify(filterRequest.value));
const previewStale = computed(() =>
  Boolean(previewResult.value && currentFingerprint.value !== previewFingerprint.value),
);
const confirmationRequirement = computed(() =>
  previewResult.value ? `REJUDGE ${previewResult.value.matchedSubmissions}` : '',
);
const idempotencyKeyValid = computed(() => {
  const bytes = new TextEncoder().encode(idempotencyKey.value.trim()).length;
  return bytes >= 8 && bytes <= 128;
});
const canCreate = computed(() =>
  Boolean(
    previewResult.value &&
    !previewStale.value &&
    previewResult.value.matchedSubmissions >= 1 &&
    previewResult.value.matchedSubmissions <= 10_000 &&
    confirmationText.value === confirmationRequirement.value &&
    idempotencyKeyValid.value,
  ),
);
const polling = computed(() => tasks.value.some((task) => isActiveTask(task.status)));

function regenerateIdempotencyKey() {
  idempotencyKey.value = `batch-rejudge-${contestId.value}-${crypto.randomUUID()}`;
}

async function loadContext() {
  errorMessage.value = '';
  try {
    const [contestValue, problemValues, teamValues] = await Promise.all([
      adminContestApi.getContest(contestId.value),
      adminContestApi.listContestProblems(contestId.value),
      adminContestApi.listContestTeams(contestId.value),
    ]);
    contest.value = contestValue;
    contestProblems.value = [...problemValues].sort((a, b) => a.displayOrder - b.displayOrder);
    contestTeams.value = [...teamValues].sort((a, b) =>
      a.teamName.localeCompare(b.teamName, 'zh-CN'),
    );
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function preview() {
  previewing.value = true;
  errorMessage.value = '';
  try {
    const request = filterRequest.value;
    previewResult.value = await bulkRejudgeApi.preview(contestId.value, request);
    previewFingerprint.value = JSON.stringify(request);
    confirmationText.value = '';
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    previewing.value = false;
  }
}

async function createTask() {
  if (!previewResult.value || !canCreate.value) return;
  creating.value = true;
  errorMessage.value = '';
  try {
    const task = await bulkRejudgeApi.create(contestId.value, {
      filter: filterRequest.value,
      expectedCount: previewResult.value.matchedSubmissions,
      confirmationText: confirmationText.value,
      idempotencyKey: idempotencyKey.value.trim(),
    });
    ElMessage.success(t('批量重判任务 #{id} 已创建', { id: task.id }));
    selectedTask.value = task;
    detailVisible.value = true;
    confirmationText.value = '';
    await loadTasks(true);
  } catch (error) {
    if (error instanceof ApiError && error.code === 'BATCH_REJUDGE_COUNT_CHANGED') {
      previewResult.value = null;
      previewFingerprint.value = '';
      confirmationText.value = '';
      ElMessage.error(t('符合条件的提交集合已变化，请重新预览并确认'));
    } else if (error instanceof ApiError && error.code === 'IDEMPOTENCY_KEY_REUSED') {
      ElMessage.error(t('该幂等键已用于其他批量重判，请生成新键后重试'));
    } else {
      ElMessage.error(getErrorMessage(error));
    }
  } finally {
    creating.value = false;
  }
}

async function loadTasks(silent = true) {
  if (!silent) tasksLoading.value = true;
  try {
    tasks.value = await bulkRejudgeApi.list(contestId.value);
    if (detailVisible.value && selectedTask.value) {
      selectedTask.value = await bulkRejudgeApi.get(contestId.value, selectedTask.value.id);
    }
    schedulePolling();
  } catch (error) {
    if (!silent) errorMessage.value = getErrorMessage(error);
    schedulePolling(8_000);
  } finally {
    tasksLoading.value = false;
  }
}

async function selectTask(taskId: number) {
  detailVisible.value = true;
  detailLoading.value = true;
  selectedTask.value = null;
  try {
    selectedTask.value = await bulkRejudgeApi.get(contestId.value, taskId);
  } catch (error) {
    detailVisible.value = false;
    ElMessage.error(getErrorMessage(error));
  } finally {
    detailLoading.value = false;
  }
}

async function pauseTask(task: BatchRejudgeTask) {
  try {
    await ElMessageBox.confirm(
      t('暂停后不会再领取新条目；当前正在处理的条目可能仍会完成。'),
      t('暂停任务 #{id}', { id: task.id }),
      { type: 'warning', confirmButtonText: t('确认暂停') },
    );
  } catch {
    return;
  }
  mutatingTaskId.value = task.id;
  try {
    const updated = await bulkRejudgeApi.pause(contestId.value, task.id);
    replaceTask(updated);
    ElMessage.success(t('任务 #{id} 已暂停', { id: task.id }));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutatingTaskId.value = null;
  }
}

async function resumeTask(task: BatchRejudgeTask) {
  mutatingTaskId.value = task.id;
  try {
    const updated = await bulkRejudgeApi.resume(contestId.value, task.id);
    replaceTask(updated);
    ElMessage.success(t('任务 #{id} 已恢复', { id: task.id }));
    schedulePolling(0);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    mutatingTaskId.value = null;
  }
}

function replaceTask(updated: BatchRejudgeTask) {
  const index = tasks.value.findIndex((task) => task.id === updated.id);
  if (index >= 0) tasks.value[index] = { ...updated, items: [] };
  if (selectedTask.value?.id === updated.id) selectedTask.value = updated;
}

function schedulePolling(delay = 2_500) {
  if (pollTimer) window.clearTimeout(pollTimer);
  if (document.hidden || !polling.value) return;
  pollTimer = window.setTimeout(() => void loadTasks(true), delay);
}

function handleVisibility() {
  if (!document.hidden) void loadTasks(true);
}

function isActiveTask(status: BatchRejudgeTaskStatus) {
  return status === 'PENDING' || status === 'RUNNING';
}

onMounted(() => {
  document.addEventListener('visibilitychange', handleVisibility);
});

watch(
  contestId,
  () => {
    regenerateIdempotencyKey();
    void Promise.all([loadContext(), loadTasks(false)]);
  },
  { immediate: true },
);

onUnmounted(() => {
  if (pollTimer) window.clearTimeout(pollTimer);
  document.removeEventListener('visibilitychange', handleVisibility);
});
</script>

<style scoped>
.admin-page {
  width: min(1320px, 100%);
  margin: 0 auto;
}
.page-head {
  height: auto;
  padding: 42px 42px 0;
}
.page-body {
  padding: 0 42px 42px;
}
.admin-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 28px;
}
.admin-page-header.compact {
  align-items: center;
}
.admin-page-header h1 {
  margin: 5px 0 6px;
  font-size: clamp(28px, 4vw, 40px);
  color: #13213b;
}
.admin-page-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
.page-alert {
  margin-bottom: 20px;
}
.rejudge-workbench-row {
  margin-bottom: 20px;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
@media (max-width: 640px) {
  .page-head {
    padding: 24px 16px 0;
  }
  .page-body {
    padding: 0 16px 24px;
  }
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 16px;
  }
}
</style>
