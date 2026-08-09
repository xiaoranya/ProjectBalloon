<template>
  <el-container direction="vertical" class="admin-page staff-accounts-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('工作人员账号') }}</h1>
        <div class="admin-page-actions">
          <ElButton :icon="Refresh" :loading="loading" @click="load">{{ t('刷新') }}</ElButton>
          <ElButton type="primary" :icon="Plus" @click="openCreate">{{ t('新建账号') }}</ElButton>
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
          <ElTableColumn :label="t('账号')" min-width="190">
            <template #default="{ row }">
              <div class="staff-account-identity">
                <strong>{{ row.displayName }}</strong>
                <span>@{{ row.username }} · #{{ row.id }}</span>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('角色')" width="150">
            <template #default="{ row }">{{ roleLabel(row.userType) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('状态')" width="130">
            <template #default="{ row }">
              <ElTag :type="row.enabled ? 'success' : 'info'">
                {{ row.enabled ? t('已启用') : t('已停用') }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('密码状态')" width="140">
            <template #default="{ row }">
              <ElTag v-if="row.passwordResetRequired" type="warning">{{ t('等待首次改密') }}</ElTag>
              <span v-else>{{ t('正常') }}</span>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('最近登录')" min-width="180">
            <template #default="{ row }">{{ formatDateTime(row.lastLoginAt) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('操作')" width="190" fixed="right">
            <template #default="{ row }">
              <ElButton link type="primary" @click="openEdit(row)">{{ t('编辑') }}</ElButton>
              <ElButton link type="warning" @click="openReset(row)">{{ t('重置密码') }}</ElButton>
            </template>
          </ElTableColumn>
        </ElTable>
        <ElEmpty v-if="!loading && !accounts.length" :description="t('暂无工作人员账号')" />
      </ElCard>

      <ElDialog v-model="createVisible" :title="t('新建工作人员账号')" width="520px">
        <ElForm ref="createFormRef" :model="createForm" :rules="createRules" label-position="top">
          <ElFormItem :label="t('用户名')" prop="username">
            <ElInput v-model="createForm.username" :placeholder="t('例如 judge-01')" />
          </ElFormItem>
          <ElFormItem :label="t('显示名称')" prop="displayName">
            <ElInput v-model="createForm.displayName" :placeholder="t('例如 裁判一号')" />
          </ElFormItem>
          <ElFormItem :label="t('角色')" prop="userType">
            <ElSelect v-model="createForm.userType" class="wide-control">
              <ElOption
                v-for="option in roleOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </ElSelect>
          </ElFormItem>
          <ElFormItem :label="t('初始密码')" prop="initialPassword">
            <ElInput v-model="createForm.initialPassword" type="password" show-password />
            <small class="form-help">{{ t('长度 8 至 128 位。') }}</small>
          </ElFormItem>
          <ElFormItem :label="t('账号策略')">
            <ElCheckbox v-model="createForm.requirePasswordReset">{{
              t('须在首次登录时修改密码')
            }}</ElCheckbox>
          </ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="createVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="primary" :loading="saving" @click="createAccount">{{
            t('创建')
          }}</ElButton>
        </template>
      </ElDialog>

      <ElDialog v-model="editVisible" :title="t('编辑工作人员账号')" width="520px">
        <ElForm :model="editForm" label-position="top">
          <ElFormItem :label="t('用户名')">
            <ElInput :model-value="selected?.username" disabled />
          </ElFormItem>
          <ElFormItem :label="t('显示名称')">
            <ElInput v-model="editForm.displayName" />
          </ElFormItem>
          <ElFormItem :label="t('角色')">
            <ElSelect v-model="editForm.userType" class="wide-control">
              <ElOption
                v-for="option in roleOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </ElSelect>
          </ElFormItem>
          <ElFormItem :label="t('账号状态')">
            <ElSwitch
              v-model="editForm.enabled"
              :active-text="t('启用')"
              :inactive-text="t('停用')"
            />
          </ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="editVisible = false">{{ t('取消') }}</ElButton>
          <ElButton type="primary" :loading="saving" @click="saveAccount">{{ t('保存') }}</ElButton>
        </template>
      </ElDialog>

      <ElDialog v-model="resetVisible" :title="t('重置工作人员密码')" width="460px">
        <p>
          {{ t('正在重置') }} <strong>{{ selected?.displayName }}</strong> (@{{
            selected?.username
          }}) {{ t('的密码。') }}
        </p>
        <ElForm label-position="top" class="reset-password-form">
          <ElFormItem :label="t('新初始密码')">
            <ElInput v-model="resetPassword" type="password" show-password />
          </ElFormItem>
          <ElFormItem :label="t('账号策略')">
            <ElCheckbox v-model="resetRequirePasswordReset">{{
              t('须在下次登录时修改密码')
            }}</ElCheckbox>
          </ElFormItem>
        </ElForm>
        <template #footer>
          <ElButton @click="resetVisible = false">{{ t('取消') }}</ElButton>
          <ElButton
            type="warning"
            :disabled="resetPassword.length < 8 || resetPassword.length > 128"
            :loading="saving"
            @click="saveResetPassword"
          >
            {{ t('确认重置') }}
          </ElButton>
        </template>
      </ElDialog>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Plus, Refresh } from '@element-plus/icons-vue';
import type { FormInstance, FormRules } from 'element-plus';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import type { StaffAccount } from '../api/types';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

type StaffType = StaffAccount['userType'];

const { t } = useI18n();
const roleOptions = computed((): Array<{ value: StaffType; label: string }> => [
  { value: 'SUPER_ADMIN', label: t('超级管理员') },
  { value: 'CONTEST_ADMIN', label: t('比赛管理员') },
  { value: 'JUDGE', label: t('裁判') },
  { value: 'PRINTER', label: t('打印员') },
  { value: 'BALLOON_STAFF', label: t('气球工作人员') },
  { value: 'RESOLVER_OPERATOR', label: t('滚榜操作员') },
  { value: 'AWARD_OPERATOR', label: t('颁奖操作员') },
  { value: 'SCREEN_OPERATOR', label: t('大屏操作员') },
  { value: 'LIVE_OPERATOR', label: t('直播操作员') },
]);

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
const createRules = computed<FormRules>(() => ({
  username: [
    { required: true, message: '请输入用户名', trigger: 'blur' },
    {
      pattern: /^[A-Za-z0-9._-]{3,64}$/,
      message: t('请输入 3 至 64 位字母、数字或 ._-'),
      trigger: 'blur',
    },
  ],
  displayName: [{ required: true, message: t('请输入显示名称'), trigger: 'blur' }],
  userType: [{ required: true, message: t('请选择角色'), trigger: 'change' }],
  initialPassword: [
    { required: true, message: t('请输入初始密码'), trigger: 'blur' },
    { min: 8, max: 128, message: t('密码长度需为 8 至 128 位'), trigger: 'blur' },
  ],
}));

function roleLabel(userType: StaffType) {
  return roleOptions.value.find((option) => option.value === userType)?.label ?? userType;
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
    accounts.value = accounts.value.map((account) =>
      account.id === updated.id ? updated : account,
    );
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
    accounts.value = accounts.value.map((account) =>
      account.id === updated.id ? updated : account,
    );
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
