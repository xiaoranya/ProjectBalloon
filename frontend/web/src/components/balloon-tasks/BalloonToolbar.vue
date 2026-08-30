<template>
  <ElCard shadow="never" class="clarification-filter-card">
    <ElSpace wrap :size="12" class="balloon-toolbar">
      <ElSelect
        v-model="selectedContestId"
        filterable
        :placeholder="t('选择比赛')"
        @change="emit('change-contest')"
      >
        <ElOption
          v-for="contest in contests"
          :key="contest.id"
          :label="contest.name"
          :value="contest.id"
        />
      </ElSelect>
      <ElSelect v-model="statusFilter" :placeholder="t('状态')" @change="emit('change-filter')">
        <ElOption :label="t('全部状态')" value="ALL" />
        <ElOption
          v-for="status in balloonTaskStatuses"
          :key="status"
          :label="statusLabel(t, status)"
          :value="status"
        />
      </ElSelect>
      <ElSelect v-model="problemFilter" clearable :placeholder="t('全部题目')">
        <ElOption
          v-for="problem in problemOptions"
          :key="problem"
          :label="t('题目 {alias}', { alias: problem })"
          :value="problem"
        />
      </ElSelect>
      <ElInput
        v-model="keyword"
        clearable
        :placeholder="t('搜索队伍或座位号')"
        :prefix-icon="Search"
      />
      <ElButton
        type="primary"
        :loading="action === 'dispatch'"
        :disabled="!selectedContestId"
        @click="emit('dispatch')"
        >{{ t('智能领取') }}</ElButton
      >
      <ElButton
        :icon="Refresh"
        :loading="loading"
        :disabled="!selectedContestId"
        @click="emit('refresh')"
        >{{ t('刷新') }}</ElButton
      >
    </ElSpace>
  </ElCard>
</template>

<script setup lang="ts">
import { Refresh, Search } from '@element-plus/icons-vue';
import { balloonTaskStatuses, type BalloonTaskStatus } from '../../api/balloons';
import type { ContestResponse } from '../../api/types';
import { useI18n } from '../../i18n';
import { statusLabel } from './status';

defineProps<{
  contests: ContestResponse[];
  problemOptions: string[];
  loading: boolean;
  action: string;
}>();
const selectedContestId = defineModel<number | null>('selectedContestId');
const statusFilter = defineModel<BalloonTaskStatus | 'ALL'>('statusFilter');
const problemFilter = defineModel<string>('problemFilter');
const keyword = defineModel<string>('keyword');
const emit = defineEmits<{
  'change-contest': [];
  'change-filter': [];
  dispatch: [];
  refresh: [];
}>();
const { t } = useI18n();
</script>

<style scoped>
.clarification-filter-card {
  margin-bottom: 22px;
}
.balloon-toolbar {
  display: flex;
  width: 100%;
}
.balloon-toolbar :deep(.el-space__item) {
  display: flex;
  align-items: center;
}
.balloon-toolbar :deep(.el-space__item .el-select),
.balloon-toolbar :deep(.el-space__item .el-input) {
  width: 100%;
}
.balloon-toolbar :deep(.el-space__item:nth-child(1)) {
  flex: 1.4 1 260px;
}
.balloon-toolbar :deep(.el-space__item:nth-child(2)),
.balloon-toolbar :deep(.el-space__item:nth-child(3)) {
  flex: 0 0 150px;
}
.balloon-toolbar :deep(.el-space__item:nth-child(4)) {
  flex: 1 1 210px;
}
@media (max-width: 900px) {
  .balloon-toolbar :deep(.el-space__item:nth-child(1)),
  .balloon-toolbar :deep(.el-space__item:nth-child(4)) {
    flex-basis: 100%;
  }
}
@media (max-width: 640px) {
  .balloon-toolbar :deep(.el-space__item) {
    flex: 1 1 100% !important;
  }
}
</style>
