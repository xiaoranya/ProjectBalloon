<template>
  <div class="judge-shell">
    <header class="judge-header">
      <RouterLink class="judge-brand" to="/judge">
        <span class="brand-mark">PB</span>
        <div><strong>裁判工作台</strong><small>Clarification Desk</small></div>
      </RouterLink>
      <div class="judge-user">
        <div><strong>{{ session.state.user?.displayName }}</strong><small>裁判</small></div>
        <ElButton plain :icon="SwitchButton" @click="logout">退出登录</ElButton>
      </div>
    </header>
    <main class="judge-main"><RouterView /></main>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { SwitchButton } from '@element-plus/icons-vue';
import { useSession } from '../auth/session';

const router = useRouter();
const session = useSession();

async function logout() {
  await session.logout();
  await router.replace('/admin/login');
}
</script>
