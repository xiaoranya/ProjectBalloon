<template>
  <section class="metrics" :class="`metrics-${template.toLowerCase()}`">
    <article v-for="tile in tiles" :key="tile.label">
      <span>{{ tile.label }}</span
      ><strong>{{ tile.value }}</strong>
    </article>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { PresentationMetrics } from '../../api/presentation';
import { useI18n } from '../../i18n';

const props = withDefaults(
  defineProps<{
    metrics: PresentationMetrics;
    variant: 'balloons' | 'statistics';
    template?: string;
  }>(),
  { template: 'DEFAULT' },
);
const { t } = useI18n();
const tiles = computed(() => {
  if (props.variant === 'balloons') {
    return [
      { label: t('气球总数'), value: props.metrics.balloons.total },
      { label: t('已送达'), value: props.metrics.balloons.delivered },
      {
        label: t('配送中'),
        value: props.metrics.balloons.preparing + props.metrics.balloons.delivering,
      },
      { label: t('首杀'), value: props.metrics.balloons.firstBlood },
    ];
  }
  const total = props.metrics.submissions.total;
  return [
    { label: t('提交'), value: total },
    { label: t('通过'), value: props.metrics.submissions.accepted },
    {
      label: t('通过率'),
      value: `${total ? Math.round((props.metrics.submissions.accepted / total) * 100) : 0}%`,
    },
    { label: t('评测中'), value: props.metrics.submissions.pending },
  ];
});
</script>

<style scoped>
.metrics {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 24px;
  align-content: center;
}
.metrics article {
  display: grid;
  background: #101d2d;
  border-left: 4px solid var(--accent, #22c55e);
  padding: 24px;
}
.metrics strong {
  font-size: clamp(54px, 9vw, 120px);
  color: var(--accent, #22c55e);
}
.metrics-minimal article {
  background: #f2f4f7;
  color: #101828;
  box-shadow: none;
}
.metrics-custom article {
  background: rgb(255 255 255 / 8%);
  background: color-mix(
    in srgb,
    var(--custom-foreground, #fff) 8%,
    var(--custom-background, #07111f)
  );
}
@media (max-width: 800px) {
  .metrics {
    grid-template-columns: 1fr;
  }
}
</style>
