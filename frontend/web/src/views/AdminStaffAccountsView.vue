<template>
  <el-container direction="vertical" class="admin-page staff-accounts-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>工作人员账号</h1>
        <div class="admin-page-actions">
          <ElButton :icon="Refresh" :loading="loading" @click="load">刷新</ElButton>
          <ElButton type="primary" :icon="Plus" @click="openCreate">新建账号</ElButton>
        </div>
      </div>
    </el-header>
    <el-main class="page-body">
    <ElAlert
      v-if="errorMessage"
      :title="errorMessage"
      type="error"
      show-icon
      :closable="false"
      class="page-alert"
    />

    <ElCard shadow="never">
      <ElTable v-loading="loading" :data="accounts" row-key="id">
        <ElTableColumn label="账号" min-width="190">
          <template #default="{ row }">
            <div class="staff-account-identity">
              <strong>{{ row.displayName }}</strong>
              <span>@{{ row.username }} · #{{ row.id }}</span>
            </div>
          </template>
        </ElTableColumn>
        <ElTableColumn label="角色" width="150">
          <template #default="{ row }">{{ roleLabel(row.userType) }}</template>
        </ElTableColumn>
        <ElTableColumn label="状态" width="130">
          <template #default="{ row }">
            <ElTag :type="row.enabled ? 'success' : 'info'">
              {{ row.enabled ? '已启用' : '已停用' }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn label="密码状态" width="140">
          <template #default="{ row }">
            <ElTag v-if="row.passwordResetRequired" type="warning">等待首次改密</ElTag>
            <span v-else>正常</span>
          </template>
        </ElTableColumn>
        <ElTableColumn label="最近登录" min-width="180">
          <template #default="{ row }">{{ formatDateTime(row.lastLoginAt) }}</template>
        </ElTableColumn>
        <ElTableColumn label="操作" width="190" fixed="right">
          <template #default="{ row }">
            <ElButton link type="primary" @click="openEdit(row)">编辑</ElButton>
            <ElButton link type="warning" @click="openReset(row)">重置密码</ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
      <ElEmpty v-if="!loading && !accounts.length" description="暂无工作人员账号" />
    </ElCard>

    <ElDialog v-model="createVisible" title="新建工作人员账号" width="520px">
      <ElForm ref="createFormRef" :model="createForm" :rules="createRules" label-position="top">
        <ElFormItem label="用户名" prop="username">
          <ElInput v-model="createForm.username" placeholder="例如 judge-01" />
        </ElFormItem>
        <ElFormItem label="显示名称" prop="displayName">
          <ElInput v-model="createForm.displayName" placeholder="例如 裁判一号" />
        </ElFormItem>
        <ElFormItem label="角色" prop="userType">
          <ElSelect v-model="createForm.userType" class="wide-control">
            <ElOption
              v-for="option in roleOptions"
              :key="option.value"
              :label="option.label"
              :value="option.value"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem label="初始密码" prop="initialPassword">
          <ElInput v-model="createForm.initialPassword" type="password" show-password />
          <small class="form-help">长度 8 至 128 位。</small>
        </ElFormItem>
        <ElFormItem label="账号策略">
          <ElCheckbox v-model="createForm.requirePasswordReset">须在首次登录时修改密码</ElCheckbox>
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="createVisible = false">取消</ElButton>
        <ElButton type="primary" :loading="saving" @click="createAccount">创建</ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="editVisible" title="编辑工作人员账号" width="520px">
      <ElForm :model="editForm" label-position="top">
        <ElFormItem label="用户名">
          <ElInput :model-value="selected?.username" disabled />
        </ElFormItem>
        <ElFormItem label="显示名称">
          <ElInput v-model="editForm.displayName" />
        </ElFormItem>
        <ElFormItem label="角色">
          <ElSelect v-model="editForm.userType" class="wide-control">
            <ElOption
              v-for="option in roleOptions"
              :key="option.value"
              :label="option.label"
              :value="option.value"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem label="账号状态">
          <ElSwitch v-model="editForm.enabled" active-text="启用" inactive-text="停用" />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="editVisible = false">取消</ElButton>
        <ElButton type="primary" :loading="saving" @click="saveAccount">保存</ElButton>
      </template>
    </ElDialog>

    <ElDialog v-model="resetVisible" title="重置工作人员密码" width="460px">
      <p>正在重置 <strong>{{ selected?.displayName }}</strong>（@{{ selected?.username }}）的密码。</p>
      <ElForm label-position="top" class="reset-password-form">
        <ElFormItem label="新初始密码">
          <ElInput v-model="resetPassword" type="password" show-password />
        </ElFormItem>
        <ElFormItem label="账号策略">
          <ElCheckbox v-model="resetRequirePasswordReset">须在下次登录时修改密码</ElCheckbox>
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="resetVisible = false">取消</ElButton>
        <ElButton
          type="warning"
          :disabled="resetPassword.length < 8 || resetPassword.length > 128"
          :loading="saving"
          @click="saveResetPassword"
        >
          确认重置
        </ElButton>
      </template>
    </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { Plus, Refresh } from '@element-plus/icons-vue';
import type { FormInstance, FormRules } from 'element-plus';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import type { StaffAccount } from '../api/types';
import { formatDateTime } from '../utils/format';

type StaffType = StaffAccount['userType'];

const roleOptions: Array<{ value: StaffType; label: string }> = [
  { value: 'SUPER_ADMIN', label: '超级管理员' },
  { value: 'CONTEST_ADMIN', label: '比赛管理员' },
  { value: 'JUDGE', label: '裁判' },
  { value: 'PRINTER', label: '打印员' },
  { value: 'BALLOON_STAFF', label: '气球工作人员' },
  { value: 'RESOLVER_OPERATOR', label: '滚榜操作员' },
  { value: 'AWARD_OPERATOR', label: '颁奖操作员' },
  { value: 'SCREEN_OPERATOR', label: '大屏操作员' },
  { value: 'LIVE_OPERATOR', label: '直播操作员' },
];

const accounts = ref<StaffAccount[]>([]);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const createVisible = ref(false);
const editVisible = ref(false);
const resetVisible = ref(false);
const createFormRef = ref<FormInstance>();
const selected = ref<StaffAccount | null>(null);
const resetPassword = ref('');
const createForm = reactive({
  username: '',
  displayName: '',
  userType: 'CONTEST_ADMIN' as StaffType,
  initialPassword: '',
  requirePasswordReset: true,
});
const resetRequirePasswordReset = ref(true);
const editForm = reactive({
  displayName: '',
  userType: 'CONTEST_ADMIN' as StaffType,
  enabled: true,
});
const createRules: FormRules = {
  username: [
    { required: true, message: '请输入用户名', trigger: 'blur' },
    { pattern: /^[A-Za-z0-9._-]{3,64}$/, message: '请输入 3 至 64 位字母、数字或 ._-', trigger: 'blur' },
  ],
  displayName: [{ required: true, message: '请输入显示名称', trigger: 'blur' }],
  userType: [{ required: true, message: '请选择角色', trigger: 'change' }],
  initialPassword: [
    { required: true, message: '请输入初始密码', trigger: 'blur' },
    { min: 8, max: 128, message: '密码长度需为 8 至 128 位', trigger: 'blur' },
  ],
};

function roleLabel(userType: StaffType) {
  return roleOptions.find((option) => option.value === userType)?.label ?? userType;
}

async function load() {
  loading.value = true;
  errorMessage.value = '';
  try {
    accounts.value = (await adminApi.listStaffAccounts()).content;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  Object.assign(createForm, {
    username: '',
    displayName: '',
    userType: 'CONTEST_ADMIN',
    initialPassword: '',
    requirePasswordReset: true,
  });
  createVisible.value = true;
}

async function createAccount() {
  if (!(await createFormRef.value?.validate().catch(() => false))) return;
  saving.value = true;
  errorMessage.value = '';
  try {
    await adminApi.createStaffAccount({
      ...createForm,
      username: createForm.username.trim(),
      displayName: createForm.displayName.trim(),
    });
    createVisible.value = false;
    await load();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    saving.value = false;
  }
}

function openEdit(row: unknown) {
  const account = row as StaffAccount;
  selected.value = account;
  Object.assign(editForm, {
    displayName: account.displayName,
    userType: account.userType,
    enabled: account.enabled,
  });
  editVisible.value = true;
}

async function saveAccount() {
  if (!selected.value) return;
  saving.value = true;
  errorMessage.value = '';
  try {
    const updated = await adminApi.updateStaffAccount(selected.value.id, {
      displayName: editForm.displayName.trim(),
      userType: editForm.userType,
      enabled: editForm.enabled,
    });
    accounts.value = accounts.value.map((account) => account.id === updated.id ? updated : account);
    editVisible.value = false;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    saving.value = false;
  }
}

function openReset(row: unknown) {
  const account = row as StaffAccount;
  selected.value = account;
  resetPassword.value = '';
  resetRequirePasswordReset.value = true;
  resetVisible.value = true;
}

async function saveResetPassword() {
  if (!selected.value) return;
  saving.value = true;
  errorMessage.value = '';
  try {
    const updated = await adminApi.resetStaffPassword(
      selected.value.id,
      resetPassword.value,
      resetRequirePasswordReset.value,
    );
    accounts.value = accounts.value.map((account) => account.id === updated.id ? updated : account);
    resetVisible.value = false;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    saving.value = false;
  }
}

onMounted(load);
</script>

<style scoped>
.admin-page {
  width: min(1320px, 100%);
  margin: 0 auto;
}
.page-head {
  height: auto;
  padding: 42px 42px 0;
}
.page-body {
  padding: 0 42px 42px;
}
.admin-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 28px;
}
.admin-page-header.compact {
  align-items: center;
}
.admin-page-header h1 {
  margin: 5px 0 6px;
  font-size: clamp(28px, 4vw, 40px);
  color: #13213b;
}
.admin-page-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
.page-alert {
  margin-bottom: 20px;
}
.staff-account-identity {
  display: grid;
  gap: 0.2rem;
}
.staff-account-identity span {
  color: var(--el-text-color-secondary);
  font-size: 0.8rem;
}
.form-help {
  color: var(--el-text-color-secondary);
  font-size: 0.8rem;
}
.reset-password-form {
  margin-top: 1rem;
}
.wide-control {
  width: 100%;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
@media (max-width: 640px) {
  .page-head {
    padding: 24px 16px 0;
  }
  .page-body {
    padding: 0 16px 24px;
  }
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 16px;
  }
}
</style>
