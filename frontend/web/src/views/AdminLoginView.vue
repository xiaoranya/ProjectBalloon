<template>
  <el-container direction="vertical" class="login-page admin-login-page">
    <el-main class="login-page-main">
      <section class="login-copy">
        <p class="eyebrow">Project Balloon Admin</p>
        <h1>让比赛运行得更稳、更清晰。</h1>
        <div class="login-feature">
          <ElIcon><DataAnalysis /></ElIcon>
          <span>比赛生命周期 · 裁判答疑 · 打印与气球配送</span>
        </div>
      </section>

      <ElCard class="login-card" shadow="never">
        <template #header>
          <div>
            <h2>工作人员登录</h2>
            <p>使用管理员、裁判、打印员、气球或颁奖工作人员账号</p>
          </div>
        </template>
        <ElForm ref="formRef" :model="form" :rules="rules" label-position="top" @submit.prevent="submit">
          <ElFormItem label="用户名" prop="username">
            <ElInput v-model="form.username" size="large" autocomplete="username" :prefix-icon="User" />
          </ElFormItem>
          <ElFormItem label="密码" prop="password">
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
          <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="form-alert" />
          <ElButton type="primary" size="large" native-type="submit" :loading="session.state.loading" class="wide-button">
            进入工作台
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
import { homeForUserType } from '../auth/access';
import { useSession } from '../auth/session';

const route = useRoute();
const router = useRouter();
const session = useSession();
const formRef = ref<FormInstance>();
const errorMessage = ref('');
const form = reactive({ username: '', password: '' });
const rules: FormRules = {
  username: [{ required: true, message: '请输入用户名', trigger: 'blur' }],
  password: [{ required: true, message: '请输入密码', trigger: 'blur' }],
};

async function submit() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  errorMessage.value = '';
  try {
    const user = await session.login(form.username.trim(), form.password);
    if (user.userType === 'TEAM') {
      errorMessage.value = '该账号是参赛队账号，请使用参赛队登录入口';
      await session.logout();
      return;
    }
    if (user.passwordResetRequired) {
      await router.replace('/change-password');
      return;
    }
    const requestedRedirect = typeof route.query.redirect === 'string' ? route.query.redirect : '';
    const allowedPrefixes = new Set([String(homeForUserType(user.userType))]);
    if (user.userType === 'SUPER_ADMIN') ['/admin', '/judge', '/printer'].forEach((prefix) => allowedPrefixes.add(prefix));
    if (user.userType === 'JUDGE' || user.roles.includes('JUDGE')) allowedPrefixes.add('/judge');
    if (user.userType === 'PRINTER' || user.roles.includes('PRINTER')) allowedPrefixes.add('/printer');
    const redirect = [...allowedPrefixes].some((prefix) => requestedRedirect === prefix || requestedRedirect.startsWith(`${prefix}/`))
      ? requestedRedirect
      : homeForUserType(user.userType);
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
