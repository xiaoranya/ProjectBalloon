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
          <ElTableColumn :label="t('账号')" min-width="170">
            <template #default="{ row }">
              <div class="staff-account-identity">
                <strong>{{ row.displayName }}</strong>
                <span>@{{ row.username }} · #{{ row.id }}</span>
              </div>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('账号类型')" width="120">
            <template #default="{ row }">
              <ElTag :type="row.userType === 'SUPER_ADMIN' ? 'danger' : 'info'">
                {{ row.userType === 'SUPER_ADMIN' ? t('超级管理员') : t('工作人员') }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('权限')" min-width="220">
            <template #default="{ row }">
              <div v-if="row.userType === 'SUPER_ADMIN'" class="permission-tags">
                <ElTag type="danger">{{ t('全部权限') }}</ElTag>
              </div>
              <div v-else-if="row.permissions.length" class="permission-tags">
                <ElTag v-for="permission in row.permissions" :key="permission" effect="plain">
                  {{ permissionLabel(permission) }}
                </ElTag>
              </div>
              <span v-else class="muted-text">{{ t('未分配权限') }}</span>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('状态')" width="100">
            <template #default="{ row }">
              <ElTag :type="row.enabled ? 'success' : 'info'">
                {{ row.enabled ? t('已启用') : t('已停用') }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('密码状态')" width="120">
            <template #default="{ row }">
              <ElTag v-if="row.passwordResetRequired" type="warning">{{ t('等待首次改密') }}</ElTag>
              <span v-else>{{ t('正常') }}</span>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('最近登录')" min-width="150">
            <template #default="{ row }">{{ formatDateTime(row.lastLoginAt) }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('操作')" width="170" fixed="right">
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
          <ElFormItem :label="t('账号类型')">
            <ElSwitch
              v-model="createForm.isSuperAdmin"
              :active-text="t('超级管理员')"
              :inactive-text="t('工作人员')"
            />
          </ElFormItem>
          <ElFormItem v-if="!createForm.isSuperAdmin" :label="t('权限')" prop="permissions">
            <ElCheckboxGroup v-model="createForm.permissions" class="permission-grid">
              <ElCheckbox
                v-for="option in permissionOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </ElCheckbox>
            </ElCheckboxGroup>
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
          <ElFormItem :label="t('账号类型')">
            <ElSwitch
              v-model="editForm.isSuperAdmin"
              :active-text="t('超级管理员')"
              :inactive-text="t('工作人员')"
            />
          </ElFormItem>
          <ElFormItem v-if="!editForm.isSuperAdmin" :label="t('权限')">
            <ElCheckboxGroup v-model="editForm.permissions" class="permission-grid">
              <ElCheckbox
                v-for="option in permissionOptions"
                :key="option.value"
                :value="option.value"
              >
                {{ option.label }}
              </ElCheckbox>
            </ElCheckboxGroup>
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
import type { PermissionCode, StaffAccountResponse } from '../api/types';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

const { t } = useI18n();
const permissionOptions = computed((): Array<{ value: PermissionCode; label: string }> => [
  { value: 'CONTEST_MANAGE', label: t('赛事管理') },
  { value: 'CLARIFICATION_MANAGE', label: t('答疑处理') },
  { value: 'PRINTING_MANAGE', label: t('打印处理') },
  { value: 'BALLOON_MANAGE', label: t('气球配送') },
  { value: 'RESOLVER_MANAGE', label: t('滚榜操作') },
  { value: 'AWARD_MANAGE', label: t('颁奖管理') },
  { value: 'SCREEN_MANAGE', label: t('大屏管理') },
  { value: 'LIVE_MANAGE', label: t('直播管理') },
]);

const accounts = ref<StaffAccountResponse[]>([]);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const createVisible = ref(false);
const editVisible = ref(false);
const resetVisible = ref(false);
const createFormRef = ref<FormInstance>();
const selected = ref<StaffAccountResponse | null>(null);
const resetPassword = ref('');
const createForm = reactive({
  username: '',
  displayName: '',
  isSuperAdmin: false,
  permissions: [] as PermissionCode[],
  initialPassword: '',
  requirePasswordReset: true,
});
const resetRequirePasswordReset = ref(true);
const editForm = reactive({
  displayName: '',
  isSuperAdmin: false,
  permissions: [] as PermissionCode[],
  enabled: true,
});
const createRules = computed<FormRules>(() => ({
  username: [
    { required: true, message: t('请输入用户名'), trigger: 'blur' },
    {
      pattern: /^[A-Za-z0-9._-]{3,64}$/,
      message: t('请输入 3 至 64 位字母、数字或 ._-'),
      trigger: 'blur',
    },
  ],
  displayName: [{ required: true, message: t('请输入显示名称'), trigger: 'blur' }],
  initialPassword: [
    { required: true, message: t('请输入初始密码'), trigger: 'blur' },
    { min: 8, max: 128, message: t('密码长度需为 8 至 128 位'), trigger: 'blur' },
  ],
}));

function permissionLabel(permission: PermissionCode) {
  return permissionOptions.value.find((option) => option.value === permission)?.label ?? permission;
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
    isSuperAdmin: false,
    permissions: [],
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
      permissions: createForm.isSuperAdmin ? [] : createForm.permissions,
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
  const account = row as StaffAccountResponse;
  selected.value = account;
  Object.assign(editForm, {
    displayName: account.displayName,
    isSuperAdmin: account.userType === 'SUPER_ADMIN',
    permissions: [...account.permissions],
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
      isSuperAdmin: editForm.isSuperAdmin,
      permissions: editForm.isSuperAdmin ? [] : editForm.permissions,
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
  const account = row as StaffAccountResponse;
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
.staff-account-identity {
  display: grid;
  gap: 0.2rem;
}
.staff-account-identity span {
  color: var(--muted);
  font-size: 0.8rem;
}
.form-help {
  color: var(--muted);
  font-size: 0.8rem;
}
.permission-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.permission-grid {
  display: grid;
  width: 100%;
  grid-template-columns: repeat(2, minmax(0, 1fr));
}
.muted-text {
  color: var(--muted);
}
.reset-password-form {
  margin-top: 1rem;
}
@media (max-width: 680px) {
  .admin-page-header {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
@media (max-width: 640px) {
  .permission-grid {
    grid-template-columns: minmax(0, 1fr);
  }
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
