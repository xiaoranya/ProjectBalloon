<template>
  <el-container direction="vertical" class="page-section narrow-section">
    <el-main class="page-body">
      <ElButton link :icon="ArrowLeft" @click="router.push(`/contests/${contestId}/submissions`)"
        >返回提交记录</ElButton
      >
      <ElSkeleton v-if="loading && !submission" :rows="6" animated />
      <ElAlert
        v-else-if="errorMessage && !submission"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
      />
      <template v-else-if="submission">
        <div class="result-hero" :class="{ accepted: submission.status === 'ACCEPTED' }">
          <div>
            <p class="eyebrow">Submission #{{ submission.id }}</p>
            <h1>{{ submissionStatusLabel(submission.status) }}</h1>
            <p v-if="!isFinalSubmissionStatus(submission.status)" class="polling-hint">
              <span class="pulse-dot"></span> 正在等待判题结果，页面将自动刷新
            </p>
          </div>
          <ElTag :type="statusTagType(submission.status)" size="large" effect="dark">
            {{ submission.status }}
          </ElTag>
        </div>

        <ElDescriptions :column="2" border class="result-details">
          <ElDescriptionsItem label="题目">{{ submission.problemAlias }}</ElDescriptionsItem>
          <ElDescriptionsItem label="语言">{{
            languageLabel(submission.language)
          }}</ElDescriptionsItem>
          <ElDescriptionsItem label="提交时间">{{
            formatDateTime(submission.submittedAt)
          }}</ElDescriptionsItem>
          <ElDescriptionsItem label="判题完成">{{
            formatDateTime(submission.judgedAt)
          }}</ElDescriptionsItem>
        </ElDescriptions>

        <ElCard v-if="activeJudgement" shadow="never" class="judgement-card">
          <template #header>
            <div class="card-header">
              <strong>判题详情</strong>
              <span class="muted-note">Judgement {{ activeJudgement.id }}</span>
            </div>
          </template>
          <div class="metric-grid">
            <div>
              <small>总时间</small>
              <strong>{{ formatMetric(activeJudgement.totalTimeMs, 'ms') }}</strong>
            </div>
            <div>
              <small>峰值内存</small>
              <strong>{{ formatMemory(activeJudgement.peakMemoryKb) }}</strong>
            </div>
            <div>
              <small>开始判题</small>
              <strong>{{ formatDateTime(activeJudgement.startedAt) }}</strong>
            </div>
            <div>
              <small>完成判题</small>
              <strong>{{ formatDateTime(activeJudgement.completedAt) }}</strong>
            </div>
          </div>

          <template v-if="activeJudgement.compileLog">
            <h3>编译日志</h3>
            <pre><code>{{ activeJudgement.compileLog }}</code></pre>
          </template>

          <template v-if="activeJudgement.runs.length">
            <h3>测试点</h3>
            <ElTable :data="activeJudgement.runs" size="small" border>
              <ElTableColumn prop="testIndex" label="#" width="70" />
              <ElTableColumn label="结果" width="170">
                <template #default="{ row }">{{
                  submissionStatusLabel(row.verdict ?? '')
                }}</template>
              </ElTableColumn>
              <ElTableColumn label="时间" width="110">
                <template #default="{ row }">{{ formatMetric(row.timeMs, 'ms') }}</template>
              </ElTableColumn>
              <ElTableColumn label="内存" width="130">
                <template #default="{ row }">{{ formatMemory(row.memoryKb) }}</template>
              </ElTableColumn>
              <ElTableColumn prop="exitCode" label="退出码" width="90" />
              <ElTableColumn label="标准错误">
                <template #default="{ row }">{{ row.stderrTail || '—' }}</template>
              </ElTableColumn>
            </ElTable>
          </template>
        </ElCard>

        <ElCard v-if="submission.judgements.length > 1" shadow="never" class="history-card">
          <template #header><strong>重判历史</strong></template>
          <ElTimeline>
            <ElTimelineItem
              v-for="judgement in submission.judgements"
              :key="judgement.id"
              :timestamp="formatDateTime(judgement.createdAt)"
              placement="top"
            >
              {{ judgement.active ? '当前判题' : '已被后续重判替代' }}
              <ElTag v-if="judgement.verdict" size="small" :type="statusTagType(judgement.verdict)">
                {{ submissionStatusLabel(judgement.verdict) }}
              </ElTag>
            </ElTimelineItem>
          </ElTimeline>
        </ElCard>

        <ElCard shadow="never" class="source-card">
          <template #header>
            <div class="card-header">
              <strong>提交源码</strong>
              <span class="muted-note">SHA-256 {{ submission.sourceSha256 ?? '—' }}</span>
            </div>
          </template>
          <ElEmpty
            v-if="submission.language === 'output'"
            description="输出题提交为 ZIP 归档，不支持在线查看"
          /><CodeEditor
            v-else
            v-model="submission.source"
            :language="submission.language"
            readonly
            height="420px"
            class="source-editor"
          />
        </ElCard>
      </template>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ArrowLeft } from '@element-plus/icons-vue';
