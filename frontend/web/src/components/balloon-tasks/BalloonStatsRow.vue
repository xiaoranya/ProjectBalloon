<template>
  <ElRow :gutter="14" class="balloon-stats-grid">
    <ElCol>
      <div class="balloon-stat">
        <span>{{ t('全部任务') }}</span
        ><strong>{{ stats?.total ?? 0 }}</strong>
      </div>
    </ElCol>
    <ElCol>
      <div class="balloon-stat pending">
        <span>{{ t('待领取') }}</span
        ><strong>{{ stats?.pending ?? 0 }}</strong>
      </div>
    </ElCol>
    <ElCol>
      <div class="balloon-stat active">
        <span>{{ t('配送中') }}</span
        ><strong>{{ stats?.claimed ?? 0 }}</strong>
      </div>
    </ElCol>
    <ElCol>
      <div class="balloon-stat delivered">
        <span>{{ t('已送达') }}</span
        ><strong>{{ stats?.delivered ?? 0 }}</strong>
      </div>
    </ElCol>
    <ElCol>
      <div class="balloon-stat first-blood">
        <span>First Blood</span><strong>{{ stats?.firstBlood ?? 0 }}</strong>
      </div>
    </ElCol>
  </ElRow>
</template>

<script setup lang="ts">
import type { BalloonStats } from '../../api/balloons';
import { useI18n } from '../../i18n';

defineProps<{ stats: BalloonStats | null }>();
const { t } = useI18n();
</script>

<style scoped>
.balloon-stats-grid :deep(.el-col) {
  flex: 0 0 20%;
  max-width: 20%;
}
.balloon-stat {
  display: grid;
  gap: 8px;
  border: 1px solid #e2e8f0;
  border-radius: 0;
  padding: 18px;
  color: #64748b;
  background: white;
}
.balloon-stat strong {
  color: #0f172a;
  font-size: 28px;
}
.balloon-stat.pending {
  border-color: #fde68a;
  background: #fffbeb;
}
.balloon-stat.active {
  border-color: #bfdbfe;
  background: #eff6ff;
}
.balloon-stat.delivered {
  border-color: #bbf7d0;
  background: #f0fdf4;
}
.balloon-stat.first-blood {
  border-color: #fecaca;
  background: #fef2f2;
}
@media (max-width: 900px) {
  .balloon-stats-grid :deep(.el-col) {
    flex: 0 0 50%;
    max-width: 50%;
  }
}
@media (max-width: 640px) {
  .balloon-stats-grid :deep(.el-col) {
    flex: 0 0 100%;
    max-width: 100%;
  }
}
</style>
