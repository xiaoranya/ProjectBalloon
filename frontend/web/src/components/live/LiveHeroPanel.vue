<template>
  <section class="hero" :class="[`hero-${template.toLowerCase()}`, { 'hero-pulse': pulsing }]">
    <p>{{ eyebrow }}</p>
    <h2>{{ title }}</h2>
    <strong v-if="detail">{{ detail }}</strong>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    variant: 'first-blood' | 'freeze';
    title: string;
    detail?: string;
    template?: string;
    /** Changes whenever the displayed value changes, re-triggering the pulse. */
    pulseKey?: string | number | null;
  }>(),
  { detail: '', template: 'DEFAULT', pulseKey: null },
);
const eyebrow = computed(() =>
  props.variant === 'first-blood' ? 'FIRST BLOOD' : 'FREEZE COUNTDOWN',
);
const pulsing = ref(false);
let pulseTimer: number | undefined;

watch(
  () => props.pulseKey,
  () => {
    if (pulseTimer !== undefined) window.clearTimeout(pulseTimer);
    pulsing.value = true;
    pulseTimer = window.setTimeout(() => {
      pulsing.value = false;
    }, 2000);
  },
);
</script>

<style scoped>
.hero {
  display: grid;
  place-content: center;
  text-align: center;
}
.hero p {
  color: var(--accent, #22c55e);
  letter-spacing: 0.2em;
}
.hero h2 {
  font-size: clamp(64px, 12vw, 170px);
  margin: 10px;
}
.hero-first-blood strong {
  color: var(--accent, #22c55e);
}
.hero-pulse h2 {
  animation: hero-pulse 2s ease-in-out;
}
@keyframes hero-pulse {
  0% {
    transform: scale(0.86);
    opacity: 0.4;
    text-shadow: 0 0 0 transparent;
  }
  45% {
    transform: scale(1.06);
    opacity: 1;
    text-shadow: 0 0 42px color-mix(in srgb, var(--accent, #22c55e) 55%, transparent);
  }
  100% {
    transform: scale(1);
    opacity: 1;
    text-shadow: 0 0 18px color-mix(in srgb, var(--accent, #22c55e) 25%, transparent);
  }
}
@media (prefers-reduced-motion: reduce) {
  .hero-pulse h2 {
    animation: none;
  }
}
.custom p,
.custom h2,
.custom strong {
  color: var(--custom-foreground, inherit);
}
.custom p {
  color: var(--custom-accent, var(--accent, #22c55e));
}
</style>
