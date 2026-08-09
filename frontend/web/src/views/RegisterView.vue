<template>
  <main class="register-page">
    <ElCard shadow="never"
      ><template #header
        ><h1>{{ t('注册练习账号') }}</h1></template
      ><ElForm :model="form" label-position="top" @submit.prevent="submit"
        ><ElFormItem :label="t('用户名')"
          ><ElInput v-model="form.username" autocomplete="username" /></ElFormItem
        ><ElFormItem :label="t('显示名称')"
          ><ElInput v-model="form.displayName" maxlength="128" /></ElFormItem
        ><ElFormItem :label="t('密码')"
          ><ElInput
            v-model="form.password"
            type="password"
            show-password
            autocomplete="new-password"
            @keyup.enter="submit" /></ElFormItem
        ><ElAlert v-if="error" :title="error" type="error" :closable="false" /><ElButton
          type="primary"
          native-type="submit"
          :loading="session.state.loading"
          class="wide"
          >{{ t('注册并开始练习') }}</ElButton
        ><RouterLink to="/login">{{ t('已有账号，返回登录') }}</RouterLink></ElForm
      ></ElCard
    >
  </main>
</template>
<script setup lang="ts">
import { reactive, ref } from 'vue';
import { useRouter } from 'vue-router';
import { getErrorMessage } from '../api/client';
import { useSession } from '../auth/session';
import { useI18n } from '../i18n';
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const form = reactive({ username: '', displayName: '', password: '' });
const error = ref('');
async function submit() {
  error.value = '';
  try {
    await session.register(form.username.trim(), form.password, form.displayName.trim());
    await router.replace('/practice');
  } catch (e) {
    error.value = getErrorMessage(e);
  }
}
</script>
<style scoped>
.register-page {
  max-width: 480px;
  margin: 12vh auto;
  padding: 20px;
}
.register-page h1 {
  font-size: 20px;
  margin: 0;
}
.register-page form {
  display: grid;
  gap: 4px;
}
.wide {
  width: 100%;
}
.register-page a {
  text-align: center;
  margin-top: 12px;
  display: block;
}
</style>
