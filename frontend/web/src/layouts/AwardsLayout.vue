<template><el-container direction="vertical" class="judge-shell awards-shell">
  <el-header height="auto" class="judge-header awards-header">
    <RouterLink class="judge-brand" to="/awards"><span class="brand-mark">PB</span><div><strong>奖项管理</strong><small>Awards Operations</small></div></RouterLink>
    <nav class="judge-nav"><RouterLink to="/awards">名单</RouterLink><RouterLink to="/awards/presentation">颁奖控制</RouterLink><RouterLink to="/awards/host-script">主持人脚本</RouterLink></nav>
    <div class="judge-user"><div><strong>{{ session.state.user?.displayName }}</strong><small>奖项操作员</small></div><ElButton plain :icon="SwitchButton" @click="logout">退出登录</ElButton></div>
  </el-header>
  <el-main class="judge-main"><RouterView /></el-main>
</el-container></template>
<script setup lang="ts">import { SwitchButton } from '@element-plus/icons-vue'; import { useRouter } from 'vue-router'; import { useSession } from '../auth/session'; const router = useRouter(); const session = useSession(); async function logout() { await session.logout(); await router.replace('/admin/login'); }</script>
