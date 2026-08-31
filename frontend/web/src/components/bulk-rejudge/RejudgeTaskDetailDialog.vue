<template>
  <ElDialog
    v-model="detailVisible"
    :title="t('批量重判任务明细')"
    width="min(1100px, 94vw)"
    @closed="emit('closed')"
  >
    <ElSkeleton v-if="detailLoading" :rows="8" animated />
    <template v-else-if="selectedTask">
      <div class="rejudge-detail-heading">
        <ElDescriptions :column="2" border>
          <ElDescriptionsItem :label="t('任务')">#{{ selectedTask.id }}</ElDescriptionsItem>
          <ElDescriptionsItem :label="t('状态')">
            <ElTag :type="taskStatusType(selectedTask.status)">{{
              taskStatusLabel(selectedTask.status)
            }}</ElTag>
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="t('进度')">
            {{ selectedTask.processedItems }} / {{ selectedTask.totalItems }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="t('成功 / 失败')">
            {{ selectedTask.succeededItems }} / {{ selectedTask.failedItems }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="t('开始')">{{
            formatDateTime(selectedTask.startedAt)
          }}</ElDescriptionsItem>
          <ElDescriptionsItem :label="t('完成')">{{
            formatDateTime(selectedTask.completedAt)
          }}</ElDescriptionsItem>
        </ElDescriptions>
        <ElProgress
          :percentage="progressPercentage(selectedTask)"
          :status="progressStatus(selectedTask)"
          :stroke-width="14"
        />
      </div>

      <ElAlert
        v-if="selectedTask.itemsTruncated"
        class="rejudge-inline-alert"
        type="warning"
        show-icon
        :closable="false"
        :title="
          t('任务共有 {total} 条，明细仅返回按 ID 排序的前 1,000 条。聚合进度仍是完整数据。', {
            total: selectedTask.totalItems,
          })
        "
      />

      <ElTable :data="selectedTask.items" row-key="id" max-height="500">
        <ElTableColumn prop="submissionId" :label="t('提交 ID')" width="110" />
        <ElTableColumn :label="t('状态')" width="120">
          <template #default="{ row }">
            <ElTag :type="itemStatusType(row.status)" effect="plain">{{
              itemStatusLabel(row.status)
            }}</ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="attempts" :label="t('尝试次数')" width="100" />
        <ElTableColumn prop="oldJudgementId" :label="t('原判定 ID')" min-width="250">
          <template #default="{ row }"
            ><code>{{ row.oldJudgementId ?? '—' }}</code></template
          >
        </ElTableColumn>
        <ElTableColumn prop="newJudgementId" :label="t('新判定 ID')" min-width="250">
          <template #default="{ row }"
            ><code>{{ row.newJudgementId ?? '—' }}</code></template
          >
        </ElTableColumn>
        <ElTableColumn :label="t('处理时间')" min-width="170">
          <template #default="{ row }">{{ formatDateTime(row.processedAt) }}</template>
        </ElTableColumn>
        <ElTableColumn prop="errorMessage" :label="t('错误')" min-width="240">
          <template #default="{ row }"
            ><span class="error-text">{{ row.errorMessage ?? '—' }}</span></template
          >
        </ElTableColumn>
        <template #empty><ElEmpty :description="t('任务明细尚未生成')" /></template>
      </ElTable>
    </template>
  </ElDialog>
</template>

<script setup lang="ts">
import type { BatchRejudgeTask } from '../../api/bulk-rejudge';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';
import {
  itemStatusLabel,
  itemStatusType,
  progressPercentage,
  progressStatus,
  taskStatusLabel,
  taskStatusType,
} from './status';

const emit = defineEmits<{ closed: [] }>();
const detailVisible = defineModel<boolean>('detailVisible', { required: true });

defineProps<{
  detailLoading: boolean;
  selectedTask: BatchRejudgeTask | null;
}>();

const { t } = useI18n();
</script>

<style scoped>
.rejudge-detail-heading {
  display: grid;
  gap: 14px;
  margin-bottom: 16px;
}
code {
  font-size: 12px;
  overflow-wrap: anywhere;
}
.el-table .error-text {
  color: #dc2626;
}
</style>
