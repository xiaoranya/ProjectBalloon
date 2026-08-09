<template>
  <el-container direction="vertical" class="judge-shell">
    <el-header height="auto" class="judge-header">
      <RouterLink class="judge-brand" to="/judge">
        <span class="brand-mark">PB</span>
        <div>
          <strong>{{ t('裁判工作台') }}</strong
          ><small>Clarification Desk</small>
        </div>
      </RouterLink>
      <div class="judge-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong
          ><small>{{ t('裁判') }}</small>
        </div>
        <ElButton plain :icon="SwitchButton" @click="logout">{{ t('退出登录') }}</ElButton>
      </div>
    </el-header>
    <el-main class="judge-main"><RouterView /></el-main>
  </el-container>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router';
import { SwitchButton } from '@element-plus/icons-vue';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';

const router = useRouter();
const session = useSession();
const { t } = useI18n();

async function logout() {
  await session.logout();
  await router.replace('/admin/login');
}
</script>
