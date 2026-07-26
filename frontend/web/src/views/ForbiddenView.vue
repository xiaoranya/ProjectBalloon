<template>
  <main class="center-page">
    <ElResult icon="warning" title="没有访问权限" sub-title="当前账号不能访问这个功能。">
      <template #extra>
        <ElButton type="primary" @click="goHome">返回可用入口</ElButton>
      </template>
    </ElResult>
  </main>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { homeForUserType } from '../auth/access';
import { useSession } from '../auth/session';

const router = useRouter();
const session = useSession();

async function goHome() {
  await router.push(homeForUserType(session.state.user?.userType));
}
</script>
