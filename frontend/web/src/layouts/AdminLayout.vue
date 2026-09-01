<template>
  <el-container class="admin-shell">
    <el-aside width="248px" class="admin-sidebar">
      <RouterLink class="admin-brand" to="/admin">
        <span>PB</span>
        <div>
          <strong>ProjectBalloon</strong>
          <small>{{ t('赛事管理控制台') }}</small>
        </div>
      </RouterLink>

      <el-menu router class="admin-nav-menu" :default-active="route.path">
        <el-menu-item index="/admin">
          <ElIcon><Monitor /></ElIcon>
          <span>{{ t('健康与审计') }}</span>
        </el-menu-item>
        <el-menu-item v-if="session.canManageContests.value" index="/admin/contests">
          <ElIcon><Trophy /></ElIcon>
          <span>{{ t('比赛管理') }}</span>
        </el-menu-item>
        <el-menu-item v-if="session.canManageContests.value" index="/admin/team-import">
          <ElIcon><UploadFilled /></ElIcon>
          <span>{{ t('队伍批量导入') }}</span>
        </el-menu-item>
        <el-menu-item
          v-if="session.canManageContests.value && session.state.deployment.mode === 'competition'"
          index="/admin/competition"
        >
          <ElIcon><Connection /></ElIcon>
          <span>{{ t('终端绑定') }}</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/problems">
          <ElIcon><Collection /></ElIcon>
          <span>{{ t('题库管理') }}</span>
        </el-menu-item>
        <el-menu-item
          v-if="session.isSuperAdmin.value && session.state.deployment.mode !== 'competition'"
          index="/admin/practice"
        >
          <ElIcon><EditPen /></ElIcon>
          <span>{{ t('日常练习') }}</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/staff-accounts">
          <ElIcon><UserFilled /></ElIcon>
          <span>{{ t('工作人员账号') }}</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/permissions">
          <ElIcon><Lock /></ElIcon>
          <span>{{ t('赛事管理范围') }}</span>
        </el-menu-item>
      </el-menu>

      <div class="admin-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong>
          <small>{{ accountTypeLabel }}</small>
        </div>
        <ElButton text :icon="SwitchButton" :aria-label="t('退出登录')" @click="logout" />
      </div>
    </el-aside>

    <el-main class="admin-main">
      <RouterView />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { Connection, EditPen, SwitchButton } from '@element-plus/icons-vue';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const accountTypeLabels: Record<string, string> = {
  SUPER_ADMIN: '超级管理员',
  STAFF: '工作人员',
};
const accountTypeLabel = computed(() => {
  const userType = session.state.user?.userType ?? '';
  return t(accountTypeLabels[userType] ?? userType);
});

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

<style scoped>
.admin-shell {
  min-height: 100vh;
  background: #f6f8fb;
}

.admin-sidebar {
  position: sticky;
  top: 0;
  display: flex;
  flex-direction: column;
  height: 100vh;
  padding: 24px 18px;
  overflow: auto;
}

.admin-sidebar .admin-brand {
  display: flex;
  align-items: center;
  gap: 12px;
  color: inherit;
  text-decoration: none;
}

.admin-sidebar .admin-brand > span {
  display: grid;
  width: 40px;
  height: 40px;
  flex: 0 0 auto;
  place-items: center;
  color: white;
  background: var(--primary);
  font-size: 13px;
  font-weight: 900;
  letter-spacing: -0.04em;
}

.admin-sidebar .admin-brand > div,
.admin-sidebar .admin-user > div {
  min-width: 0;
}

.admin-sidebar .admin-brand strong,
.admin-sidebar .admin-brand small {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.admin-sidebar .admin-brand small {
  margin-top: 3px;
  color: var(--muted);
  font-size: 11px;
}

.admin-sidebar .admin-nav-menu {
  flex: 1;
  margin-top: 42px;
  border-right: none;
  background: transparent;
  --el-menu-border-color: transparent;
}

.admin-sidebar :deep(.admin-nav-menu .el-menu-item) {
  height: 46px;
  line-height: 46px;
}

.admin-sidebar .admin-user {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: auto;
  padding: 16px 0 0;
  border-top: 1px solid var(--border);
}

.admin-sidebar .admin-user strong,
.admin-sidebar .admin-user small {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.admin-sidebar .admin-user small {
  margin-top: 3px;
  color: var(--muted);
}

.admin-sidebar .admin-user .el-button {
  flex: 0 0 auto;
  color: var(--muted);
}

.admin-main {
  min-width: 0;
  padding: 0;
}

@media (max-width: 900px) {
  .admin-shell {
    flex-direction: column;
  }

  .admin-sidebar {
    position: static;
    flex-direction: row;
    align-items: center;
    height: auto;
    padding: 14px 20px;
  }

  .admin-sidebar .admin-brand small,
  .admin-sidebar .admin-user > div {
    display: none;
  }

  .admin-sidebar .admin-nav-menu {
    display: flex;
    flex: none;
    flex-direction: row;
    margin: 0 0 0 auto;
  }

  .admin-sidebar .admin-user {
    margin: 0 0 0 8px;
    padding: 0;
    border: 0;
  }
}
</style>
