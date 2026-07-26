<template>
  <section class="admin-page permissions-page">
    <header class="admin-page-header">
      <div>
        <p class="eyebrow">Access Control</p>
        <h1>比赛管理员授权</h1>
        <p>比赛管理员只能查看和管理这里明确分配的比赛。</p>
      </div>
      <ElButton :icon="Refresh" :loading="loading" @click="load">刷新</ElButton>
    </header>

    <ElAlert
      v-if="errorMessage"
      :title="errorMessage"
      type="error"
      show-icon
      :closable="false"
      class="page-alert"
    />

    <div v-loading="loading" class="permission-admin-list">
      <ElCard
        v-for="admin in admins"
        :key="admin.userId"
        shadow="never"
        class="admin-card permission-admin-card"
      >
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
          <ElCheckbox
            v-for="contest in contests"
            :key="contest.id"
            :value="contest.id"
            border
          >
            <span>{{ contest.name }}</span>
            <small>#{{ contest.id }} · {{ contestStatusLabel(contest.status) }}</small>
          </ElCheckbox>
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
  </section>
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
  return JSON.stringify(normalized(draftScopes[admin.userId] ?? []))
    !== JSON.stringify(normalized(admin.contestIds));
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
