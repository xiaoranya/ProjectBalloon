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
          <ElButton :icon="Refresh" :loading="tasksLoading" @click="loadTasks(false)"
            >{{ t('刷新任务') }}</ElButton
          >
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
          <ElCard shadow="never" class="rejudge-filter-card">
            <template #header>
              <div class="card-header">
                <div>
                  <strong>{{ t('1. 筛选与预览') }}</strong>
                  <small>{{ t('比赛范围由 URL 固定；全部筛选均可留空。') }}</small>
                </div>
              </div>
            </template>

            <ElForm label-position="top">
              <ElRow :gutter="14" class="rejudge-filter-grid-row">
                <ElCol :xs="24" :sm="12" :md="6">
                  <ElFormItem :label="t('题目')">
                    <ElSelect
                      v-model="filter.problemId"
                      clearable
                      filterable
                      :placeholder="t('全部题目')"
                    >
                      <ElOption
                        v-for="problem in contestProblems"
                        :key="problem.problemId"
                        :label="`${problem.alias} · ${problem.title}`"
                        :value="problem.problemId"
                      />
                    </ElSelect>
                  </ElFormItem>
                </ElCol>
                <ElCol :xs="24" :sm="12" :md="6">
                  <ElFormItem :label="t('队伍')">
                    <ElSelect v-model="filter.teamId" clearable filterable :placeholder="t('全部队伍')">
                      <ElOption
                        v-for="team in contestTeams"
                        :key="team.teamId"
                        :label="`${team.teamName} (#${team.teamId})`"
                        :value="team.teamId"
                      />
                    </ElSelect>
                  </ElFormItem>
                </ElCol>
                <ElCol :xs="24" :sm="12" :md="6">
                  <ElFormItem :label="t('语言')">
                    <ElSelect v-model="filter.language" clearable :placeholder="t('全部语言')">
                      <ElOption label="C" value="c" />
                      <ElOption label="C++" value="cpp" />
                      <ElOption label="Java" value="java" />
                      <ElOption label="Python" value="python" />
                    </ElSelect>
                  </ElFormItem>
                </ElCol>
                <ElCol :xs="24" :sm="12" :md="6">
                  <ElFormItem :label="t('当前判罚')">
                    <ElSelect v-model="filter.verdict" clearable :placeholder="t('全部最终判罚')">
                      <ElOption
                        v-for="option in verdictOptions"
                        :key="option.value"
                        :label="option.label"
                        :value="option.value"
                      />
                    </ElSelect>
                  </ElFormItem>
                </ElCol>
                <ElCol :xs="24" :sm="12" :md="12">
                  <ElFormItem :label="t('提交时间')">
                    <ElDatePicker
                      v-model="filter.submittedRange"
                      type="datetimerange"
                      :start-placeholder="t('起始时间')"
                      :end-placeholder="t('结束时间')"
                      :range-separator="t('至')"
                    />
                  </ElFormItem>
                </ElCol>
              </ElRow>
            </ElForm>

            <ElSpace wrap :size="14" class="rejudge-preview-actions-row">
              <ElButton type="primary" plain :loading="previewing" @click="preview">
                {{ t('预览影响范围') }}
              </ElButton>
              <span class="muted-text">{{ t('预览只统计，不创建判题任务。') }}</span>
            </ElSpace>

            <ElAlert
              v-if="previewResult && previewStale"
              class="rejudge-inline-alert"
              type="warning"
              show-icon
              :closable="false"
              :title="t('筛选条件已变化，当前预览已失效，请重新预览。')"
            />
          </ElCard>
        </ElCol>

        <ElCol :xs="24" :md="8">
          <ElCard shadow="never" class="rejudge-confirm-card">
            <template #header>
              <div class="card-header">
                <div>
                  <strong>{{ t('2. 确认创建') }}</strong>
                  <small>{{ t('数量快照与幂等键共同保护重复操作。') }}</small>
                </div>
              </div>
            </template>

            <ElEmpty v-if="!previewResult" :description="t('先预览筛选结果')" :image-size="76" />
            <template v-else>
              <div class="rejudge-preview-count" aria-live="polite">
                <span>{{ t('匹配提交') }}</span>
                <strong>{{ previewResult.matchedSubmissions }}</strong>
                <small>{{ t('Rust 创建上限为 10,000 条') }}</small>
              </div>

              <ElAlert
                v-if="previewResult.matchedSubmissions === 0"
                class="rejudge-inline-alert"
                type="info"
                show-icon
                :closable="false"
                :title="t('当前筛选没有可重判的已完成提交。')"
              />
              <ElAlert
                v-else-if="previewResult.matchedSubmissions > 10_000"
                class="rejudge-inline-alert"
                type="warning"
                show-icon
                :closable="false"
                :title="t('匹配数量超过单任务上限，请缩小筛选范围。')"
              />

              <ElForm label-position="top" class="rejudge-confirm-form">
                <ElFormItem :label="t('幂等键')">
                  <ElInput v-model="idempotencyKey" maxlength="128" show-word-limit>
                    <template #append>
                      <ElButton
                        :icon="Refresh"
                        :aria-label="t('生成新幂等键')"
                        @click="regenerateIdempotencyKey"
                      />
                    </template>
                  </ElInput>
                  <p class="form-help">
                    {{ t('需为 8–128 字节。网络重试时保留同一键；只在发起一项新操作时更换。') }}
                  </p>
                </ElFormItem>
                <ElFormItem :label="t('输入 {text} 以继续', { text: confirmationRequirement || t('确认文本') })">
                  <ElInput
                    v-model="confirmationText"
                    autocomplete="off"
                    :placeholder="confirmationRequirement"
                  />
                </ElFormItem>
              </ElForm>

              <ElAlert
                class="rejudge-inline-alert"
                type="warning"
                show-icon
                :closable="false"
                :title="t('重判会让匹配提交重新进入评测队列，并可能短暂改变榜单；任务不能整体撤销。')"
              />
              <ElButton
                type="danger"
                class="wide-button"
                :loading="creating"
                :disabled="!canCreate"
                @click="createTask"
              >
                {{ t('创建批量重判任务') }}
              </ElButton>
            </template>
          </ElCard>
        </ElCol>
      </ElRow>

      <ElCard shadow="never" class="rejudge-tasks-card">
        <template #header>
          <div class="card-header">
            <div>
              <strong>{{ t('任务列表') }}</strong>
              <small>{{ t('最多显示本场比赛最近 100 项；活动任务自动轮询。') }}</small>
            </div>
            <span class="rejudge-poll-state" aria-live="polite">
              {{ polling ? t('自动刷新中') : t('暂无活动任务') }}
            </span>
          </div>
        </template>

        <ElTable v-loading="tasksLoading && tasks.length === 0" :data="tasks" row-key="id">
          <ElTableColumn prop="id" :label="t('任务 ID')" width="100" />
          <ElTableColumn :label="t('状态')" width="120">
            <template #default="{ row }">
              <ElTag :type="taskStatusType(row.status)">{{ taskStatusLabel(row.status) }}</ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('进度')" min-width="250">
            <template #default="{ row }">
              <ElProgress
                :percentage="progressPercentage(row as BatchRejudgeTask)"
                :status="progressStatus(row as BatchRejudgeTask)"
                :stroke-width="10"
              />
              <small class="rejudge-progress-copy">
                {{ t('{processed} / {total} · 成功 {success} · 失败 {failed}', { processed: row.processedItems, total: row.totalItems, success: row.succeededItems, failed: row.failedItems }) }}
              </small>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('创建时间')" min-width="170">
            <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('操作')" width="220" fixed="right">
            <template #default="{ row }">
              <ElButton link @click="selectTask(row.id)">{{ t('查看明细') }}</ElButton>
              <ElButton
                v-if="row.status === 'PENDING' || row.status === 'RUNNING'"
                link
                type="warning"
                :loading="mutatingTaskId === row.id"
                @click="pauseTask(row as BatchRejudgeTask)"
              >
                {{ t('暂停') }}
              </ElButton>
              <ElButton
                v-if="row.status === 'PAUSED'"
                link
                type="primary"
                :loading="mutatingTaskId === row.id"
                @click="resumeTask(row as BatchRejudgeTask)"
              >
                {{ t('恢复') }}
              </ElButton>
            </template>
          </ElTableColumn>
          <template #empty><ElEmpty :description="t('尚未创建批量重判任务')" /></template>
        </ElTable>
      </ElCard>

      <ElDialog
        v-model="detailVisible"
        :title="t('批量重判任务明细')"
        width="min(1100px, 94vw)"
        @closed="selectedTask = null"
      >
        <ElSkeleton v-if="detailLoading" :rows="8" animated />
        <template v-else-if="selectedTask">
          <div class="rejudge-detail-heading">
            <ElDescriptions :column="2" border>
              <ElDescriptionsItem :label="t('任务')">#{{ selectedTask.id }}</ElDescriptionsItem>
              <ElDescriptionsItem :label="t('状态')">
                <ElTag :type="taskStatusType(selectedTask.status)">{{
                  taskStatusLabel(selectedTask.status)
                }}</ElTag>
              </ElDescriptionsItem>
              <ElDescriptionsItem :label="t('进度')">
                {{ selectedTask.processedItems }} / {{ selectedTask.totalItems }}
              </ElDescriptionsItem>
              <ElDescriptionsItem :label="t('成功 / 失败')">
                {{ selectedTask.succeededItems }} / {{ selectedTask.failedItems }}
              </ElDescriptionsItem>
              <ElDescriptionsItem :label="t('开始')">{{
                formatDateTime(selectedTask.startedAt)
              }}</ElDescriptionsItem>
              <ElDescriptionsItem :label="t('完成')">{{
                formatDateTime(selectedTask.completedAt)
              }}</ElDescriptionsItem>
            </ElDescriptions>
            <ElProgress
              :percentage="progressPercentage(selectedTask)"
              :status="progressStatus(selectedTask)"
              :stroke-width="14"
            />
          </div>

          <ElAlert
            v-if="selectedTask.itemsTruncated"
            class="rejudge-inline-alert"
            type="warning"
            show-icon
            :closable="false"
            :title="t('任务共有 {total} 条，明细仅返回按 ID 排序的前 1,000 条。聚合进度仍是完整数据。', { total: selectedTask.totalItems })"
          />

          <ElTable :data="selectedTask.items" row-key="id" max-height="500">
            <ElTableColumn prop="submissionId" :label="t('提交 ID')" width="110" />
            <ElTableColumn :label="t('状态')" width="120">
              <template #default="{ row }">
                <ElTag :type="itemStatusType(row.status)" effect="plain">{{
                  itemStatusLabel(row.status)
                }}</ElTag>
              </template>
            </ElTableColumn>
            <ElTableColumn prop="attempts" :label="t('尝试次数')" width="100" />
            <ElTableColumn prop="oldJudgementId" :label="t('原判定 ID')" min-width="250">
              <template #default="{ row }"
                ><code>{{ row.oldJudgementId ?? '—' }}</code></template
              >
            </ElTableColumn>
            <ElTableColumn prop="newJudgementId" :label="t('新判定 ID')" min-width="250">
              <template #default="{ row }"
                ><code>{{ row.newJudgementId ?? '—' }}</code></template
              >
            </ElTableColumn>
            <ElTableColumn :label="t('处理时间')" min-width="170">
              <template #default="{ row }">{{ formatDateTime(row.processedAt) }}</template>
            </ElTableColumn>
            <ElTableColumn prop="errorMessage" :label="t('错误')" min-width="240">
              <template #default="{ row }"
                ><span class="error-text">{{ row.errorMessage ?? '—' }}</span></template
              >
            </ElTableColumn>
            <template #empty><ElEmpty :description="t('任务明细尚未生成')" /></template>
          </ElTable>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { ArrowLeft, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { adminContestApi } from '../api/admin-contests';
import {
  bulkRejudgeApi,
  type BatchRejudgeFilter,
  type BatchRejudgeItemStatus,
  type BatchRejudgePreview,
  type BatchRejudgeTask,
  type BatchRejudgeTaskStatus,
  type BatchRejudgeVerdict,
} from '../api/bulk-rejudge';
import { ApiError, getErrorMessage } from '../api/client';
import type { Contest, ContestProblem, ContestTeam } from '../api/types';
import { formatDateTime, submissionStatusLabel } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const { t } = useI18n();
const contestId = Number(route.params.contestId);
const contest = ref<Contest | null>(null);
const contestProblems = ref<ContestProblem[]>([]);
const contestTeams = ref<ContestTeam[]>([]);
const filter = reactive({
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

const verdictOptions = computed(() => [
  'ACCEPTED',
  'WRONG_ANSWER',
  'COMPILE_ERROR',
  'RUNTIME_ERROR',
  'TIME_LIMIT_EXCEEDED',
  'MEMORY_LIMIT_EXCEEDED',
  'OUTPUT_LIMIT_EXCEEDED',
  'SYSTEM_ERROR',
  'CANCELLED',
].map((value) => ({ value: value as BatchRejudgeVerdict, label: submissionStatusLabel(value) })));

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
  idempotencyKey.value = `batch-rejudge-${contestId}-${crypto.randomUUID()}`;
}

async function loadContext() {
  errorMessage.value = '';
  try {
    const [contestValue, problemValues, teamValues] = await Promise.all([
      adminContestApi.getContest(contestId),
      adminContestApi.listContestProblems(contestId),
      adminContestApi.listContestTeams(contestId),
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
    previewResult.value = await bulkRejudgeApi.preview(contestId, request);
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
    const task = await bulkRejudgeApi.create(contestId, {
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
    tasks.value = await bulkRejudgeApi.list(contestId);
    if (detailVisible.value && selectedTask.value) {
      selectedTask.value = await bulkRejudgeApi.get(contestId, selectedTask.value.id);
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
    selectedTask.value = await bulkRejudgeApi.get(contestId, taskId);
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
    const updated = await bulkRejudgeApi.pause(contestId, task.id);
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
    const updated = await bulkRejudgeApi.resume(contestId, task.id);
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

function progressPercentage(task: BatchRejudgeTask) {
  if (task.totalItems === 0) return 0;
  return Math.min(100, Math.round((task.processedItems / task.totalItems) * 100));
}

function progressStatus(task: BatchRejudgeTask): '' | 'success' | 'exception' | 'warning' {
  if (task.failedItems > 0) return 'exception';
  if (task.status === 'COMPLETED') return 'success';
  if (task.status === 'PAUSED') return 'warning';
  return '';
}

function taskStatusType(
  status: BatchRejudgeTaskStatus,
): 'success' | 'warning' | 'danger' | 'info' | 'primary' {
  return ({
    PENDING: 'info',
    RUNNING: 'primary',
    PAUSED: 'warning',
    COMPLETED: 'success',
    CANCELLED: 'danger',
  }[status] as 'success' | 'warning' | 'danger' | 'info' | 'primary');
}

function taskStatusLabel(status: BatchRejudgeTaskStatus) {
  return t({
    PENDING: '等待执行',
    RUNNING: '执行中',
    PAUSED: '已暂停',
    COMPLETED: '已完成',
    CANCELLED: '已终止',
  }[status]);
}

function itemStatusType(
  status: BatchRejudgeItemStatus,
): 'success' | 'warning' | 'danger' | 'info' | 'primary' {
  return ({
    PENDING: 'info',
    PROCESSING: 'primary',
    SUCCEEDED: 'success',
    FAILED: 'danger',
    CANCELLED: 'warning',
  }[status] as 'success' | 'warning' | 'danger' | 'info' | 'primary');
}

function itemStatusLabel(status: BatchRejudgeItemStatus) {
  return t({
    PENDING: '等待',
    PROCESSING: '处理中',
    SUCCEEDED: '成功',
    FAILED: '失败',
    CANCELLED: '已终止',
  }[status]);
}

onMounted(async () => {
  regenerateIdempotencyKey();
  document.addEventListener('visibilitychange', handleVisibility);
  await Promise.all([loadContext(), loadTasks(false)]);
});

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
.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.card-header > div {
  min-width: 0;
}
.card-header small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.rejudge-workbench-row {
  margin-bottom: 20px;
}
.rejudge-filter-grid-row :deep(.el-select),
.rejudge-filter-grid-row :deep(.el-date-editor) {
  width: 100%;
}
.rejudge-preview-actions-row {
  margin-top: 2px;
}
.rejudge-preview-count {
  display: grid;
  place-items: center;
  margin: 2px 0 18px;
  padding: 22px;
  border: 1px solid #dbeafe;
  border-radius: 0;
  background: #f7faff;
  text-align: center;
}
.rejudge-preview-count span,
.rejudge-preview-count small,
.rejudge-progress-copy {
  color: var(--muted);
  font-size: 12px;
}
.rejudge-preview-count strong {
  margin: 4px 0;
  color: #172033;
  font-size: 42px;
  font-variant-numeric: tabular-nums;
  line-height: 1;
}
.rejudge-confirm-form {
  display: grid;
  gap: 14px;
}
.rejudge-confirm-form :deep(.el-form-item:last-child) {
  margin-bottom: 0;
}
.rejudge-tasks-card {
  margin-bottom: 20px;
}
.rejudge-poll-state {
  color: var(--muted);
  font-size: 12px;
}
.rejudge-progress-copy {
  display: block;
  margin-top: 6px;
  font-variant-numeric: tabular-nums;
}
.rejudge-detail-heading {
  display: grid;
  gap: 14px;
  margin-bottom: 16px;
}
.rejudge-page code {
  font-size: 12px;
  overflow-wrap: anywhere;
}
.muted-text {
  color: var(--muted);
}
.error-text {
  color: #dc2626 !important;
}
.form-help {
  color: var(--el-text-color-secondary);
  font-size: 0.8rem;
}
.wide-button {
  width: 100%;
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
