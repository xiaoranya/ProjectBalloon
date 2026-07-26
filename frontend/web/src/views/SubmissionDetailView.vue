<template>
  <section class="page-section narrow-section">
    <ElButton link :icon="ArrowLeft" @click="router.push(`/contests/${contestId}/submissions`)">返回提交记录</ElButton>
    <ElSkeleton v-if="loading && !submission" :rows="6" animated />
    <ElAlert v-else-if="errorMessage && !submission" :title="errorMessage" type="error" show-icon :closable="false" />
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
        <ElDescriptionsItem label="语言">{{ languageLabel(submission.language) }}</ElDescriptionsItem>
        <ElDescriptionsItem label="提交时间">{{ formatDateTime(submission.submittedAt) }}</ElDescriptionsItem>
        <ElDescriptionsItem label="判题完成">{{ formatDateTime(submission.judgedAt) }}</ElDescriptionsItem>
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
              <template #default="{ row }">{{ submissionStatusLabel(row.verdict ?? '') }}</template>
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
        <pre><code>{{ submission.source }}</code></pre>
      </ElCard>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ArrowLeft } from '@element-plus/icons-vue';
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
const contestId = Number(route.params.contestId);
const submissionId = Number(route.params.submissionId);
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
    submission.value = await contestApi.getSubmission(contestId, submissionId);
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

onMounted(() => {
  void loadSubmission();
  document.addEventListener('visibilitychange', handleVisibility);
});

onUnmounted(() => {
  if (pollTimer) window.clearTimeout(pollTimer);
  document.removeEventListener('visibilitychange', handleVisibility);
});
</script>
