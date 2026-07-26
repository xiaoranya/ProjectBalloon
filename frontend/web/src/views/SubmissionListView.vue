<template>
  <section class="page-section">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Submissions</p>
        <h1>提交记录</h1>
        <p>未完成的提交会自动刷新判题状态。</p>
      </div>
      <ElButton :icon="Refresh" :loading="loading" @click="loadSubmissions()">刷新</ElButton>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <ElTable v-loading="loading" :data="submissions" class="data-table" empty-text="暂无提交记录" @row-click="openSubmission">
      <ElTableColumn prop="id" label="#" width="90" />
      <ElTableColumn prop="problemAlias" label="题目" width="100" />
      <ElTableColumn label="语言" width="120">
        <template #default="{ row }">{{ languageLabel(row.language) }}</template>
      </ElTableColumn>
      <ElTableColumn label="状态" min-width="170">
        <template #default="{ row }">
          <ElTag :type="statusTagType(row.status)" effect="light">
            {{ submissionStatusLabel(row.status) }}
          </ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn label="提交时间" min-width="180">
        <template #default="{ row }">{{ formatDateTime(row.submittedAt) }}</template>
      </ElTableColumn>
      <ElTableColumn label="判题完成" min-width="180">
        <template #default="{ row }">{{ formatDateTime(row.judgedAt) }}</template>
      </ElTableColumn>
    </ElTable>

    <div v-if="page.totalPages > 1" class="pagination-row">
      <ElPagination
        v-model:current-page="currentPage"
        :page-size="page.size"
        :total="page.totalElements"
        layout="prev, pager, next"
        @current-change="loadSubmissions()"
      />
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { Refresh } from '@element-plus/icons-vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { PageResponse, SubmissionSummary } from '../api/types';
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
const submissions = ref<SubmissionSummary[]>([]);
const loading = ref(false);
const errorMessage = ref('');
const currentPage = ref(1);
const page = ref<PageResponse<SubmissionSummary>>({
  content: [],
  page: 0,
  size: 30,
  totalElements: 0,
  totalPages: 0,
});
let pollTimer: number | undefined;
let requestGeneration = 0;

async function loadSubmissions(silent = false) {
  const generation = ++requestGeneration;
  if (!silent) loading.value = true;
  errorMessage.value = '';
  try {
    const result = await contestApi.listSubmissions(contestId, currentPage.value - 1);
    if (generation !== requestGeneration) return;
    page.value = result;
    submissions.value = result.content;
    schedulePolling();
  } catch (error) {
    if (generation === requestGeneration) {
      if (!silent) errorMessage.value = getErrorMessage(error);
      schedulePolling(8_000);
    }
  } finally {
    if (generation === requestGeneration) loading.value = false;
  }
}

function schedulePolling(delay = 4_000) {
  if (pollTimer) window.clearTimeout(pollTimer);
  if (document.hidden || !submissions.value.some((item) => !isFinalSubmissionStatus(item.status))) return;
  pollTimer = window.setTimeout(() => void loadSubmissions(true), delay);
}

function handleVisibility() {
  if (!document.hidden) void loadSubmissions(true);
}

function openSubmission(row: SubmissionSummary) {
  void router.push(`/contests/${contestId}/submissions/${row.id}`);
}

onMounted(() => {
  void loadSubmissions();
  document.addEventListener('visibilitychange', handleVisibility);
});

onUnmounted(() => {
  if (pollTimer) window.clearTimeout(pollTimer);
  document.removeEventListener('visibilitychange', handleVisibility);
});
</script>
