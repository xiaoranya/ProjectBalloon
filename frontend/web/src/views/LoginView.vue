<template>
  <el-container direction="vertical" class="login-page">
    <el-main class="login-page-main">
      <ElCard class="login-card" shadow="never">
        <template #header>
          <div>
            <h2>{{ t('参赛队登录') }}</h2>
            <p>{{ t(competitionMode ? '选择本机登录方式' : '请输入比赛账号和密码') }}</p>
          </div>
        </template>
        <ElSegmented
          v-if="competitionMode"
          v-model="loginMode"
          :options="loginOptions"
          class="login-mode"
        />
        <ElForm
          v-if="loginMode === 'account'"
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
              :placeholder="t('请输入用户名')"
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
              :placeholder="t('请输入密码')"
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
            {{ t('登录') }}
          </ElButton>
          <RouterLink v-if="!competitionMode" class="register-link" to="/register">{{
            t('注册个人练习账号')
          }}</RouterLink>
        </ElForm>
        <ElForm v-else label-position="top" @submit.prevent="submitPairing">
          <ElFormItem :label="t('配对码')">
            <ElInput
              v-model="pairingCode"
              size="large"
              autocomplete="one-time-code"
              :placeholder="t('请输入本机配对码')"
              :prefix-icon="Key"
              maxlength="32"
              @keyup.enter="submitPairing"
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
            :disabled="!pairingCode.trim()"
            class="wide-button"
          >
            {{ t('进入比赛') }}
          </ElButton>
        </ElForm>
        <div class="staff-entry">
          <RouterLink to="/admin/login">{{ t('工作人员登录') }}</RouterLink>
        </div>
      </ElCard>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { Key, Lock, User } from '@element-plus/icons-vue';
import { getErrorMessage } from '../api/client';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';
const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const formRef = ref<FormInstance>();
const errorMessage = ref('');
const pairingCode = ref('');
const competitionMode = computed(() => session.state.deployment.mode === 'competition');
const loginMode = ref<'pairing' | 'account'>(competitionMode.value ? 'pairing' : 'account');
const loginOptions = computed(() => [
  { label: t('配对码'), value: 'pairing' },
  { label: t('账号密码'), value: 'account' },
]);
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
    const accepted = competitionMode.value
      ? user.userType === 'TEAM'
      : ['TEAM', 'INDIVIDUAL'].includes(user.userType);
    if (!accepted) {
      errorMessage.value = t('该账号不是参赛队账号，请使用对应的管理入口');
      await session.logout();
      return;
    }
    if (user.passwordResetRequired) {
      await router.replace('/change-password');
      return;
    }
    const redirect =
      typeof route.query.redirect === 'string'
        ? route.query.redirect
        : user.userType === 'INDIVIDUAL'
          ? '/practice'
          : '/contests';
    await router.replace(redirect);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function submitPairing() {
  if (!pairingCode.value.trim()) return;
  errorMessage.value = '';
  try {
    const user = await session.workstationLogin(pairingCode.value.trim());
    await router.replace(`/contests/${user.competition?.contestId ?? ''}`);
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

.login-page-main {
  padding: 0;
}

.login-mode {
  width: 100%;
  margin-bottom: 22px;
}

.staff-entry {
  margin-top: 16px;
  font-size: 14px;
  text-align: center;
}
.staff-entry a {
  color: #606266;
  text-decoration: none;
}
.staff-entry a:hover {
  color: #337ecc;
  text-decoration: underline;
}

.register-link {
  display: block;
  margin-top: 14px;
  font-size: 14px;
  text-align: center;
  color: var(--primary);
  text-decoration: none;
}
.register-link:hover {
  color: var(--primary-dark);
  text-decoration: underline;
}
</style>
