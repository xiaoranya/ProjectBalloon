<template>
  <el-container direction="vertical" class="contest-shell">
    <el-header height="auto" class="contest-header">
      <div class="brand" @click="router.push('/contests')">
        <span class="brand-mark">PB</span>
        <div>
          <strong>{{ contest?.name ?? t('比赛工作台') }}</strong>
          <small v-if="contest">{{ contestStatusLabel(contest.status) }} · {{ timeHint }}</small>
        </div>
      </div>

      <nav class="contest-nav" :aria-label="t('比赛导航')">
        <RouterLink :to="contestRoute('problems')">{{ t('题目') }}</RouterLink>
        <RouterLink :to="contestRoute('submissions')">{{ t('提交记录') }}</RouterLink>
        <RouterLink :to="contestRoute('clarifications')">{{ t('答疑') }}</RouterLink>
        <RouterLink :to="contestRoute('printing')">{{ t('打印') }}</RouterLink>
        <RouterLink :to="contestRoute('scoreboard')">{{ t('榜单') }}</RouterLink>
      </nav>

      <ElDropdown trigger="click" @command="handleCommand">
        <button class="user-menu" type="button">
          <ElIcon><User /></ElIcon>
          {{ session.state.user?.displayName }}
          <ElIcon><ArrowDown /></ElIcon>
        </button>
        <template #dropdown>
          <ElDropdownMenu>
            <ElDropdownItem command="contests">{{ t('切换比赛') }}</ElDropdownItem>
            <ElDropdownItem command="logout" divided>{{ t('退出登录') }}</ElDropdownItem>
          </ElDropdownMenu>
        </template>
      </ElDropdown>
    </el-header>

    <el-main class="contest-content">
      <ElAlert
        v-if="loadError"
        :title="loadError"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />
      <RouterView v-else :contest="contest" />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { ContestResponse } from '../api/types';
import { useSession } from '../auth/session';
import { contestStatusLabel } from '../utils/format';
import { useI18n } from '../i18n';

const route = useRoute();
const router = useRouter();
const session = useSession();
const { t } = useI18n();
const contest = ref<ContestResponse | null>(null);
const loadError = ref('');
const now = ref(Date.now());
let clock: number | undefined;

const contestId = computed(() => Number(route.params.contestId));
const timeHint = computed(() => {
  if (!contest.value) return '';
  const afterStart = ['RUNNING', 'PAUSED', 'ENDED', 'ARCHIVED'].includes(contest.value.status);
  const target = afterStart ? contest.value.endAt : contest.value.startAt;
  if (!target) return t('未设置时间');
  const seconds = Math.max(0, Math.floor((new Date(target).getTime() - now.value) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (contest.value.status === 'ENDED' || contest.value.status === 'ARCHIVED')
    return t('比赛已结束');
  return t(afterStart ? '{hours} 小时 {minutes} 分钟后结束' : '{hours} 小时 {minutes} 分钟后开始', {
    hours,
    minutes,
  });
});

function contestRoute(child: string) {
  return `/contests/${contestId.value}/${child}`;
}

async function loadContest() {
  contest.value = null;
  try {
    contest.value = await contestApi.getContest(contestId.value);
    loadError.value = '';
  } catch (error) {
    loadError.value = getErrorMessage(error);
  }
}

async function handleCommand(command: string) {
  if (command === 'contests') {
    await router.push('/contests');
    return;
  }
  if (command === 'logout') {
    // session.logout() clears the session in its finally block even when the
    // request fails; always toast and navigate so the UI never half-clears.
    try {
      await session.logout();
    } catch {
      // Swallowed deliberately: the outcome of the logout request does not
      // change the local session state or the destination.
    }
    ElMessage.success(t('已退出登录'));
    await router.push('/login');
  }
}

onMounted(() => {
  void loadContest();
  clock = window.setInterval(() => {
    now.value = Date.now();
  }, 30_000);
});

watch(contestId, () => void loadContest());

onUnmounted(() => {
  if (clock) window.clearInterval(clock);
});
</script>
