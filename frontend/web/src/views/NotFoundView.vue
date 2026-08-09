<template>
  <main class="center-page">
    <ElResult icon="warning" :title="t('页面不存在')" :sub-title="t('请检查地址，或返回比赛列表')">
      <template #extra>
        <ElButton type="primary" @click="$router.push('/contests')">{{
          t('返回比赛列表')
        }}</ElButton>
      </template>
    </ElResult>
  </main>
</template>
<script setup lang="ts">
import { useI18n } from '../i18n';
const { t } = useI18n();
</script>
