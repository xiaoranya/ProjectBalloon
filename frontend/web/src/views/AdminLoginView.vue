<template>
  <el-container direction="vertical" class="login-page admin-login-page">
    <el-main class="login-page-main">
      <section class="login-copy">
        <h1>{{ t('让比赛运行得更稳、更清晰。') }}</h1>
        <div class="login-feature">
          <ElIcon><DataAnalysis /></ElIcon>
          <span>{{ t('比赛生命周期 · 裁判答疑 · 打印与气球配送') }}</span>
        </div>
      </section>

      <ElCard class="login-card" shadow="never">
        <template #header>
          <div>
            <h2>{{ t('工作人员登录') }}</h2>
            <p>{{ t('使用管理员、裁判、打印员、气球或颁奖工作人员账号') }}</p>
          </div>
        </template>
        <ElForm
          ref="formRef"
          :model="form"
          :rules="rules"
          label-position="top"
          @submit.prevent="submit"
        >
          <ElFormItem :label="t('用户名')" prop="username">
            <ElInput
              v-model="form.username"
              size="large"
              autocomplete="username"
              :prefix-icon="User"
            />
          </ElFormItem>
          <ElFormItem :label="t('密码')" prop="password">
            <ElInput
              v-model="form.password"
              size="large"
              type="password"
              autocomplete="current-password"
              show-password
              :prefix-icon="Lock"
              @keyup.enter="submit"
            />
          </ElFormItem>
          <ElAlert
            v-if="errorMessage"
            :title="errorMessage"
            type="error"
            show-icon
            :closable="false"
            class="form-alert"
          />
          <ElButton
            type="primary"
            size="large"
            native-type="submit"
            :loading="session.state.loading"
            class="wide-button"
          >
            {{ t('进入工作台') }}
          </ElButton>
        </ElForm>
      </ElCard>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { Lock, User } from '@element-plus/icons-vue';
import { getErrorMessage } from '../api/client';
import { homeForUser } from '../auth/access';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const formRef = ref<FormInstance>();
const errorMessage = ref('');
const form = reactive({ username: '', password: '' });
const rules: FormRules = {
  username: [{ required: true, message: t('请输入用户名'), trigger: 'blur' }],
  password: [{ required: true, message: t('请输入密码'), trigger: 'blur' }],
};

async function submit() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  errorMessage.value = '';
  try {
    const user = await session.login(form.username.trim(), form.password);
    if (user.userType === 'TEAM') {
      errorMessage.value = t('该账号是参赛队账号，请使用参赛队登录入口');
      await session.logout();
      return;
    }
    if (user.passwordResetRequired) {
      await router.replace('/change-password');
      return;
    }
    const requestedRedirect = typeof route.query.redirect === 'string' ? route.query.redirect : '';
    const allowedPrefixes = new Set([String(homeForUser(user))]);
    if (user.userType === 'SUPER_ADMIN')
      [
        '/admin',
        '/judge',
        '/printer',
        '/balloon',
        '/resolver',
        '/awards',
        '/screen',
        '/live',
      ].forEach((prefix) => allowedPrefixes.add(prefix));
    const permissionPrefixes = [
      ['CONTEST_MANAGE', '/admin'],
      ['CLARIFICATION_MANAGE', '/judge'],
      ['PRINTING_MANAGE', '/printer'],
      ['BALLOON_MANAGE', '/balloon'],
      ['RESOLVER_MANAGE', '/resolver'],
      ['AWARD_MANAGE', '/awards'],
      ['SCREEN_MANAGE', '/screen'],
      ['LIVE_MANAGE', '/live'],
    ] as const;
    permissionPrefixes.forEach(([permission, prefix]) => {
      if (user.permissions.includes(permission)) allowedPrefixes.add(prefix);
    });
    const redirect = [...allowedPrefixes].some(
      (prefix) => requestedRedirect === prefix || requestedRedirect.startsWith(`${prefix}/`),
    )
      ? requestedRedirect
      : homeForUser(user);
    await router.replace(redirect);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
</script>

<style scoped>
.login-page {
  display: grid;
  min-height: 100vh;
  place-items: center;
  padding: 32px;
  background: #eff5ff;
}

.admin-login-page {
  background: #f6f8fb;
}

.login-page-main {
  padding: 0;
}
</style>
