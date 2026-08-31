<template>
  <ElRow :gutter="14" class="award-metrics">
    <ElCol :xs="12" :md="6">
      <div class="award-metric">
        <span>{{ t('奖项类别') }}</span
        ><strong>{{ categories.length }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="award-metric">
        <span>{{ t('获奖记录') }}</span
        ><strong>{{ awardSet?.recipients.length ?? 0 }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="award-metric">
        <span>{{ t('获奖队伍') }}</span
        ><strong>{{ uniqueTeams }}</strong>
      </div>
    </ElCol>
    <ElCol :xs="12" :md="6">
      <div class="award-metric">
        <span>{{ t('名单状态') }}</span
        ><strong :class="{ locked: awardSet?.status === 'FROZEN' }">{{
          awardSet ? (awardSet.status === 'FROZEN' ? t('已锁定') : t('草稿')) : t('未生成')
        }}</strong>
      </div>
    </ElCol>
  </ElRow>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { AwardCategory, AwardSet } from '../../api/awards';
import { useI18n } from '../../i18n';

const props = defineProps<{
  categories: AwardCategory[];
  awardSet: AwardSet | null;
}>();
const { t } = useI18n();

const uniqueTeams = computed(
  () => new Set(props.awardSet?.recipients.map((item) => item.teamId) ?? []).size,
);
</script>

<style scoped>
.award-metrics {
  margin-bottom: 18px;
}
.award-metric {
  padding: 20px 22px;
  border: 1px solid #e6eaf0;

  background: white;
}
.award-metric span,
.award-metric strong {
  display: block;
}
.award-metric span {
  margin-bottom: 7px;
  color: var(--muted);
  font-size: 13px;
}
.award-metric strong {
  color: #172033;
  font-size: 24px;
}
.award-metric strong.locked {
  color: #b45309;
}
</style>
