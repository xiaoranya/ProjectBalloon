<template>
  <ElCard shadow="never" class="rejudge-tasks-card">
    <template #header>
      <div class="card-header">
        <div>
          <strong>{{ t('任务列表') }}</strong>
          <small>{{ t('最多显示本场比赛最近 100 项；活动任务自动轮询。') }}</small>
        </div>
        <span class="rejudge-poll-state" aria-live="polite">
          {{ polling ? t('自动刷新中') : t('暂无活动任务') }}
        </span>
      </div>
    </template>

    <ElTable v-loading="tasksLoading && tasks.length === 0" :data="tasks" row-key="id">
      <ElTableColumn prop="id" :label="t('任务 ID')" width="100" />
      <ElTableColumn :label="t('状态')" width="120">
        <template #default="{ row }">
          <ElTag :type="taskStatusType(row.status)">{{ taskStatusLabel(row.status) }}</ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('进度')" min-width="250">
        <template #default="{ row }">
          <ElProgress
            :percentage="progressPercentage(row as BatchRejudgeTask)"
            :status="progressStatus(row as BatchRejudgeTask)"
            :stroke-width="10"
          />
          <small class="rejudge-progress-copy">
            {{
              t('{processed} / {total} · 成功 {success} · 失败 {failed}', {
                processed: row.processedItems,
                total: row.totalItems,
                success: row.succeededItems,
                failed: row.failedItems,
              })
            }}
          </small>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="t('创建时间')" min-width="170">
        <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
      </ElTableColumn>
      <ElTableColumn :label="t('操作')" width="220" fixed="right">
        <template #default="{ row }">
          <ElButton link @click="emit('select', row.id)">{{ t('查看明细') }}</ElButton>
          <ElButton
            v-if="row.status === 'PENDING' || row.status === 'RUNNING'"
            link
            type="warning"
            :loading="mutatingTaskId === row.id"
            @click="emit('pause', row as BatchRejudgeTask)"
          >
            {{ t('暂停') }}
          </ElButton>
          <ElButton
            v-if="row.status === 'PAUSED'"
            link
            type="primary"
            :loading="mutatingTaskId === row.id"
            @click="emit('resume', row as BatchRejudgeTask)"
          >
            {{ t('恢复') }}
          </ElButton>
        </template>
      </ElTableColumn>
      <template #empty><ElEmpty :description="t('尚未创建批量重判任务')" /></template>
    </ElTable>
  </ElCard>
</template>

<script setup lang="ts">
import type { BatchRejudgeTask } from '../../api/bulk-rejudge';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';
import { progressPercentage, progressStatus, taskStatusLabel, taskStatusType } from './status';

const emit = defineEmits<{
  select: [taskId: number];
  pause: [task: BatchRejudgeTask];
  resume: [task: BatchRejudgeTask];
}>();

defineProps<{
  tasks: BatchRejudgeTask[];
  tasksLoading: boolean;
  polling: boolean;
  mutatingTaskId: number | null;
}>();

const { t } = useI18n();
</script>

<style scoped>
.card-header > div {
  min-width: 0;
}
.card-header small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.rejudge-tasks-card {
  margin-bottom: 20px;
}
.rejudge-poll-state {
  color: var(--muted);
  font-size: 12px;
}
.rejudge-progress-copy {
  display: block;
  margin-top: 6px;
  font-variant-numeric: tabular-nums;
}
</style>
