<template>
  <el-container direction="vertical" class="page-section clarification-page">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <h1>{{ t('赛中答疑') }}</h1>
        </div>
        <div
          class="clarification-live-state"
          :class="{ connected: realtimeConnected }"
          aria-live="polite"
        >
          <span />{{ t(realtimeConnected ? '实时更新' : '轮询更新') }}
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

      <ElRow :gutter="24" align="top" class="clarification-team-grid">
        <ElCol :xs="24" :md="9">
          <ElCard shadow="never" class="clarification-compose-card">
            <template #header>
              <div class="card-header">
                <div>
                  <strong>{{ t('提交新问题') }}</strong
                  ><small>{{ t('每队每 5 分钟最多提问一次。') }}</small>
                </div>
              </div>
            </template>
            <ElForm label-position="top" @submit.prevent="submitQuestion">
              <ElFormItem :label="t('问题范围')">
                <ElRadioGroup v-model="form.scope">
                  <ElRadioButton value="GENERAL">{{ t('通用问题') }}</ElRadioButton>
                  <ElRadioButton value="PROBLEM">{{ t('题目相关') }}</ElRadioButton>
                </ElRadioGroup>
              </ElFormItem>
              <ElFormItem v-if="form.scope === 'PROBLEM'" :label="t('题目')">
                <ElSelect
                  v-model="form.problemId"
                  filterable
                  :placeholder="t('请选择题目')"
                  class="wide-control"
                >
                  <ElOption
                    v-for="problem in problems"
                    :key="problem.problemId"
                    :label="`${problem.alias} · ${problem.title}`"
                    :value="problem.problemId"
                  />
                </ElSelect>
              </ElFormItem>
              <ElFormItem :label="t('问题内容')">
                <ElInput
                  v-model="form.question"
                  type="textarea"
                  :rows="7"
                  maxlength="4000"
                  show-word-limit
                  :placeholder="t('请清晰描述需要裁判确认的内容')"
                />
              </ElFormItem>
              <ElButton
                type="primary"
                native-type="submit"
                class="wide-button"
                :loading="submitting"
                :disabled="!canSubmit"
              >
                {{ t('提交问题') }}
              </ElButton>
            </ElForm>
          </ElCard>
        </ElCol>
        <ElCol :xs="24" :md="15">
          <div class="clarification-list-column">
            <div class="clarification-list-heading">
              <div>
                <h2>{{ t('我的问题') }}</h2>
                <p>{{ t('仅显示本队问题和裁判回复。') }}</p>
              </div>
              <ElButton :icon="Refresh" :loading="loading" @click="loadClarifications(false)">{{
                t('刷新')
              }}</ElButton>
            </div>
            <ElSkeleton v-if="loading && clarifications.length === 0" :rows="5" animated />
            <ElEmpty v-else-if="clarifications.length === 0" :description="t('本队尚未提交问题')" />
            <div v-else class="clarification-cards">
              <article v-for="item in clarifications" :key="item.id" class="clarification-card">
                <div class="clarification-card-meta">
                  <div>
                    <ElTag :type="statusType(item.status)">{{ statusLabel(item.status) }}</ElTag>
                    <ElTag v-if="item.problemAlias" type="info" effect="plain">{{
                      t('题目 {alias}', { alias: item.problemAlias ?? '' })
                    }}</ElTag>
                    <ElTag v-else type="info" effect="plain">{{ t('通用') }}</ElTag>
                  </div>
                  <time>{{ formatDateTime(item.createdAt) }}</time>
                </div>
                <h3>{{ item.question }}</h3>
                <div v-if="item.reply" class="clarification-reply">
                  <strong>{{ t('裁判回复') }}</strong>
                  <p>{{ item.reply }}</p>
                  <small>
                    {{ t(item.replyVisibility === 'PUBLIC' ? '公开回复' : '仅本队可见') }} ·
                    {{ formatDateTime(item.repliedAt) }}
                  </small>
                </div>
                <p v-else class="clarification-pending-copy">
                  {{
                    t(item.status === 'CLOSED' ? '该问题已关闭，未提供回复。' : '裁判尚未回复。')
                  }}
                </p>
              </article>
            </div>
          </div>
        </ElCol>
      </ElRow>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onUnmounted, reactive, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import { ElMessage } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import {
  clarificationApi,
  type Clarification,
  type ClarificationScope,
  type ClarificationStatus,
} from '../api/clarifications';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { ContestProblem } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const contestId = computed(() => Number(route.params.contestId));
const { t } = useI18n();
const problems = ref<ContestProblem[]>([]);
const clarifications = ref<Clarification[]>([]);
const loading = ref(false);
const submitting = ref(false);
const errorMessage = ref('');
const realtimeConnected = ref(false);
const form = reactive<{ scope: ClarificationScope; problemId: number | null; question: string }>({
  scope: 'GENERAL',
  problemId: null,
  question: '',
});
let realtime: ContestRealtimeSubscription | undefined;

