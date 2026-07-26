<template>
  <section class="page-section clarification-page">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Clarifications</p>
        <h1>赛中答疑</h1>
        <p>向裁判提问，并查看本队历史问题和回复。</p>
      </div>
      <div class="clarification-live-state" :class="{ connected: realtimeConnected }" aria-live="polite">
        <span />{{ realtimeConnected ? '实时更新' : '轮询更新' }}
      </div>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <div class="clarification-team-grid">
      <ElCard shadow="never" class="clarification-compose-card">
        <template #header>
          <div class="card-header">
            <div><strong>提交新问题</strong><small>每队每 5 分钟最多提问一次。</small></div>
          </div>
        </template>
        <ElForm label-position="top" @submit.prevent="submitQuestion">
          <ElFormItem label="问题范围">
            <ElRadioGroup v-model="form.scope">
              <ElRadioButton value="GENERAL">通用问题</ElRadioButton>
              <ElRadioButton value="PROBLEM">题目相关</ElRadioButton>
            </ElRadioGroup>
          </ElFormItem>
          <ElFormItem v-if="form.scope === 'PROBLEM'" label="题目">
            <ElSelect v-model="form.problemId" filterable placeholder="请选择题目" class="wide-control">
              <ElOption
                v-for="problem in problems"
                :key="problem.problemId"
                :label="`${problem.alias} · ${problem.title}`"
                :value="problem.problemId"
              />
            </ElSelect>
          </ElFormItem>
          <ElFormItem label="问题内容">
            <ElInput
              v-model="form.question"
              type="textarea"
              :rows="7"
              maxlength="4000"
              show-word-limit
              placeholder="请清晰描述需要裁判确认的内容"
            />
          </ElFormItem>
          <ElButton type="primary" native-type="submit" class="wide-button" :loading="submitting" :disabled="!canSubmit">
            提交问题
          </ElButton>
        </ElForm>
      </ElCard>

      <div class="clarification-list-column">
        <div class="clarification-list-heading">
          <div><h2>我的问题</h2><p>仅显示本队问题和裁判回复。</p></div>
          <ElButton :icon="Refresh" :loading="loading" @click="loadClarifications(false)">刷新</ElButton>
        </div>
        <ElSkeleton v-if="loading && clarifications.length === 0" :rows="5" animated />
        <ElEmpty v-else-if="clarifications.length === 0" description="本队尚未提交问题" />
        <div v-else class="clarification-cards">
          <article v-for="item in clarifications" :key="item.id" class="clarification-card">
            <div class="clarification-card-meta">
              <div>
                <ElTag :type="statusType(item.status)">{{ statusLabel(item.status) }}</ElTag>
                <ElTag v-if="item.problemAlias" type="info" effect="plain">题目 {{ item.problemAlias }}</ElTag>
                <ElTag v-else type="info" effect="plain">通用</ElTag>
              </div>
              <time>{{ formatDateTime(item.createdAt) }}</time>
            </div>
            <h3>{{ item.question }}</h3>
            <div v-if="item.reply" class="clarification-reply">
              <strong>裁判回复</strong>
              <p>{{ item.reply }}</p>
              <small>
                {{ item.replyVisibility === 'PUBLIC' ? '公开回复' : '仅本队可见' }} · {{ formatDateTime(item.repliedAt) }}
              </small>
            </div>
            <p v-else class="clarification-pending-copy">
              {{ item.status === 'CLOSED' ? '该问题已关闭，未提供回复。' : '裁判尚未回复。' }}
            </p>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { useRoute } from 'vue-router';
import { ElMessage } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import { clarificationApi, type Clarification, type ClarificationScope, type ClarificationStatus } from '../api/clarifications';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { ContestProblem } from '../api/types';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';

const route = useRoute();
const contestId = Number(route.params.contestId);
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

const canSubmit = computed(() => form.question.trim().length > 0 && (form.scope === 'GENERAL' || form.problemId !== null));

function statusLabel(status: ClarificationStatus) {
  return { PENDING: '待回复', ANSWERED: '已回复', CLOSED: '已关闭' }[status];
}

function statusType(status: ClarificationStatus): 'warning' | 'success' | 'info' {
  return status === 'PENDING' ? 'warning' : status === 'ANSWERED' ? 'success' : 'info';
}

async function loadClarifications(silent = true) {
  if (!silent) loading.value = true;
  try {
    clarifications.value = await clarificationApi.listMine(contestId);
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
    const request = form.scope === 'GENERAL'
      ? { scope: 'GENERAL' as const, problemId: null, question }
      : { scope: 'PROBLEM' as const, problemId: form.problemId!, question };
    const created = await clarificationApi.ask(contestId, request);
    clarifications.value = [created, ...clarifications.value.filter((item) => item.id !== created.id)];
    form.question = '';
    form.problemId = null;
    form.scope = 'GENERAL';
    errorMessage.value = '';
    ElMessage.success('问题已提交');
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    submitting.value = false;
  }
}

onMounted(async () => {
  loading.value = true;
  const [problemResult] = await Promise.allSettled([
    contestApi.listProblems(contestId),
    loadClarifications(false),
  ]);
  if (problemResult.status === 'fulfilled') problems.value = problemResult.value;
  else errorMessage.value = getErrorMessage(problemResult.reason);
  loading.value = false;
  realtime = subscribeContestEvents({
    contestId,
    scope: 'TEAM',
    eventTypes: ['CLARIFICATION_UPDATED'],
    onEvent: () => void loadClarifications(),
    onConnectionChange: (connected) => { realtimeConnected.value = connected; },
    poll: () => loadClarifications(),
  });
});

onUnmounted(() => realtime?.stop());
</script>
