<template>
  <footer v-if="active" class="ticker" :class="`ticker-${template.toLowerCase()}`">
    <span class="ticker-brand">ProjectBalloon</span>
    <Transition name="ticker-swap" mode="out-in">
      <p v-if="current" :key="current.id">
        <strong>{{ current.title }}</strong
        ><span>{{ current.body }}</span>
      </p>
      <p v-else key="empty" class="ticker-empty">{{ fallback }}</p>
    </Transition>
  </footer>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';

interface TickerAnnouncement {
  id: number;
  title: string;
  body: string;
  pinned?: boolean;
}

const props = withDefaults(
  defineProps<{
    announcements: TickerAnnouncement[];
    intervalSeconds?: number;
    template?: string;
    fallback?: string;
  }>(),
  { intervalSeconds: 10, template: 'DEFAULT', fallback: '' },
);

const index = ref(0);
let timer: number | undefined;
const active = computed(() => props.announcements.length > 0 || props.fallback.length > 0);
const current = computed(() =>
  props.announcements.length ? props.announcements[index.value % props.announcements.length] : null,
);
function restart() {
  if (timer !== undefined) window.clearInterval(timer);
  index.value = 0;
  if (props.announcements.length > 1) {
    timer = window.setInterval(
      () => {
        index.value += 1;
      },
      Math.max(1, props.intervalSeconds) * 1000,
    );
  }
}
function beforeUnmountCleanup() {
  if (timer !== undefined) window.clearInterval(timer);
}
defineExpose({ restart });
onBeforeUnmount(beforeUnmountCleanup);
watch(
  () => [props.announcements.length, props.intervalSeconds],
  () => restart(),
  { immediate: true },
);
</script>

<style scoped>
.ticker {
  display: flex;
  align-items: center;
  gap: 18px;
  border-top: 2px solid var(--accent, #22c55e);
  padding: 12px 0 0;
  min-height: 58px;
}
.ticker-brand {
  color: var(--accent, #22c55e);
  letter-spacing: 0.2em;
  text-transform: uppercase;
  font-size: 13px;
  white-space: nowrap;
}
.ticker p {
  margin: 0;
  display: flex;
  gap: 14px;
  align-items: baseline;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.ticker strong {
  flex: none;
}
.ticker .ticker-empty {
  color: var(--muted-light, #94a3b8);
}
.ticker-swap-enter-active,
.ticker-swap-leave-active {
  transition:
    opacity 0.4s ease,
    transform 0.4s ease;
}
.ticker-swap-enter-from {
  opacity: 0;
  transform: translateY(12px);
}
.ticker-swap-leave-to {
  opacity: 0;
  transform: translateY(-12px);
}
.ticker-minimal {
  color: #101828;
}
.ticker-custom {
  border-color: var(--custom-accent, var(--accent, #22c55e));
  color: var(--custom-foreground, inherit);
}
@media (prefers-reduced-motion: reduce) {
  .ticker-swap-enter-active,
  .ticker-swap-leave-active {
    transition: none;
  }
}
</style>