const canSubmit = computed(
  () => form.question.trim().length > 0 && (form.scope === 'GENERAL' || form.problemId !== null),
);

function statusLabel(status: ClarificationStatus) {
  return t({ PENDING: '待回复', ANSWERED: '已回复', CLOSED: '已关闭' }[status] ?? status);
}

function statusType(status: ClarificationStatus): 'warning' | 'success' | 'info' {
  return status === 'PENDING' ? 'warning' : status === 'ANSWERED' ? 'success' : 'info';
}

async function loadClarifications(silent = true) {
  if (!silent) loading.value = true;
  try {
    clarifications.value = await clarificationApi.listMine(contestId.value);
    errorMessage.value = '';
  } catch (error) {
    if (!silent) errorMessage.value = getErrorMessage(error);
  } finally {
    if (!silent) loading.value = false;
  }
}

async function submitQuestion() {
  const question = form.question.trim();
  if (!canSubmit.value) return;
  submitting.value = true;
  try {
    const request =
      form.scope === 'GENERAL'
        ? { scope: 'GENERAL' as const, problemId: null, question }
        : { scope: 'PROBLEM' as const, problemId: form.problemId!, question };
    const created = await clarificationApi.ask(contestId.value, request);
    clarifications.value = [
      created,
      ...clarifications.value.filter((item) => item.id !== created.id),
    ];
    form.question = '';
    form.problemId = null;
    form.scope = 'GENERAL';
    errorMessage.value = '';
    ElMessage.success(t('问题已提交'));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    submitting.value = false;
  }
}

async function loadAll() {
  loading.value = true;
  const [problemResult] = await Promise.allSettled([
    contestApi.listProblems(contestId.value),
    loadClarifications(false),
  ]);
  if (problemResult.status === 'fulfilled') problems.value = problemResult.value;
  else errorMessage.value = getErrorMessage(problemResult.reason);
  loading.value = false;
}

watch(
  contestId,
  () => {
    realtime?.stop();
    realtime = undefined;
    void loadAll();
    realtime = subscribeContestEvents({
      contestId: contestId.value,
      scope: 'TEAM',
      eventTypes: ['CLARIFICATION_UPDATED'],
      onEvent: () => void loadClarifications(),
      onConnectionChange: (connected) => {
        realtimeConnected.value = connected;
      },
      poll: () => loadClarifications(),
    });
  },
  { immediate: true },
);

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

.page-alert {
  margin-bottom: 20px;
}

.clarification-compose-card {
  position: sticky;
  top: 98px;
  border: 1px solid var(--border);
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

.clarification-reply {
  padding: 16px;
  border: 1px solid #bfdbfe;

  background: #eff6ff;
}

.clarification-reply strong,
.clarification-reply p,
.clarification-reply small {
  display: block;
}

.clarification-reply p {
  margin: 8px 0;
  line-height: 1.7;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.clarification-reply small {
  color: var(--muted);
  font-size: 12px;
}

.clarification-pending-copy {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
}

.card-header > div {
  min-width: 0;
}

@media (max-width: 900px) {
  .clarification-compose-card {
    position: static;
  }
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
