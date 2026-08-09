<template>
  <ElDropdown v-if="visible" trigger="click" @command="changeLocale">
    <ElButton class="language-switcher" :icon="Switch" plain :aria-label="t('切换语言')">
      {{ locale === 'en' ? 'EN' : '中文' }}
    </ElButton>
    <template #dropdown>
      <ElDropdownMenu>
        <ElDropdownItem command="zh-CN" :disabled="locale === 'zh-CN'">中文</ElDropdownItem>
        <ElDropdownItem command="en" :disabled="locale === 'en'">English</ElDropdownItem>
      </ElDropdownMenu>
    </template>
  </ElDropdown>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';
import { Switch } from '@element-plus/icons-vue';
import { type AppLocale, useI18n } from '../i18n';

const route = useRoute();
const { locale, setLocale, t } = useI18n();
const hiddenRoutes = new Set([
  'screen-client',
  'live',
  'live-first-blood',
  'live-balloons',
  'live-freeze',
  'live-statistics',
  'awards-display',
  'resolver-display',
]);
const visible = computed(() => !hiddenRoutes.has(String(route.name ?? '')));

function changeLocale(value: AppLocale) {
  setLocale(value);
}
</script>

<style scoped>
.language-switcher {
  position: fixed;
  right: 18px;
  bottom: 18px;
  z-index: 2000;
  min-width: 76px;
  background: rgb(255 255 255 / 94%);
  box-shadow: 0 4px 16px rgb(15 23 42 / 14%);
}
</style>
