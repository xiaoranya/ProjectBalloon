<template>
  <header>
    <div>
      <small v-if="showBrand">ProjectBalloon · LIVE</small>
      <h1>{{ title }}</h1>
      <p v-if="subtitle">{{ subtitle }}</p>
    </div>
    <span>{{ status }}</span>
  </header>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    title?: string | null;
    subtitle?: string | null;
    status?: string | null;
    showBrand?: boolean | null;
    template?: string;
  }>(),
  { title: 'LIVE', subtitle: '', status: '', showBrand: true, template: 'DEFAULT' },
);
</script>

<style scoped>
header {
  display: flex;
  justify-content: space-between;
  border-bottom: 2px solid var(--accent, #22c55e);
}
h1 {
  margin: 4px 0;
}
header > span {
  color: var(--accent, #22c55e);
  letter-spacing: 0.2em;
}
.minimal header {
  border-bottom-width: 1px;
}
.minimal h1,
.minimal p,
.minimal span {
  color: #101828;
}
.custom header {
  border-color: var(--custom-accent, var(--accent, #22c55e));
}
.custom h1,
.custom p,
.custom span {
  color: var(--custom-foreground, inherit);
}
.custom > span {
  color: var(--custom-accent, var(--accent, #22c55e));
}
</style>
