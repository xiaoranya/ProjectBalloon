<template>
  <ElCard shadow="never">
    <ElTable
      v-loading="loading"
      :data="tasks"
      row-key="id"
      :empty-text="loaded ? t('当前筛选下暂无气球任务') : t('气球任务加载失败，请重试')"
      @row-click="emit('open-detail', $event)"
    >
      <ElTableColumn :label="t('队伍与座位')" min-width="220">
        <template #default="{ row }">
          <div class="admin-primary-cell">
            <strong>{{ row.teamName }}</strong
            ><small>{{
              t('座位 {seat} · 任务 #{id}', { seat: row.seatNo ?? '—', id: row.id })
            }}</small>
          </div>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('题目与颜色')" width="180">
        <template #default="{ row }">
          <div class="balloon-problem-cell">
            <span class="balloon-color" :style="{ backgroundColor: row.color }" />
            <div>
              <strong>{{ row.problemAlias }}</strong
              ><small>{{ row.color }}</small>
            </div>
          </div>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('标记')" width="130">
        <template #default="{ row }"
          ><ElTag v-if="row.isFirstBlood" type="danger" effect="dark">First Blood</ElTag
          ><span v-else class="muted-text">{{ t('普通任务') }}</span></template
        >
      </ElTableColumn>
      <ElTableColumn :label="t('状态')" width="110">
        <template #default="{ row }"
          ><ElTag :type="statusType(row.status)">{{ statusLabel(t, row.status) }}</ElTag></template
        >
      </ElTableColumn>
      <ElTableColumn :label="t('领取信息')" min-width="180">
        <template #default="{ row }">
          <div class="admin-primary-cell">
            <span>{{
              row.claimedByUserId ? t('工作人员 #{id}', { id: row.claimedByUserId }) : t('尚未领取')
            }}</span
            ><small>{{ formatDateTime(row.claimedAt) }}</small>
          </div>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('生成时间')" min-width="170">
        <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
      </ElTableColumn>
    </ElTable>
  </ElCard>
</template>

<script setup lang="ts">
import type { BalloonTask } from '../../api/balloons';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';
import { statusLabel, statusType } from './status';

defineProps<{
  tasks: BalloonTask[];
  loading: boolean;
  loaded: boolean;
}>();
const emit = defineEmits<{
  'open-detail': [task: BalloonTask];
}>();
const { t } = useI18n();
</script>

<style scoped>
.balloon-problem-cell {
  display: flex;
  align-items: center;
  gap: 10px;
}
.balloon-problem-cell > div {
  display: grid;
  gap: 2px;
}
.balloon-problem-cell small {
  color: var(--muted);
}
.muted-text {
  color: var(--muted);
}
</style>
