<template>
  <Transition name="fb-popup">
    <aside
      v-if="shown"
      :key="String(playKey ?? '')"
      class="fb-popup"
      :class="`fb-popup-${template.toLowerCase()}`"
    >
      <span class="fb-popup-eyebrow">FIRST BLOOD</span>
      <strong class="fb-popup-team">{{ teamName }}</strong>
      <span v-if="problemAlias" class="fb-popup-problem">{{ problemAlias }}</span>
      <i v-for="index in 8" :key="index" class="fb-popup-spark" aria-hidden="true"></i>
    </aside>
  </Transition>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    teamName: string;
    problemAlias?: string;
    /** Bump to replay the entrance animation for a new first blood. */
    playKey?: string | number | null;
    template?: string;
    /** Transient popups auto-hide after the duration; persistent stay on air. */
    transient?: boolean;
    hideAfterMs?: number;
  }>(),
  {
    problemAlias: '',
    playKey: null,
    template: 'DEFAULT',
    transient: false,
    hideAfterMs: 12_000,
  },
);

const shown = ref(false);
let hideTimer: number | undefined;

watch(
  () => [props.playKey, props.teamName] as const,
  ([playKey, teamName]) => {
    shown.value = Boolean(playKey && teamName);
    if (hideTimer !== undefined) {
      window.clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    if (shown.value && props.transient) {
      hideTimer = window.setTimeout(() => {
        shown.value = false;
      }, props.hideAfterMs);
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  if (hideTimer !== undefined) window.clearTimeout(hideTimer);
});
</script>

<style scoped>
.fb-popup {
  position: relative;
  overflow: hidden;
  display: grid;
  justify-items: center;
  gap: 6px;
  padding: 26px 44px;
  background: color-mix(in srgb, var(--accent, #22c55e) 14%, #07111f);
  border: 2px solid var(--accent, #22c55e);
  border-radius: 18px;
  box-shadow: 0 0 60px color-mix(in srgb, var(--accent, #22c55e) 35%, transparent);
  color: #fff;
  text-align: center;
}
.fb-popup-eyebrow {
  letter-spacing: 0.32em;
  color: var(--accent, #22c55e);
  font-size: 14px;
}
.fb-popup-team {
  font-size: clamp(34px, 4.4vw, 64px);
  line-height: 1.1;
}
.fb-popup-problem {
  letter-spacing: 0.14em;
  color: var(--muted-light, #94a3b8);
}
.fb-popup-spark {
  position: absolute;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--accent, #22c55e);
  opacity: 0;
  animation: fb-spark 1.1s ease-out;
}
.fb-popup-spark:nth-child(odd) {
  background: #fff;
}
.fb-popup-enter-active {
  transition:
    opacity 0.45s ease,
    transform 0.45s cubic-bezier(0.2, 0.9, 0.3, 1.3);
  animation: fb-glow 1.4s ease-out;
}
.fb-popup-leave-active {
  transition:
    opacity 0.5s ease,
    transform 0.5s ease;
}
.fb-popup-enter-from {
  opacity: 0;
  transform: scale(0.6) translateY(30px);
}
.fb-popup-leave-to {
  opacity: 0;
  transform: translateY(18px) scale(0.96);
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(1) {
  --spark-x: -60px;
  --spark-y: -34px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(2) {
  --spark-x: 44px;
  --spark-y: -52px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(3) {
  --spark-x: 96px;
  --spark-y: -8px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(4) {
  --spark-x: -104px;
  --spark-y: 6px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(5) {
  --spark-x: -36px;
  --spark-y: 42px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(6) {
  --spark-x: 62px;
  --spark-y: 44px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(7) {
  --spark-x: 118px;
  --spark-y: -30px;
}
.fb-popup-enter-active .fb-popup-spark:nth-of-type(8) {
  --spark-x: -112px;
  --spark-y: -40px;
}
@keyframes fb-spark {
  0% {
    opacity: 0;
    transform: translate(0, 0) scale(0.4);
  }
  25% {
    opacity: 1;
  }
  100% {
    opacity: 0;
    transform: translate(var(--spark-x, 40px), var(--spark-y, -40px)) scale(1.1);
  }
}
@keyframes fb-glow {
  0% {
    box-shadow: 0 0 0 transparent;
  }
  40% {
    box-shadow: 0 0 90px color-mix(in srgb, var(--accent, #22c55e) 60%, transparent);
  }
  100% {
    box-shadow: 0 0 60px color-mix(in srgb, var(--accent, #22c55e) 35%, transparent);
  }
}
.fb-popup-minimal {
  background: #ffffffee;
  border-color: #101828;
  color: #101828;
  box-shadow: 0 12px 42px rgb(16 24 40 / 24%);
}
.fb-popup-minimal .fb-popup-eyebrow,
.fb-popup-minimal .fb-popup-spark {
  color: #101828;
  background: #101828;
}
.fb-popup-minimal .fb-popup-spark:nth-child(odd) {
  background: #ef4444;
}
.fb-popup-custom {
  background: color-mix(
    in srgb,
    var(--custom-accent, var(--accent, #22c55e)) 14%,
    var(--custom-background, #07111f)
  );
  border-color: var(--custom-accent, var(--accent, #22c55e));
  color: var(--custom-foreground, #fff);
}
.fb-popup-custom .fb-popup-eyebrow {
  color: var(--custom-accent, var(--accent, #22c55e));
}
@media (prefers-reduced-motion: reduce) {
  .fb-popup-enter-active,
  .fb-popup-leave-active,
  .fb-popup-spark {
    animation: none;
    transition: opacity 0.2s ease;
  }
  .fb-popup-enter-from,
  .fb-popup-leave-to {
    opacity: 0;
    transform: none;
  }
}
</style>
