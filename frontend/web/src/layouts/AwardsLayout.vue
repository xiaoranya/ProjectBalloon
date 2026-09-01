<template>
  <el-container direction="vertical" class="judge-shell">
    <el-header height="auto" class="judge-header awards-header">
      <RouterLink class="judge-brand" to="/awards"
        ><span class="brand-mark">PB</span>
        <div>
          <strong>{{ t('奖项管理') }}</strong
          ><small>Awards Operations</small>
        </div></RouterLink
      >
      <nav class="judge-nav">
        <RouterLink to="/awards">{{ t('名单') }}</RouterLink
        ><RouterLink to="/awards/presentation">{{ t('颁奖控制') }}</RouterLink
        ><RouterLink to="/awards/host-script">{{ t('主持人脚本') }}</RouterLink>
      </nav>
      <div class="judge-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong
          ><small>{{ t('奖项操作员') }}</small>
        </div>
        <ElButton plain :icon="SwitchButton" @click="logout">{{ t('退出登录') }}</ElButton>
      </div>
    </el-header>
    <el-main class="judge-main"><RouterView /></el-main>
  </el-container>
</template>
<script setup lang="ts">
import { SwitchButton } from '@element-plus/icons-vue';
import { useRouter } from 'vue-router';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';
const router = useRouter();
const session = useSession();
const { t } = useI18n();
async function logout() {
  // session.logout() clears the session in its finally block even when the
  // request fails; navigate regardless so the UI never half-clears.
  try {
    await session.logout();
  } catch {
    // The logout request outcome does not change the local session state.
  }
  await router.replace('/admin/login');
}
</script>
