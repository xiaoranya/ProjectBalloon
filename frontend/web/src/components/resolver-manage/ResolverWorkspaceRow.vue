<template>
  <div>
    <ElRow :gutter="18" class="resolver-workspace">
      <ElCol :xs="24" :md="15">
        <ElCard shadow="never" class="resolver-focus-card">
          <template #header>
            <div class="card-header">
              <div>
                <strong>{{ t('最近揭晓') }}</strong
                ><small>{{
                  t('步骤 {current} / {total}', {
                    current: run.currentStep,
                    total: run.totalSteps,
                  })
                }}</small>
              </div>
            </div>
          </template>
          <div v-if="focusRow && run.state.lastReveal" class="resolver-focus">
            <div class="resolver-team-identity">
              <span class="resolver-rank">#{{ focusRow.rank }}</span>
              <div>
                <h2>{{ focusRow.teamName }}</h2>
                <p>{{ focusRow.school ?? t('学校未填写') }}</p>
              </div>
            </div>
            <div class="resolver-reveal-grid">
              <div>
                <span>{{ t('题目') }}</span
                ><strong>{{ focusProblem?.alias ?? run.state.lastReveal.problemId }}</strong>
              </div>
              <div>
                <span>{{ t('结果') }}</span
                ><strong :class="run.state.lastReveal.after.solved ? 'accepted' : 'rejected'">{{
                  run.state.lastReveal.after.solved ? 'ACCEPTED' : t('未通过')
                }}</strong>
              </div>
              <div>
                <span>{{ t('当前成绩') }}</span
                ><strong>{{
                  t('{solved} 题 / {minutes} 分钟', {
                    solved: focusRow.solvedCount,
                    minutes: focusRow.penaltyMinutes,
                  })
                }}</strong>
              </div>
            </div>
          </div>
          <ElEmpty v-else :description="t('尚未揭晓步骤')" />
        </ElCard>
      </ElCol>
      <ElCol :xs="24" :md="9">
        <ElCard shadow="never" class="resolver-history-card">
          <template #header>
            <div class="card-header">
              <div>
                <strong>{{ t('操作历史') }}</strong
                ><small>{{ t('最近 {count} 条', { count: events.length }) }}</small>
              </div>
            </div>
          </template>
          <div class="resolver-event-list">
            <article v-for="event in events.slice(-12).reverse()" :key="event.id">
              <span>{{ event.sequence }}</span>
              <div>
                <strong>{{ eventLabel(event.eventType) }}</strong
                ><small>{{ formatDateTime(event.createdAt) }}</small>
              </div>
            </article>
          </div>
        </ElCard>
      </ElCol>
    </ElRow>

    <ElCard shadow="never" class="resolver-table-card">
      <template #header>
        <div class="card-header">
          <div>
            <strong>{{ t('当前 Resolver 榜单') }}</strong
            ><small>{{ t('状态数据由后端快照校验恢复') }}</small>
          </div>
        </div>
      </template>
      <ElTable :data="run.state.board.rows" row-key="teamId" stripe max-height="560">
        <ElTableColumn prop="rank" :label="t('排名')" width="80" />
        <ElTableColumn :label="t('队伍')" min-width="220">
          <template #default="{ row }">
            <div class="resolver-table-team">
              <strong>{{ row.teamName }}</strong
              ><span>{{ row.school ?? '—' }}</span>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="solvedCount" :label="t('解题')" width="80" />
        <ElTableColumn prop="penaltyMinutes" :label="t('罚时')" width="90" />
        <ElTableColumn :label="t('题目状态')" min-width="300">
          <template #default="{ row }">
            <div class="resolver-cell-list">
              <span
                v-for="cell in row.problems"
                :key="cell.problemId"
                :class="{
                  solved: cell.solved,
                  attempted: !cell.solved && cell.wrongAttempts > 0,
                }"
                >{{ problemAlias(cell.problemId) }}
                {{
                  cell.solved
                    ? `+${cell.wrongAttempts || ''}`
                    : cell.wrongAttempts
                      ? `-${cell.wrongAttempts}`
                      : '·'
                }}</span
              >
            </div>
          </template>
        </ElTableColumn>
      </ElTable>
    </ElCard>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { ResolverEvent, ResolverRun } from '../../api/resolver';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';

