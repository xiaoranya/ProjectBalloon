<template>
  <main class="login-page">
    <section class="login-copy">
      <p class="eyebrow">Project Balloon</p>
      <h1>专注比赛，剩下的交给系统。</h1>
      <p>登录参赛端，查看题目、提交代码并跟踪实时判题结果。</p>
      <div class="login-feature">
        <ElIcon><CircleCheck /></ElIcon>
        <span>局域网离线运行 · 会话安全保护 · 完整判题链路</span>
      </div>
    </section>

    <ElCard class="login-card" shadow="never">
      <template #header>
        <div>
          <h2>参赛队登录</h2>
          <p>请输入比赛账号和密码</p>
        </div>
      </template>
      <ElForm ref="formRef" :model="form" :rules="rules" label-position="top" @submit.prevent="submit">
        <ElFormItem label="用户名" prop="username">
          <ElInput
            v-model="form.username"
            size="large"
            autocomplete="username"
            placeholder="请输入用户名"
            :prefix-icon="User"
          />
        </ElFormItem>
        <ElFormItem label="密码" prop="password">
          <ElInput
            v-model="form.password"
            size="large"
            type="password"
            autocomplete="current-password"
            show-password
            placeholder="请输入密码"
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
        <ElButton type="primary" size="large" native-type="submit" :loading="session.state.loading" class="wide-button">
          登录比赛
        </ElButton>
      </ElForm>
    </ElCard>
  </main>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { Lock, User } from '@element-plus/icons-vue';
import { getErrorMessage } from '../api/client';
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
    if (user.userType !== 'TEAM') {
      errorMessage.value = '该账号不是参赛队账号，请使用对应的管理入口';
      await session.logout();
      return;
    }
    if (user.passwordResetRequired) {
      await router.replace('/change-password');
      return;
    }
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/contests';
    await router.replace(redirect);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
</script>