import CodeEditor from '../components/CodeEditor.vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { SubmissionDetail } from '../api/types';
import {
  formatDateTime,
  isFinalSubmissionStatus,
  languageLabel,
  statusTagType,
  submissionStatusLabel,
} from '../utils/format';

const route = useRoute();
const router = useRouter();
const contestId = computed(() => Number(route.params.contestId));
const submissionId = computed(() => Number(route.params.submissionId));
const submission = ref<SubmissionDetail | null>(null);
const loading = ref(false);
const errorMessage = ref('');
let pollTimer: number | undefined;
let failures = 0;

const activeJudgement = computed(
  () => submission.value?.judgements.find((judgement) => judgement.active) ?? null,
);

function formatMetric(value: number | null, unit: string) {
  return value === null ? '—' : `${value} ${unit}`;
}

function formatMemory(value: number | null) {
  return value === null ? '—' : `${(value / 1024).toFixed(1)} MiB`;
}

async function loadSubmission() {
  if (pollTimer) window.clearTimeout(pollTimer);
  pollTimer = undefined;
  loading.value = true;
  try {
    submission.value = await contestApi.getSubmission(contestId.value, submissionId.value);
    errorMessage.value = '';
    failures = 0;
    if (!isFinalSubmissionStatus(submission.value.status) && !document.hidden) {
      pollTimer = window.setTimeout(loadSubmission, 2_000);
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
    failures += 1;
    pollTimer = window.setTimeout(loadSubmission, Math.min(2_000 * 2 ** failures, 15_000));
  } finally {
    loading.value = false;
  }
}

function handleVisibility() {
  if (!document.hidden && submission.value && !isFinalSubmissionStatus(submission.value.status)) {
    if (pollTimer) window.clearTimeout(pollTimer);
    void loadSubmission();
  }
}

watch(
  [contestId, submissionId],
  () => {
    submission.value = null;
    void loadSubmission();
  },
  { immediate: true },
);

onMounted(() => document.addEventListener('visibilitychange', handleVisibility));

onUnmounted(() => {
  if (pollTimer) window.clearTimeout(pollTimer);
  document.removeEventListener('visibilitychange', handleVisibility);
});
</script>

<style scoped>
.page-section {
  min-height: 50vh;
}

.narrow-section {
  max-width: 940px;
  margin: 0 auto;
}

.page-body {
  padding: 0;
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

.page-alert {
  margin-bottom: 20px;
}

.result-hero {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  margin: 24px 0;
  border: 1px solid #f5d58d;
  border-radius: 0;
  padding: 30px;
  background: #fffbeb;
}

.result-hero.accepted {
  border-color: #a7e3c0;
  background: #ecfdf5;
}

.result-hero h1 {
  margin-bottom: 8px;
  font-size: 38px;
}

.polling-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  color: var(--muted);
}

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #f59e0b;
  box-shadow: 0 0 0 0 rgb(245 158 11 / 55%);
  animation: pulse 1.6s infinite;
}

@keyframes pulse {
  70% {
    box-shadow: 0 0 0 8px rgb(245 158 11 / 0%);
  }
  100% {
    box-shadow: 0 0 0 0 rgb(245 158 11 / 0%);
  }
}

.result-details {
  margin-bottom: 24px;
}

.source-card {
  border-radius: 0;
}

.judgement-card,
.history-card {
  margin-bottom: 24px;
  border-radius: 0;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
  margin-bottom: 24px;
}

.metric-grid div {
  border-radius: 0;
  padding: 15px;
  background: #f8fafc;
}

.metric-grid small,
.metric-grid strong {
  display: block;
}

.metric-grid small {
  margin-bottom: 7px;
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

.muted-note {
  margin: 12px 0 0;
  color: var(--muted);
  font-size: 12px;
}

.markdown-body pre {
  overflow-x: auto;
  border-radius: 0;
  padding: 18px;
  color: #dbeafe;
  background: #101827;
  font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', monospace;
  font-size: 13px;
  line-height: 1.65;
}

.source-editor {
  margin-top: 4px;
  border: 0;
}

@media (max-width: 640px) {
  .result-hero {
    align-items: flex-start;
    flex-direction: column;
    padding: 24px;
  }

  .metric-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .result-details :deep(.el-descriptions__body .el-descriptions__table) {
    display: block;
  }
}
</style>
