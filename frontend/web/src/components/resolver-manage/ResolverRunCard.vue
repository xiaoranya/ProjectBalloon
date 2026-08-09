<template>
  <ElCard shadow="never" class="resolver-command-card">
    <div class="resolver-command-bar">
      <div>
        <strong>{{ t('运行与快照') }}</strong
        ><small v-if="sources"
          >PUBLIC v{{ sources.publicSnapshot.version }} · ADMIN v{{
            sources.finalSnapshot.version
          }}</small
        ><small v-else>{{ t('尚未找到完整快照来源') }}</small>
      </div>
      <div class="resolver-command-actions">
        <ElSelect
          v-if="runs.length"
          v-model="runId"
          :placeholder="t('选择已有运行')"
          @change="emit('select-run')"
        >
          <ElOption
            v-for="item in runs"
            :key="item.id"
            :value="item.id"
            :label="
              t('#{id} · {type} · {status}', {
                id: item.id,
                type: item.official ? t('正式') : t('预演'),
                status: statusLabel(item.status),
              })
            "
          />
        </ElSelect>
        <ElButton :disabled="!sources" :loading="acting" @click="emit('create-run', false)">{{
          t('创建预演')
        }}</ElButton>
        <ElButton
          type="danger"
          plain
          :disabled="!sources || hasOfficial"
          :loading="acting"
          @click="emit('create-run', true)"
          >{{ t('创建正式运行') }}</ElButton
        >
      </div>
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import type { ResolverRun, ResolverRunStatus, ResolverSources } from '../../api/resolver';
import { useI18n } from '../../i18n';

defineProps<{
  runs: ResolverRun[];
  sources: ResolverSources | null;
  hasOfficial: boolean;
  acting: boolean;
}>();
const runId = defineModel<number | null>('runId');
const emit = defineEmits<{
  'select-run': [];
  'create-run': [official: boolean];
}>();
const { t } = useI18n();

function statusLabel(status: ResolverRunStatus) {
  return t({ READY: '就绪', RUNNING: '运行中', PAUSED: '已暂停', COMPLETED: '已完成' }[status]);
}
</script>

<style scoped>
.resolver-command-card {
  margin-bottom: 18px;
  border-color: #cfe4df;
  background: #f0fdfa;
}
.resolver-command-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-content: space-between;
}
.resolver-command-bar small {
  display: block;
  margin: 4px 0 0;
  color: var(--muted);
}
.resolver-command-actions {
  display: flex;
  align-items: center;
  gap: 12px;
  justify-content: flex-end;
  flex-wrap: wrap;
}
.resolver-command-actions .el-button + .el-button {
  margin-left: 0;
}
.resolver-command-actions .el-select {
  width: min(320px, 100%);
}
@media (max-width: 1100px) {
  .resolver-command-bar {
    align-items: stretch;
    flex-direction: column;
  }
  .resolver-command-actions {
    justify-content: flex-start;
  }
}
</style>
