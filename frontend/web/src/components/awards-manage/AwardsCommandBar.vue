<template>
  <ElCard shadow="never" class="award-command-card">
    <div class="award-command-bar">
      <div>
        <strong>{{ t('名单操作') }}</strong
        ><small>{{ t('只有已完成的正式 Resolver 可以生成奖项名单。') }}</small>
      </div>
      <ElSpace wrap :size="10">
        <ElSelect v-model="resolverRunId" :placeholder="t('选择正式 Resolver')">
          <ElOption
            v-for="item in completedRuns"
            :key="item.id"
            :value="item.id"
            :label="
              t('运行 #{id} · {time}', { id: item.id, time: formatDateTime(item.completedAt) })
            "
          />
        </ElSelect>
        <ElButton
          type="primary"
          :icon="MagicStick"
          :disabled="!resolverRunId || !categories.length || awardSet?.status === 'FROZEN'"
          :loading="mutating"
          @click="emit('generate')"
          >{{ t('生成名单') }}</ElButton
        >
        <ElButton
          v-if="awardSet?.status === 'DRAFT'"
          type="warning"
          :icon="Lock"
          :loading="mutating"
          @click="emit('freeze')"
          >{{ t('锁定名单') }}</ElButton
        >
        <ElButton
          v-if="awardSet?.status === 'FROZEN'"
          :icon="Unlock"
          :loading="mutating"
          @click="emit('unfreeze')"
          >{{ t('解除锁定') }}</ElButton
        >
        <ElButton
          :icon="Download"
          :disabled="!awardSet"
          :loading="exporting"
          @click="emit('export-csv')"
          >{{ t('导出名单') }}</ElButton
        >
        <ElButton
          :icon="Download"
          :disabled="awardSet?.status !== 'FROZEN'"
          :loading="exporting"
          @click="emit('export-certificates')"
          >{{ t('导出证书数据') }}</ElButton
        >
      </ElSpace>
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import { Download, Lock, MagicStick, Unlock } from '@element-plus/icons-vue';
import type { AwardCategory, AwardResolverRun, AwardSet } from '../../api/awards';
import { formatDateTime } from '../../utils/format';
import { useI18n } from '../../i18n';

defineProps<{
  categories: AwardCategory[];
  awardSet: AwardSet | null;
  completedRuns: AwardResolverRun[];
  mutating: boolean;
  exporting: boolean;
}>();
const resolverRunId = defineModel<number | null>('resolverRunId');
const emit = defineEmits<{
  generate: [];
  freeze: [];
  unfreeze: [];
  'export-csv': [];
  'export-certificates': [];
}>();
const { t } = useI18n();
</script>

<style scoped>
.award-command-card {
  margin-bottom: 18px;
  border-color: #eadfca;
  background: #fffbeb;
}
.award-command-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.award-command-bar small {
  display: none;
  margin: 4px 0 0;
  color: var(--muted);
}
@media (max-width: 760px) {
  .award-command-bar {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
