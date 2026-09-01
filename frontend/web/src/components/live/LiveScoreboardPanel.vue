<template>
  <section class="board" :class="`board-${template.toLowerCase()}`">
    <table>
      <thead>
        <tr>
          <th>#</th>
          <th>{{ t('队伍') }}</th>
          <th>{{ scoreboard.scoringMode === 'ICPC' ? t('解题') : t('总分') }}</th>
          <th v-if="scoreboard.scoringMode === 'ICPC'">{{ t('罚时') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in scoreboard.rows.slice(0, rowLimit)" :key="row.teamId">
          <td>{{ row.rank }}</td>
          <td>
            <strong>{{ row.teamName }}</strong
            ><small>{{ row.school }}</small>
          </td>
          <td>
            {{
              scoreboard.scoringMode === 'ICPC'
                ? row.solvedCount
                : (row.totalScoreMilli / 1000).toFixed(3).replace(/\.?0+$/, '')
            }}
          </td>
          <td v-if="scoreboard.scoringMode === 'ICPC'">{{ row.penaltyMinutes }}</td>
        </tr>
      </tbody>
    </table>
    <aside v-if="announcement">
      <strong>{{ announcement.title }}</strong>
      <p>{{ announcement.body }}</p>
    </aside>
  </section>
</template>

<script setup lang="ts">
import type { ScoreboardResponse } from '../../api/types';
import { useI18n } from '../../i18n';

withDefaults(
  defineProps<{
    scoreboard: ScoreboardResponse;
    rowLimit: number;
    announcement?: { title: string; body: string } | null;
    template?: string;
  }>(),
  { announcement: null, template: 'DEFAULT' },
);
const { t } = useI18n();
</script>

<style scoped>
.board {
  display: grid;
  grid-template-columns: 1fr 320px;
  gap: 24px;
}
.board table {
  width: 100%;
  border-collapse: collapse;
  font-size: clamp(18px, 2vw, 30px);
}
th,
td {
  padding: var(--custom-density, 14px);
  border-bottom: 1px solid #203047;
  text-align: left;
}
.board small {
  display: block;
  color: var(--muted-light);
}
.board aside {
  background: #101d2d;
  border-left: 4px solid var(--accent, #22c55e);
  padding: 24px;
}
.board-minimal aside {
  background: #f2f4f7;
  color: #101828;
  box-shadow: none;
}
.board-minimal small {
  color: #667085;
}
.board-custom aside {
  background: rgb(255 255 255 / 8%);
  background: color-mix(
    in srgb,
    var(--custom-foreground, #fff) 8%,
    var(--custom-background, #07111f)
  );
}
.board-split {
  grid-template-columns: 1.4fr 0.8fr;
}
.board-split aside {
  border-left-width: 8px;
}
@media (max-width: 800px) {
  .board,
  .board-split {
    grid-template-columns: 1fr;
  }
}
</style>
