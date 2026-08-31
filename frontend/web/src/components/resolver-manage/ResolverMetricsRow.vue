<template>
  <ElRow :gutter="14" class="resolver-metrics">
    <ElCol :xs="12" :md="6">
      <div class="resolver-metric">
        <span>{{ t('运行状态') }}</span
        ><strong :class="`status-${run?.status.toLowerCase() ?? 'none'}`">{{
          run ? statusLabel(run.status) : t('未选择')
        }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="resolver-metric">
        <span>{{ t('揭晓进度') }}</span
        ><strong>{{ run ? `${run.currentStep} / ${run.totalSteps}` : '—' }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="resolver-metric">
        <span>{{ t('运行类型') }}</span
        ><strong>{{ run ? (run.official ? t('正式') : t('预演')) : '—' }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="resolver-metric">
        <span>{{ t('同步方式') }}</span
        ><strong>{{ realtimeConnected ? t('SSE 实时') : t('轮询校准') }}</strong>
      </div>
    </ElCol>
  </ElRow>
</template>

<script setup lang="ts">
import type { ResolverRun, ResolverRunStatus } from '../../api/resolver';
import { useI18n } from '../../i18n';

defineProps<{
  run: ResolverRun | null;
  realtimeConnected: boolean;
}>();
const { t } = useI18n();

function statusLabel(status: ResolverRunStatus) {
  return t({ READY: '就绪', RUNNING: '运行中', PAUSED: '已暂停', COMPLETED: '已完成' }[status]);
}
</script>

<style scoped>
.resolver-metrics {
  margin-bottom: 18px;
}
.resolver-metric {
  min-width: 0;
  padding: 18px 20px;
  border: 1px solid #e4e9f0;

  background: #fff;
}
.resolver-metric span,
.resolver-metric strong {
  display: block;
}
.resolver-metric span {
  margin-bottom: 7px;
  color: var(--muted);
  font-size: 12px;
}
.resolver-metric strong {
  overflow: hidden;
  color: #172033;
  font-size: 21px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resolver-metric .status-running {
  color: #047857;
}
.resolver-metric .status-paused {
  color: #b45309;
}
.resolver-metric .status-completed {
  color: var(--primary);
}
</style>
