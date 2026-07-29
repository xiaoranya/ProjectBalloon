<template>
  <div class="admin-shell">
    <aside class="admin-sidebar">
      <RouterLink class="admin-brand" to="/admin">
        <span>PB</span>
        <div>
          <strong>Project Balloon</strong>
          <small>赛事管理控制台</small>
        </div>
      </RouterLink>

      <nav class="admin-nav" aria-label="后台导航">
        <RouterLink to="/admin">
          <ElIcon><Monitor /></ElIcon>
          健康与审计
        </RouterLink>
        <RouterLink v-if="session.isAdmin.value" to="/admin/contests">
          <ElIcon><Trophy /></ElIcon>
          比赛管理
        </RouterLink>
        <RouterLink v-if="session.isAdmin.value" to="/admin/team-import">
          <ElIcon><UploadFilled /></ElIcon>
          队伍批量导入
        </RouterLink>
        <RouterLink v-if="session.isSuperAdmin.value" to="/admin/problems">
          <ElIcon><Collection /></ElIcon>
          题库管理
        </RouterLink>
        <RouterLink v-if="session.isSuperAdmin.value" to="/admin/practice">
          <ElIcon><EditPen /></ElIcon>
          日常练习
        </RouterLink>
        <RouterLink v-if="session.isSuperAdmin.value" to="/admin/staff-accounts">
          <ElIcon><UserFilled /></ElIcon>
          工作人员账号
        </RouterLink>
        <RouterLink v-if="session.isSuperAdmin.value" to="/admin/permissions">
          <ElIcon><Lock /></ElIcon>
          比赛管理员权限
        </RouterLink>
      </nav>

      <div class="admin-user">
        <div>
          <strong>{{ session.state.user?.displayName }}</strong>
          <small>{{ roleLabel }}</small>
        </div>
        <ElButton text :icon="SwitchButton" aria-label="退出登录" @click="logout" />
      </div>
    </aside>

    <main class="admin-main">
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { EditPen, SwitchButton } from '@element-plus/icons-vue';
import { useSession } from '../auth/session';

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
