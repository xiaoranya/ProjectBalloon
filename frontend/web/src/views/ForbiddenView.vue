<template>
  <el-container direction="vertical" class="center-page">
    <el-main class="center-page-main">
      <ElResult
        icon="warning"
        :title="t('没有访问权限')"
        :sub-title="t('当前账号不能访问这个功能。')"
      >
        <template #extra>
          <ElButton type="primary" @click="goHome">{{ t('返回可用入口') }}</ElButton>
        </template>
      </ElResult>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { homeForUserType } from '../auth/access';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';

const router = useRouter();
const session = useSession();
const { t } = useI18n();

async function goHome() {
  await router.push(homeForUserType(session.state.user?.userType));
}
</script>

<style scoped>
.center-page {
  display: grid;
  min-height: 100vh;
  place-items: center;
  padding: 48px;
}

.center-page-main {
  padding: 0;
}
</style>