const props = defineProps<{
  run: ResolverRun;
  events: ResolverEvent[];
}>();
const { t } = useI18n();

const focusRow = computed(
  () =>
    props.run.state.board.rows.find((row) => row.teamId === props.run.state.lastReveal?.teamId) ??
    null,
);
const focusProblem = computed(
  () =>
    props.run.state.board.problems.find(
      (problem) => problem.problemId === props.run.state.lastReveal?.problemId,
    ) ?? null,
);
function eventLabel(type: string) {
  const labels: Record<string, string> = {
    CREATED: '创建运行',
    START: '开始',
    NEXT: '揭晓下一步',
    PREVIOUS: '回退',
    PAUSE: '暂停',
    RESUME: '恢复',
    COMPLETE: '完成',
    AUTO_PLAY: '自动播放设置',
    AUTO_NEXT: '自动揭晓',
  };
  const label = labels[type];
  return label ? t(label) : type;
}
function problemAlias(id: number) {
  return (
    props.run.state.board.problems.find((problem) => problem.problemId === id)?.alias ?? String(id)
  );
}
</script>

<style scoped>
.resolver-workspace {
  margin-bottom: 18px;
}
.resolver-workspace > .el-col {
  align-self: flex-start;
}
.resolver-focus-card,
.resolver-history-card,
.resolver-table-card {
  border: 1px solid #e4e9f0;
}
.resolver-focus {
  min-height: 284px;
}
.resolver-team-identity {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 24px;
}
.resolver-rank {
  display: grid;
  width: 76px;
  height: 76px;
  place-items: center;

  color: #fff;
  background: #17233d;
  font-size: 25px;
  font-weight: 800;
}
.resolver-team-identity h2 {
  margin: 0 0 5px;
  color: #172033;
  font-size: 27px;
}
.resolver-team-identity p {
  margin: 0;
  color: var(--muted);
}
.resolver-reveal-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}
.resolver-reveal-grid > div {
  padding: 14px;

  background: #f5f7fa;
}
.resolver-reveal-grid span,
.resolver-reveal-grid strong {
  display: block;
}
.resolver-reveal-grid span {
  margin-bottom: 6px;
  color: var(--muted);
  font-size: 11px;
}
.resolver-reveal-grid strong {
  color: #172033;
  font-size: 15px;
}
.resolver-reveal-grid strong.accepted {
  color: #047857;
}
.resolver-reveal-grid strong.rejected {
  color: #b91c1c;
}
.resolver-event-list {
  display: grid;
  gap: 8px;
  max-height: 310px;
  overflow: auto;
}
.resolver-event-list article {
  display: grid;
  grid-template-columns: 30px minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 10px;
  border: 1px solid #edf0f4;
}
.resolver-event-list article > span:first-child {
  color: var(--muted);
  text-align: center;
  font-weight: 700;
}
.resolver-event-list strong,
.resolver-event-list small,
.resolver-table-team strong,
.resolver-table-team span {
  display: block;
}
.resolver-event-list small,
.resolver-table-team span {
  margin-top: 3px;
  color: var(--muted);
  font-size: 11px;
}
.resolver-table-card {
  margin-bottom: 24px;
}
.resolver-cell-list {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 5px;
}
.resolver-cell-list span {
  min-width: 43px;
  padding: 4px 6px;

  color: var(--muted);
  background: #f1f5f9;
  text-align: center;
  font-size: 11px;
}
.resolver-cell-list span.solved {
  color: #fff;
  background: #059669;
}
.resolver-cell-list span.attempted {
  color: #fff;
  background: #dc2626;
}
@media (max-width: 1100px) {
  .resolver-workspace > .el-col {
    flex: 0 0 100%;
    max-width: 100%;
  }
  .resolver-reveal-grid {
    grid-template-columns: 1fr 1fr;
  }
}
</style>
