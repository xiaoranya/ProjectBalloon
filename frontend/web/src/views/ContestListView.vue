<template>
  <el-container direction="vertical" class="dashboard-page">
    <el-header height="auto" class="page-head">
      <header class="dashboard-header">
        <div>
          <p class="eyebrow">Contestant Dashboard</p>
          <h1>选择比赛</h1>
          <p>欢迎回来，{{ session.state.user?.displayName }}</p>
        </div>
        <ElButton plain @click="logout">退出登录</ElButton>
      </header>
    </el-header>

    <el-main class="page-body">
      <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert">
        <template #default>
          <ElButton link type="primary" @click="loadContests">重新加载</ElButton>
        </template>
      </ElAlert>

      <div v-loading="loading" class="contest-grid">
        <ElEmpty v-if="!loading && page.content.length === 0" description="当前没有可访问的比赛" />
        <article v-for="contest in page.content" :key="contest.id" class="contest-card">
          <div class="contest-card-top">
            <ElTag :type="contest.status === 'RUNNING' ? 'success' : 'info'" effect="light">
              {{ contestStatusLabel(contest.status) }}
            </ElTag>
            <span>{{ contest.visibility === 'PRIVATE' ? '私有比赛' : '公开比赛' }}</span>
          </div>
          <h2>{{ contest.name }}</h2>
          <dl class="contest-times">
            <div><dt>开始</dt><dd>{{ formatDateTime(contest.startAt) }}</dd></div>
            <div><dt>封榜</dt><dd>{{ formatDateTime(contest.freezeAt) }}</dd></div>
            <div><dt>结束</dt><dd>{{ formatDateTime(contest.endAt) }}</dd></div>
          </dl>
          <ElButton type="primary" size="large" @click="enterContest(contest.id)">进入比赛</ElButton>
        </article>
      </div>
      <ElRow v-if="page.totalPages > 1" justify="end" class="pagination-row">
        <ElPagination
          v-model:current-page="currentPage"
          :page-size="page.size"
          :total="page.totalElements"
          layout="prev, pager, next, total"
          @current-change="loadContests"
        />
      </ElRow>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Contest, PageResponse } from '../api/types';
import { useSession } from '../auth/session';
import { contestStatusLabel, formatDateTime } from '../utils/format';

const router = useRouter();
const session = useSession();
const page = ref<PageResponse<Contest>>({ content: [], page: 0, size: 50, totalElements: 0, totalPages: 0 });
const currentPage = ref(1);
const loading = ref(false);
const errorMessage = ref('');

async function loadContests() {
  loading.value = true;
  errorMessage.value = '';
  try {
    page.value = await contestApi.listContests(currentPage.value - 1, page.value.size);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

function enterContest(contestId: number) {
  void router.push(`/contests/${contestId}/problems`);
}

async function logout() {
  await session.logout();
  await router.push('/login');
}

onMounted(loadContests);
</script>

<style scoped>
.dashboard-page {
  min-height: 100vh;
}

.page-head {
  height: auto;
  padding: clamp(28px, 6vw, 84px) clamp(28px, 6vw, 84px) 0;
}

.page-body {
  padding: 0 clamp(28px, 6vw, 84px) clamp(28px, 6vw, 84px);
}

.dashboard-header {
  display: flex;
  max-width: 1180px;
  align-items: flex-start;
  justify-content: space-between;
  margin: 0 auto 42px;
}

.dashboard-header h1 {
  margin-bottom: 8px;
  font-size: clamp(32px, 4vw, 48px);
  letter-spacing: -0.035em;
}

.dashboard-header p {
  display: none;
  margin-bottom: 0;
  color: var(--muted);
}

.eyebrow {
  display: none;
  margin: 0 0 8px;
  color: var(--primary);
  font-size: 12px;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

.page-alert {
  margin-bottom: 20px;
}

.contest-grid {
  display: grid;
  width: 100%;
  max-width: 1180px;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 22px;
  margin: 0 auto;
}

.contest-card {
  display: flex;
  min-height: 310px;
  flex-direction: column;
  padding: 28px;
  border: 1px solid var(--border);
  border-radius: 0;
  background: var(--surface);
  box-shadow: 0 16px 50px rgb(35 55 80 / 7%);
}

.contest-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--muted);
  font-size: 13px;
}

.contest-card h2 {
  margin: 28px 0 22px;
  font-size: 25px;
  line-height: 1.25;
}

.contest-card > :deep(.el-button) {
  margin-top: auto;
}

.contest-times {
  display: grid;
  gap: 10px;
  margin: 0 0 28px;
}

.contest-times div {
  display: flex;
  justify-content: space-between;
}

.contest-times dt {
  color: var(--muted);
}

.contest-times dd {
  margin: 0;
  font-variant-numeric: tabular-nums;
}

.pagination-row {
  margin-top: 24px;
}

@media (max-width: 640px) {
  .dashboard-page {
    min-height: 100vh;
  }

  .page-head {
    padding: 22px 22px 0;
  }

  .page-body {
    padding: 0 22px 22px;
  }

  .dashboard-header {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
