<template>
  <ElDrawer
    v-model="detailVisible"
    :title="t('气球任务详情')"
    size="min(650px, 95vw)"
    @closed="selected = null"
  >
    <div v-if="selected" class="balloon-detail">
      <div class="announcement-detail-tags">
        <ElTag :type="statusType(selected.status)">{{ statusLabel(t, selected.status) }}</ElTag>
        <ElTag v-if="selected.isFirstBlood" type="danger" effect="dark">First Blood</ElTag>
        <ElTag effect="plain"
          ><span class="balloon-color small" :style="{ backgroundColor: selected.color }" />
          {{ selected.problemAlias }} · {{ selected.color }}</ElTag
        >
      </div>
      <ElDescriptions :column="1" border>
        <ElDescriptionsItem :label="t('队伍')">{{ selected.teamName }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('座位号')">{{ selected.seatNo ?? '—' }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('关联提交')">#{{ selected.submissionId }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('领取时间')">{{
          formatDateTime(selected.claimedAt)
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="t('送达时间')">{{
          formatDateTime(selected.deliveredAt)
        }}</ElDescriptionsItem>
        <ElDescriptionsItem v-if="selected.cancelledReason" :label="t('取消原因')"
          ><span class="danger-text">{{ selected.cancelledReason }}</span></ElDescriptionsItem
        >
        <ElDescriptionsItem v-if="selected.reopenedCount" :label="t('重新打开')">{{
          t('{count} 次', { count: selected.reopenedCount })
        }}</ElDescriptionsItem>
      </ElDescriptions>
      <div class="balloon-note-panel">
        <div>
          <strong>{{ t('配送备注') }}</strong
          ><small>{{ t('入口、楼层、座位异常或交接信息，最多 1000 字') }}</small>
        </div>
        <ElInput v-model="note" type="textarea" :rows="4" maxlength="1000" show-word-limit />
        <ElButton :loading="action === 'note'" @click="emit('save-note')">{{
          t('保存备注')
        }}</ElButton>
      </div>
      <div class="clarification-actions balloon-actions">
        <ElButton
          v-if="selected.status === 'PENDING'"
          type="primary"
          :loading="action === 'claim'"
          @click="emit('claim')"
          >{{ t('领取任务') }}</ElButton
        >
        <ElButton
          v-if="canDeliver"
          type="success"
          :loading="action === 'deliver'"
          @click="emit('deliver')"
          >{{ t('标记已送达') }}</ElButton
        >
        <ElButton
          v-if="selected.status === 'CANCELLED'"
          type="warning"
          :loading="action === 'reopen'"
          @click="emit('reopen')"
          >{{ t('重新打开') }}</ElButton
        >
        <ElButton
          v-if="selected.status === 'PENDING' || selected.status === 'CLAIMED'"
          type="danger"
          plain
          @click="emit('open-cancel')"
          >{{ t('取消任务') }}</ElButton
        >
      </div>
    </div>
  </ElDrawer>
</template>

<script setup lang="ts">
import type { BalloonTask } from '../../api/balloons';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';
import { statusLabel, statusType } from './status';

defineProps<{
  action: string;
  canDeliver: boolean;
}>();
const emit = defineEmits<{
  claim: [];
  deliver: [];
  reopen: [];
  'open-cancel': [];
  'save-note': [];
}>();
const { t } = useI18n();
const note = defineModel<string>('note');
const detailVisible = defineModel<boolean>('detailVisible');
const selected = defineModel<BalloonTask | null>('selected');
</script>

<style scoped>
.balloon-detail {
  display: grid;
  gap: 22px;
}
.announcement-detail-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 18px;
}
.balloon-color.small {
  width: 14px;
  height: 14px;
  margin-right: 5px;
}
.balloon-note-panel {
  display: grid;
  gap: 12px;
  border: 1px solid #e2e8f0;

  padding: 16px;
  background: #f8fafc;
}
.balloon-note-panel > div {
  display: grid;
  gap: 3px;
}
.balloon-note-panel small {
  color: var(--muted);
}
.balloon-note-panel > :deep(.el-button) {
  justify-self: end;
}
.balloon-actions {
  flex-wrap: wrap;
}
.danger-text {
  color: #dc2626;
}
</style>
