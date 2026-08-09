<template>
  <ElCard v-if="run" shadow="never" class="resolver-command-card">
    <div class="resolver-command-bar">
      <div>
        <strong>{{ t('单步控制') }}</strong
        ><small>{{ t('每次命令携带当前 version；完成前必须揭晓全部步骤。') }}</small>
      </div>
      <div class="resolver-command-actions">
        <ElButton
          v-if="run.status === 'READY'"
          type="primary"
          :loading="acting"
          @click="emit('control', 'start')"
          >{{ t('开始') }}</ElButton
        >
        <ElButton
          v-if="run.status === 'RUNNING'"
          :loading="acting"
          @click="emit('control', 'pause')"
          >{{ t('暂停') }}</ElButton
        >
        <ElButton
          v-if="run.status === 'PAUSED'"
          type="success"
          :loading="acting"
          @click="emit('control', 'resume')"
          >{{ t('恢复') }}</ElButton
        >
        <ElButton :disabled="!canPrevious" :loading="acting" @click="emit('control', 'previous')">{{
          t('回退一步')
        }}</ElButton>
        <ElButton
          type="primary"
          :disabled="!canNext"
          :loading="acting"
          @click="emit('control', 'next')"
          >{{ t('揭晓下一步') }}</ElButton
        >
        <ElButton
          type="danger"
          plain
          :disabled="!canComplete"
          :loading="acting"
          @click="emit('complete')"
          >{{ t('完成 Resolver') }}</ElButton
        >
      </div>
    </div>
    <div class="resolver-command-bar resolver-auto-play">
      <div>
        <strong>{{ t('自动播放') }}</strong
        ><small>{{ t('间隔范围 500–60000 ms；暂停、完成或到达末尾会自动关闭。') }}</small>
      </div>
      <div class="resolver-command-actions">
        <ElInputNumber v-model="autoInterval" :min="500" :max="60000" :step="500" /><ElButton
          :type="run.autoPlayEnabled ? 'warning' : 'success'"
          :disabled="run.status !== 'RUNNING' || run.currentStep >= run.totalSteps"
          :loading="acting"
          @click="emit('toggle-auto-play')"
          >{{ run.autoPlayEnabled ? t('停止自动播放') : t('启动自动播放') }}</ElButton
        >
      </div>
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import type { ResolverRun } from '../../api/resolver';
import { useI18n } from '../../i18n';

defineProps<{
  run: ResolverRun | null;
  canNext: boolean;
  canPrevious: boolean;
  canComplete: boolean;
  acting: boolean;
}>();
const autoInterval = defineModel<number>('autoInterval');
const emit = defineEmits<{
  control: [action: 'start' | 'next' | 'previous' | 'pause' | 'resume'];
  complete: [];
  'toggle-auto-play': [];
}>();
const { t } = useI18n();
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
.resolver-auto-play {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px dashed #b7ddd6;
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
