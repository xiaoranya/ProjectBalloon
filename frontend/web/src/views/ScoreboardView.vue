<template>
  <el-container direction="vertical" class="page-section scoreboard-section">
    <el-header height="auto" class="page-head">
      <div class="page-title-row">
        <div>
          <h1>{{ t('比赛榜单') }}</h1>
          <p>
            {{ t(scoreboard?.frozen ? '榜单已封榜，显示公开数据。' : '榜单自动刷新。') }}
            <span v-if="scoreboard">{{
              t('更新于 {time}', { time: formatDateTime(scoreboard.generatedAt) })
            }}</span>
          </p>
        </div>
        <ElButton :icon="Refresh" :loading="loading" @click="loadScoreboard()">{{
          t('刷新')
        }}</ElButton>
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

      <div v-loading="loading" class="scoreboard-wrap">
        <table v-if="scoreboard" class="scoreboard-table">
          <thead>
            <tr>
              <th>{{ t('排名') }}</th>
              <th class="team-column">{{ t('队伍') }}</th>
              <th>{{ t(scoreboard.scoringMode === 'ICPC' ? '解题' : '总分') }}</th>
              <th v-if="scoreboard.scoringMode === 'ICPC'">{{ t('罚时') }}</th>
              <th v-for="problem in scoreboard.problems" :key="problem.problemId">
                {{ problem.alias }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in scoreboard.rows" :key="row.teamId">
              <td class="rank-cell">{{ row.rank || '—' }}</td>
              <td class="team-column">
                <strong
                  >{{ row.teamName }}
                  <ElTag v-if="row.isStar" size="small" type="warning">{{
                    t('打星')
                  }}</ElTag></strong
                >
                <small v-if="row.school">{{ row.school }}</small>
              </td>
              <td>
                <strong>{{
                  scoreboard.scoringMode === 'ICPC' ? row.solvedCount : points(row.totalScoreMilli)
                }}</strong>
              </td>
              <td v-if="scoreboard.scoringMode === 'ICPC'">{{ row.penaltyMinutes }}</td>
              <td v-for="cell in orderedCells(row.problems)" :key="cell.problemId">
                <div
                  class="score-cell"
                  :class="{
                    solved: cell.solved,
                    attempted: !cell.solved && cell.wrongAttempts > 0,
                    first: cell.firstBlood,
                  }"
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
        <ElEmpty v-else-if="!loading" :description="t('暂无榜单数据')" />
      </div>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRoute } from 'vue-router';
import { Refresh } from '@element-plus/icons-vue';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { ScoreboardResponse, ScoreboardCell } from '../api/types';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const { t } = useI18n();
const contestId = computed(() => Number(route.params.contestId));
const scoreboard = ref<ScoreboardResponse | null>(null);
const loading = ref(false);
const errorMessage = ref('');
let timer: number | undefined;
let requestGeneration = 0;

async function loadScoreboard(silent = false) {
  const generation = ++requestGeneration;
  if (!silent) loading.value = true;
  try {
    const result = await contestApi.getScoreboard(contestId.value);
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
  return scoreboard.value.problems.map(
    (problem) =>
      byProblem.get(problem.problemId) ?? {
        problemId: problem.problemId,
        wrongAttempts: 0,
        solved: false,
        solvedAt: null,
        penaltyMinutes: 0,
        scoreMilli: 0,
        firstBlood: false,
      },
  );
}

function points(scoreMilli: number): string {
  return (scoreMilli / 1000).toLocaleString(undefined, { maximumFractionDigits: 3 });
}

onMounted(() => {
  timer = window.setInterval(() => {
    if (!document.hidden) void loadScoreboard(true);
  }, 15_000);
});

watch(
  contestId,
  () => {
    scoreboard.value = null;
    void loadScoreboard();
  },
  { immediate: true },
);

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<style scoped>
.page-section {
  min-height: 50vh;
}

.page-head {
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

.scoreboard-wrap {
  /* 表格列数随题目数增长，横向滚动必须保留；纵向也限制在视口内，
     让 thead 的 position: sticky 相对本滚动容器吸附。 */
  max-height: calc(100vh - 160px);
  overflow: auto;
  border: 1px solid var(--border);

  background: white;
}

.scoreboard-table {
  width: 100%;
  min-width: 900px;
  border-spacing: 0;
  border-collapse: separate;
}

.scoreboard-table th,
.scoreboard-table td {
  min-width: 74px;
  padding: 12px 10px;
  border-right: 1px solid #edf1f6;
  border-bottom: 1px solid #edf1f6;
  text-align: center;
}

.scoreboard-table thead th {
  position: sticky;
  z-index: 2;
  top: 0;
  color: #526176;
  background: #f8fafc;
  font-size: 12px;
  letter-spacing: 0.04em;
}

.scoreboard-table tbody tr:last-child td {
  border-bottom: 0;
}

.scoreboard-table .team-column {
  min-width: 220px;
  text-align: left;
}

.team-column strong,
.team-column small {
  display: block;
}

.team-column small {
  margin-top: 4px;
  color: var(--muted);
}

.rank-cell {
  font-size: 18px;
  font-weight: 800;
}

.score-cell {
  display: grid;
  min-height: 46px;
  place-content: center;

  color: var(--muted-light);
}

.score-cell.solved {
  color: #08783f;
  background: #dcfce7;
}

.score-cell.attempted {
  color: #b42318;
  background: #fee2e2;
}

.score-cell.first {
  outline: 2px solid #f59e0b;
}

.score-cell small {
  margin-top: 2px;
  font-size: 10px;
}

@media (max-width: 640px) {
  .page-title-row {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
