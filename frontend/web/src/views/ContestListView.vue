<template>
  <main class="dashboard-page">
    <header class="dashboard-header">
      <div>
        <p class="eyebrow">Contestant Dashboard</p>
        <h1>选择比赛</h1>
        <p>欢迎回来，{{ session.state.user?.displayName }}</p>
      </div>
      <ElButton plain @click="logout">退出登录</ElButton>
    </header>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert">
      <template #default>
        <ElButton link type="primary" @click="loadContests">重新加载</ElButton>
      </template>
    </ElAlert>

    <div v-loading="loading" class="contest-grid">
      <ElEmpty v-if="!loading && contests.length === 0" description="当前没有可访问的比赛" />
      <article v-for="contest in contests" :key="contest.id" class="contest-card">
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
  </main>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Contest } from '../api/types';
import { useSession } from '../auth/session';
import { contestStatusLabel, formatDateTime } from '../utils/format';

const router = useRouter();
const session = useSession();
const contests = ref<Contest[]>([]);
const loading = ref(false);
const errorMessage = ref('');

async function loadContests() {
  loading.value = true;
  errorMessage.value = '';
  try {
    const page = await contestApi.listContests();
    contests.value = page.content;
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
