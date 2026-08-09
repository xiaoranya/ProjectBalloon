<template>
  <el-container class="admin-shell">
    <el-aside width="248px" class="admin-sidebar">
      <RouterLink class="admin-brand" to="/admin">
        <span>PB</span>
        <div>
          <strong>ProjectBalloon</strong>
          <small>赛事管理控制台</small>
        </div>
      </RouterLink>

      <el-menu router class="admin-nav-menu" :default-active="route.path">
        <el-menu-item index="/admin">
          <ElIcon><Monitor /></ElIcon>
          <span>健康与审计</span>
        </el-menu-item>
        <el-menu-item v-if="session.isAdmin.value" index="/admin/contests">
          <ElIcon><Trophy /></ElIcon>
          <span>比赛管理</span>
        </el-menu-item>
        <el-menu-item v-if="session.isAdmin.value" index="/admin/team-import">
          <ElIcon><UploadFilled /></ElIcon>
          <span>队伍批量导入</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/problems">
          <ElIcon><Collection /></ElIcon>
          <span>题库管理</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/practice">
          <ElIcon><EditPen /></ElIcon>
          <span>日常练习</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/staff-accounts">
          <ElIcon><UserFilled /></ElIcon>
          <span>工作人员账号</span>
        </el-menu-item>
        <el-menu-item v-if="session.isSuperAdmin.value" index="/admin/permissions">
          <ElIcon><Lock /></ElIcon>
          <span>比赛管理员权限</span>
        </el-menu-item>
      </el-menu>

      <div class="admin-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong>
          <small>{{ roleLabel }}</small>
        </div>
        <ElButton text :icon="SwitchButton" aria-label="退出登录" @click="logout" />
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
import { EditPen, SwitchButton } from '@element-plus/icons-vue';
import { useSession } from '../auth/session';

const route = useRoute();
const router = useRouter();
const session = useSession();
const roleLabels: Record<string, string> = {
  SUPER_ADMIN: '超级管理员',
  CONTEST_ADMIN: '比赛管理员',
  JUDGE: '裁判',
  PRINTER: '打印员',
  BALLOON_STAFF: '气球工作人员',
  AWARD_OPERATOR: '颁奖操作员',
  RESOLVER_OPERATOR: '滚榜操作员',
  SCREEN_OPERATOR: '大屏操作员',
  LIVE_OPERATOR: '直播操作员',
};
const roleLabel = computed(() => {
  const userType = session.state.user?.userType ?? '';
  return roleLabels[userType] ?? userType;
});

async function logout() {
  await session.logout();
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

.admin-sidebar :deep(.admin-brand) {
  display: flex;
  align-items: center;
  gap: 12px;
  color: inherit;
  text-decoration: none;
}

.admin-sidebar :deep(.admin-brand > span) {
  display: grid;
  width: 40px;
  height: 40px;
  flex: 0 0 auto;
  place-items: center;
  color: white;
  background: #2563eb;
  font-size: 13px;
  font-weight: 900;
  letter-spacing: -0.04em;
}

.admin-sidebar :deep(.admin-brand > div),
.admin-sidebar :deep(.admin-user > div) {
  min-width: 0;
}

.admin-sidebar :deep(.admin-brand strong),
.admin-sidebar :deep(.admin-brand small) {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.admin-sidebar :deep(.admin-brand small) {
  margin-top: 3px;
  color: var(--muted);
  font-size: 11px;
}

.admin-sidebar :deep(.admin-nav-menu) {
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

.admin-sidebar :deep(.admin-user) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: auto;
  padding: 16px 0 0;
  border-top: 1px solid var(--border);
}

.admin-sidebar :deep(.admin-user strong),
.admin-sidebar :deep(.admin-user small) {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.admin-sidebar :deep(.admin-user small) {
  margin-top: 3px;
  color: var(--muted);
}

.admin-sidebar :deep(.admin-user .el-button) {
  flex: 0 0 auto;
  color: var(--muted);
}

.admin-main {
  min-width: 0;
  padding: 0;
  overflow: auto;
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

  .admin-sidebar :deep(.admin-brand small),
  .admin-sidebar :deep(.admin-user > div) {
    display: none;
  }

  .admin-sidebar :deep(.admin-nav-menu) {
    display: flex;
    flex: none;
    flex-direction: row;
    margin: 0 0 0 auto;
  }

  .admin-sidebar :deep(.admin-user) {
    margin: 0 0 0 8px;
    padding: 0;
    border: 0;
  }
}
</style>
