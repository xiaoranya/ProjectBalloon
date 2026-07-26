<template>
  <div class="judge-shell resolver-shell">
    <header class="judge-header resolver-header">
      <RouterLink class="judge-brand" to="/resolver"><span class="brand-mark">PB</span><div><strong>Resolver 控制台</strong><small>Official Reveal Control</small></div></RouterLink>
      <div class="judge-user"><div><strong>{{ session.state.user?.displayName }}</strong><small>Resolver 操作员</small></div><ElButton plain :icon="SwitchButton" @click="logout">退出登录</ElButton></div>
    </header>
    <main class="judge-main"><RouterView /></main>
  </div>
</template>

<script setup lang="ts">
import { SwitchButton } from '@element-plus/icons-vue';
import { useRouter } from 'vue-router';
import { useSession } from '../auth/session';
const router = useRouter();
const session = useSession();
async function logout() { await session.logout(); await router.replace('/admin/login'); }
</script>
