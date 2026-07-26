<template>
  <main class="login-page password-change-page">
    <section class="login-copy">
      <p class="eyebrow">Account Security</p>
      <h1>{{ required ? '首次登录，请修改密码' : '修改登录密码' }}</h1>
      <p>新密码设置成功后，初始密码将立即失效。</p>
    </section>

    <ElCard class="login-card" shadow="never">
      <ElAlert
        v-if="required"
        title="完成密码修改前，其他系统功能暂不可用。"
        type="warning"
        show-icon
        :closable="false"
      />
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
      />
      <ElForm ref="formRef" :model="form" :rules="rules" label-position="top" @submit.prevent="submit">
        <ElFormItem label="当前密码" prop="currentPassword">
          <ElInput v-model="form.currentPassword" type="password" show-password autocomplete="current-password" />
        </ElFormItem>
        <ElFormItem label="新密码" prop="newPassword">
          <ElInput v-model="form.newPassword" type="password" show-password autocomplete="new-password" />
        </ElFormItem>
        <ElFormItem label="确认新密码" prop="confirmation">
          <ElInput v-model="form.confirmation" type="password" show-password autocomplete="new-password" />
        </ElFormItem>
        <ElButton type="primary" native-type="submit" :loading="session.state.loading">
          保存新密码
        </ElButton>
      </ElForm>
    </ElCard>
  </main>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from 'vue';
import { useRouter } from 'vue-router';
import type { FormInstance, FormRules } from 'element-plus';
import { getErrorMessage } from '../api/client';
import { homeForUserType } from '../auth/access';
import { useSession } from '../auth/session';

const session = useSession();
const router = useRouter();
const formRef = ref<FormInstance>();
const errorMessage = ref('');
const required = computed(() => session.state.user?.passwordResetRequired === true);
const form = reactive({ currentPassword: '', newPassword: '', confirmation: '' });
const rules: FormRules = {
  currentPassword: [{ required: true, message: '请输入当前密码', trigger: 'blur' }],
  newPassword: [
    { required: true, message: '请输入新密码', trigger: 'blur' },
    { min: 8, max: 128, message: '密码长度需为 8 至 128 位', trigger: 'blur' },
  ],
  confirmation: [{
    validator: (_rule, value, callback) => {
      if (value !== form.newPassword) callback(new Error('两次输入的新密码不一致'));
      else callback();
    },
    trigger: 'blur',
  }],
};

async function submit() {
  if (!(await formRef.value?.validate().catch(() => false))) return;
  errorMessage.value = '';
  try {
    const user = await session.changePassword(form.currentPassword, form.newPassword);
    await router.replace(homeForUserType(user.userType));
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
</script>
