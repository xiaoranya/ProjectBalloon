<template>
  <span class="live-clock">{{ time }}</span>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    /** Latest server time (ISO); the clock runs monotonically from it. */
    serverTime?: string | null;
    locale?: 'en' | 'zh-CN';
  }>(),
  { serverTime: null },
);

const now = ref(Date.now());
let serverTimeAnchor = Date.now();
let performanceAnchor = 0;
let timer: number | undefined;

function resync() {
  const server = props.serverTime ? new Date(props.serverTime).getTime() : Date.now();
  serverTimeAnchor = server;
  performanceAnchor = performance.now();
  now.value = server;
}
watch(
  () => props.serverTime,
  () => resync(),
  { immediate: true },
);
timer = window.setInterval(() => {
  now.value = serverTimeAnchor + (performance.now() - performanceAnchor);
}, 1000);

const time = computed(() => {
  const date = new Date(now.value);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
});
onBeforeUnmount(() => {
  if (timer !== undefined) window.clearInterval(timer);
});
</script>

<style scoped>
.live-clock {
  font-variant-numeric: tabular-nums;
  letter-spacing: 0.08em;
  color: var(--accent, #22c55e);
  font-size: clamp(16px, 1.6vw, 26px);
}
</style>
