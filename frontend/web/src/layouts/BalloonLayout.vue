<template>
  <el-container direction="vertical" class="judge-shell">
    <el-header height="auto" class="judge-header balloon-header">
      <RouterLink class="judge-brand" to="/balloon">
        <span class="brand-mark">PB</span>
        <div><strong>气球工作台</strong><small>Balloon Delivery Desk</small></div>
      </RouterLink>
      <div class="judge-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong
          ><small>气球工作人员</small>
        </div>
        <ElButton plain :icon="SwitchButton" @click="logout">退出登录</ElButton>
      </div>
    </el-header>
    <el-main class="judge-main"><RouterView /></el-main>
  </el-container>
</template>

<script setup lang="ts">
import { SwitchButton } from '@element-plus/icons-vue';
import { useRouter } from 'vue-router';
import { useSession } from '../auth/session';

const router = useRouter();
const session = useSession();

async function logout() {
  await session.logout();
  await router.replace('/admin/login');
}
</script>
