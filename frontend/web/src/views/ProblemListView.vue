<template>
  <el-container direction="vertical" class="page-section">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <p class="eyebrow">Problems</p>
          <h1>题目列表</h1>
        </div>
        <ElButton :icon="Refresh" :loading="loading" @click="loadProblems">刷新</ElButton>
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
    </el-main>
  </el-container>
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

watch(
  contestId,
  () => {
    problems.value = [];
    void loadProblems();
  },
  { immediate: true },
);
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

.page-title-row p {
  display: none;
  margin-bottom: 0;
  color: var(--muted);
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

.problem-grid {
  display: grid;
  width: 100%;
  max-width: 1180px;
  grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
  gap: 22px;
  margin: 0 auto;
}

.problem-card {
  display: flex;
  min-height: 116px;
  align-items: center;
  gap: 20px;
  padding: 20px;
  border: 1px solid var(--border);
  border-radius: 0;
  background: white;
  cursor: pointer;
  transition:
    transform 160ms ease,
    border-color 160ms ease,
    box-shadow 160ms ease;
}

.problem-card:hover,
.problem-card:focus-visible {
  border-color: #93b4f5;
  outline: none;
  box-shadow: 0 16px 40px rgb(37 99 235 / 10%);
  transform: translateY(-2px);
}

.problem-alias {
  display: grid;
  width: 58px;
  height: 58px;
  flex: 0 0 auto;
  place-items: center;
  border: 3px solid var(--problem-color);
  border-radius: 0;
  color: var(--problem-color);
  background: color-mix(in srgb, var(--problem-color) 9%, white);
  font-size: 24px;
  font-weight: 900;
}

.problem-card-body {
  min-width: 0;
  flex: 1;
}

.problem-card h2 {
  overflow: hidden;
  margin-bottom: 7px;
  font-size: 18px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.problem-card p {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }

  .problem-grid {
    grid-template-columns: 1fr;
  }

  .problem-card {
    gap: 14px;
  }

  .problem-alias {
    width: 48px;
    height: 48px;
    border-radius: 0;
    font-size: 20px;
  }
}
</style>
