<template>
  <section class="page-section scoreboard-section">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Scoreboard</p>
        <h1>比赛榜单</h1>
        <p>
          {{ scoreboard?.frozen ? '榜单已封榜，显示公开数据。' : '榜单自动刷新。' }}
          <span v-if="scoreboard">更新于 {{ formatDateTime(scoreboard.generatedAt) }}</span>
        </p>
      </div>
      <ElButton :icon="Refresh" :loading="loading" @click="loadScoreboard()">刷新</ElButton>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <div v-loading="loading" class="scoreboard-wrap">
      <table v-if="scoreboard" class="scoreboard-table">
        <thead>
          <tr>
            <th>排名</th>
            <th class="team-column">队伍</th>
            <th>{{ scoreboard.scoringMode === 'ICPC' ? '解题' : '总分' }}</th>
            <th v-if="scoreboard.scoringMode === 'ICPC'">罚时</th>
            <th v-for="problem in scoreboard.problems" :key="problem.problemId">
              {{ problem.alias }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in scoreboard.rows" :key="row.teamId">
            <td class="rank-cell">{{ row.rank || '—' }}</td>
            <td class="team-column">
              <strong>{{ row.teamName }} <ElTag v-if="row.isStar" size="small" type="warning">打星</ElTag></strong>
              <small v-if="row.school">{{ row.school }}</small>
            </td>
            <td><strong>{{ scoreboard.scoringMode === 'ICPC' ? row.solvedCount : points(row.totalScoreMilli) }}</strong></td>
            <td v-if="scoreboard.scoringMode === 'ICPC'">{{ row.penaltyMinutes }}</td>
            <td v-for="cell in orderedCells(row.problems)" :key="cell.problemId">
              <div
                class="score-cell"
                :class="{ solved: cell.solved, attempted: !cell.solved && cell.wrongAttempts > 0, first: cell.firstBlood }"
              >
                <template v-if="scoreboard.scoringMode === 'ICPC'">
                  <strong v-if="cell.solved">+{{ cell.wrongAttempts || '' }}</strong>
                  <strong v-else-if="cell.wrongAttempts">-{{ cell.wrongAttempts }}</strong>
                  <span v-else>·</span>
                  <small v-if="cell.solved">{{ cell.penaltyMinutes }}</small>
                </template>
                <strong v-else>{{ points(cell.scoreMilli) }}</strong>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
      <ElEmpty v-else-if="!loading" description="暂无榜单数据" />
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { useRoute } from 'vue-router';
import { Refresh } from '@element-plus/icons-vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Scoreboard, ScoreboardCell } from '../api/types';
import { formatDateTime } from '../utils/format';

const route = useRoute();
const contestId = Number(route.params.contestId);
const scoreboard = ref<Scoreboard | null>(null);
const loading = ref(false);
const errorMessage = ref('');
let timer: number | undefined;
let requestGeneration = 0;

async function loadScoreboard(silent = false) {
  const generation = ++requestGeneration;
  if (!silent) loading.value = true;
  try {
    const result = await contestApi.getScoreboard(contestId);
    if (generation !== requestGeneration) return;
    scoreboard.value = result;
    errorMessage.value = '';
  } catch (error) {
    if (generation === requestGeneration && !silent) errorMessage.value = getErrorMessage(error);
  } finally {
    if (generation === requestGeneration) loading.value = false;
  }
}

function orderedCells(cells: ScoreboardCell[]): ScoreboardCell[] {
  if (!scoreboard.value) return cells;
  const byProblem = new Map(cells.map((cell) => [cell.problemId, cell]));
  return scoreboard.value.problems.map((problem) => byProblem.get(problem.problemId) ?? {
    problemId: problem.problemId,
    wrongAttempts: 0,
    solved: false,
    solvedAt: null,
    penaltyMinutes: 0,
    scoreMilli: 0,
    firstBlood: false,
  });
}

function points(scoreMilli: number): string {
  return (scoreMilli / 1000).toLocaleString(undefined, { maximumFractionDigits: 3 });
}

onMounted(async () => {
  await loadScoreboard();
  timer = window.setInterval(() => {
    if (!document.hidden) void loadScoreboard(true);
  }, 15_000);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>
