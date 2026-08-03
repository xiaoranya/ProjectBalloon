<template>
  <el-container direction="vertical" class="admin-page permissions-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>比赛管理员授权</h1>
        <ElButton :icon="Refresh" :loading="loading" @click="load">刷新</ElButton>
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

      <div v-loading="loading" class="permission-admin-list">
        <ElCard v-for="admin in admins" :key="admin.userId" shadow="never">
          <template #header>
            <div class="permission-admin-heading">
              <div>
                <strong>{{ admin.displayName }}</strong>
                <span>@{{ admin.username }} · #{{ admin.userId }}</span>
              </div>
              <ElTag :type="admin.enabled ? 'success' : 'info'">
                {{ admin.enabled ? '已启用' : '已停用' }}
              </ElTag>
            </div>
          </template>

          <ElCheckboxGroup v-model="draftScopes[admin.userId]" class="permission-contest-grid">
            <ElRow :gutter="12">
              <ElCol
                v-for="contest in contests"
                :key="contest.id"
                :xs="24"
                :sm="12"
                :md="8"
                :lg="6"
              >
                <ElCheckbox :value="contest.id" border>
                  <span>{{ contest.name }}</span>
                  <small>#{{ contest.id }} · {{ contestStatusLabel(contest.status) }}</small>
                </ElCheckbox>
              </ElCol>
            </ElRow>
          </ElCheckboxGroup>

          <ElEmpty v-if="!contests.length" description="尚无可分配比赛" />

          <div class="permission-admin-actions">
            <span>已选择 {{ draftScopes[admin.userId]?.length ?? 0 }} 场比赛</span>
            <ElButton
              type="primary"
              :loading="savingUserId === admin.userId"
              :disabled="!changed(admin)"
              @click="save(admin)"
            >
              保存授权
            </ElButton>
          </div>
        </ElCard>

        <ElEmpty v-if="!loading && !admins.length" description="当前没有比赛管理员账号" />
      </div>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import { Refresh } from '@element-plus/icons-vue';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import type { Contest, ContestAdminScope } from '../api/types';
import { contestStatusLabel } from '../utils/format';

const loading = ref(false);
const savingUserId = ref<number | null>(null);
const errorMessage = ref('');
const admins = ref<ContestAdminScope[]>([]);
const contests = ref<Contest[]>([]);
const draftScopes = reactive<Record<number, number[]>>({});

async function load() {
  loading.value = true;
  errorMessage.value = '';
  try {
    const [adminRows, contestPage] = await Promise.all([
      adminApi.listContestAdminScopes(),
      adminApi.listContests(0, 500),
    ]);
    admins.value = adminRows;
    contests.value = contestPage.content;
    for (const admin of adminRows) {
      draftScopes[admin.userId] = [...admin.contestIds];
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function normalized(values: number[]) {
  return [...values].sort((left, right) => left - right);
}

function changed(admin: ContestAdminScope) {
  return (
    JSON.stringify(normalized(draftScopes[admin.userId] ?? [])) !==
    JSON.stringify(normalized(admin.contestIds))
  );
}

async function save(admin: ContestAdminScope) {
  savingUserId.value = admin.userId;
  errorMessage.value = '';
  try {
    const updated = await adminApi.updateContestAdminScope(
      admin.userId,
      normalized(draftScopes[admin.userId] ?? []),
    );
    const index = admins.value.findIndex((row) => row.userId === admin.userId);
    admins.value[index] = updated;
    draftScopes[admin.userId] = [...updated.contestIds];
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    savingUserId.value = null;
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
.page-alert {
  margin-bottom: 20px;
}
.permission-admin-heading,
.permission-admin-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.permission-admin-heading strong,
.permission-admin-heading span {
  display: block;
}
.permission-admin-heading span,
.permission-admin-actions span {
  margin-top: 4px;
  color: var(--muted);
  font-size: 13px;
}
.permission-contest-grid :deep(.el-checkbox) {
  width: 100%;
  height: auto;
  min-height: 58px;
  margin: 0;
  padding: 10px 14px;
}
.permission-contest-grid :deep(.el-checkbox__label span),
.permission-contest-grid :deep(.el-checkbox__label small) {
  display: block;
}
.permission-contest-grid :deep(.el-checkbox__label small) {
  margin-top: 3px;
  color: var(--muted);
}
.permission-admin-actions {
  margin-top: 18px;
  padding-top: 16px;
  border-top: 1px solid #edf0f5;
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
