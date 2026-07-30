<template>
  <section class="page-section">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Problems</p>
        <h1>题目列表</h1>
        <p>选择题目查看题面并提交代码。</p>
      </div>
      <ElButton :icon="Refresh" :loading="loading" @click="loadProblems">刷新</ElButton>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <div v-loading="loading" class="problem-grid">
      <ElEmpty v-if="!loading && problems.length === 0" description="当前比赛还没有题目" />
      <article
        v-for="entry in problems"
        :key="entry.problemId"
        class="problem-card"
        tabindex="0"
        @click="openProblem(entry.problemId)"
        @keyup.enter="openProblem(entry.problemId)"
      >
        <div class="problem-alias" :style="{ '--problem-color': entry.color || '#2563eb' }">
          {{ entry.alias }}
        </div>
        <div class="problem-card-body">
          <h2>{{ entry.title }}</h2>
          <p>
            {{ entry.timeLimitMs }} ms · {{ entry.memoryLimitMb }} MB ·
            {{ entry.languages.map(languageLabel).join(' / ') }}
          </p>
        </div>
        <ElIcon><ArrowRight /></ElIcon>
      </article>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { Refresh } from '@element-plus/icons-vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { ContestProblem } from '../api/types';
import { languageLabel } from '../utils/format';

const route = useRoute();
const router = useRouter();
const problems = ref<ContestProblem[]>([]);
const loading = ref(false);
const errorMessage = ref('');
const contestId = computed(() => Number(route.params.contestId));

async function loadProblems() {
  loading.value = true;
  errorMessage.value = '';
  try {
    problems.value = await contestApi.listProblems(contestId.value);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function openProblem(problemId: number) {
  void router.push(`/contests/${contestId.value}/problems/${problemId}`);
}

watch(contestId, () => {
  problems.value = [];
  void loadProblems();
}, { immediate: true });
</script>
