<template>
  <ElConfigProvider :locale="elementLocale">
    <RouterView />
    <LanguageSwitcher />
  </ElConfigProvider>
</template>

<script setup lang="ts">
import LanguageSwitcher from './components/LanguageSwitcher.vue';
import { useI18n } from './i18n';

const { elementLocale } = useI18n();
</script>
