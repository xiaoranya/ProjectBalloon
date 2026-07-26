<template>
  <section class="judge-page clarification-judge-page">
    <div class="page-title-row">
      <div>
        <p class="eyebrow">Clarification Desk</p>
        <h1>答疑工作台</h1>
        <p>筛选比赛问题，查看队伍信息并完成回复、关闭或转公告。</p>
      </div>
      <div class="clarification-live-state" :class="{ connected: realtimeConnected }" aria-live="polite">
        <span />{{ realtimeConnected ? '实时更新' : '轮询更新' }}
      </div>
    </div>

    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <ElCard shadow="never" class="clarification-filter-card">
      <div class="clarification-toolbar">
        <ElSelect v-model="selectedContestId" filterable placeholder="选择比赛" @change="changeContest">
          <ElOption
            v-for="contest in contests"
            :key="contest.id"
            :label="contest.name"
            :value="contest.id"
          />
        </ElSelect>
        <ElRadioGroup v-model="statusFilter" @change="loadClarifications(false)">
          <ElRadioButton value="ALL">全部</ElRadioButton>
          <ElRadioButton value="PENDING">待回复</ElRadioButton>
          <ElRadioButton value="ANSWERED">已回复</ElRadioButton>
          <ElRadioButton value="CLOSED">已关闭</ElRadioButton>
        </ElRadioGroup>
        <ElButton :icon="Refresh" :loading="loading" :disabled="!selectedContestId" @click="loadClarifications(false)">
          刷新
        </ElButton>
      </div>
    </ElCard>

    <ElCard shadow="never" class="clarification-list-card">
      <ElTable
        v-loading="loading"
        :data="clarifications"
        row-key="id"
        empty-text="当前筛选下没有答疑"
        highlight-current-row
        @row-click="openDetail"
      >
        <ElTableColumn prop="id" label="#" width="76" />
        <ElTableColumn label="状态" width="110">
          <template #default="{ row }"><ElTag :type="statusType(row.status)">{{ statusLabel(row.status) }}</ElTag></template>
        </ElTableColumn>
        <ElTableColumn label="范围" width="100">
          <template #default="{ row }">{{ row.problemAlias ? `题目 ${row.problemAlias}` : '通用' }}</template>
        </ElTableColumn>
        <ElTableColumn label="队伍" min-width="180">
          <template #default="{ row }">
            <div class="admin-primary-cell"><strong>{{ row.teamName ?? `队伍 #${row.teamId}` }}</strong><small>队伍 ID {{ row.teamId }}</small></div>
          </template>
        </ElTableColumn>
        <ElTableColumn prop="question" label="问题" min-width="300" show-overflow-tooltip />
        <ElTableColumn label="提问时间" min-width="170">
          <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
        </ElTableColumn>
        <ElTableColumn label="操作" width="110" fixed="right">
          <template #default="{ row }"><ElButton link type="primary" @click.stop="openDetail(row as Clarification)">查看详情</ElButton></template>
        </ElTableColumn>
      </ElTable>
    </ElCard>

    <ElDrawer v-model="detailVisible" title="答疑详情" size="min(620px, 94vw)" @closed="selected = null">
      <ElSkeleton v-if="detailLoading" :rows="8" animated />
      <div v-else-if="selected" class="clarification-detail">
        <div class="clarification-card-meta">
          <div>
            <ElTag :type="statusType(selected.status)">{{ statusLabel(selected.status) }}</ElTag>
            <ElTag type="info" effect="plain">{{ selected.problemAlias ? `题目 ${selected.problemAlias}` : '通用问题' }}</ElTag>
          </div>
          <span>版本 {{ selected.version }}</span>
        </div>
        <h3>提问队伍</h3>
        <p class="clarification-detail-meta">
          {{ selected.teamName ?? '未命名队伍' }}（队伍 #{{ selected.teamId }}） · 用户 #{{ selected.askedByUserId }} · {{ formatDateTime(selected.createdAt) }}
        </p>
        <h3>问题</h3>
        <div class="clarification-bubble question">{{ selected.question }}</div>
        <template v-if="selected.reply">
          <h3>当前回复</h3>
          <div class="clarification-bubble reply">{{ selected.reply }}</div>
          <p class="clarification-detail-meta">
            {{ selected.replyVisibility === 'PUBLIC' ? '公开回复' : '仅该队可见' }} · 回复用户 #{{ selected.repliedByUserId }} · {{ formatDateTime(selected.repliedAt) }}
          </p>
        </template>

        <ElForm v-if="selected.status !== 'CLOSED'" label-position="top" class="clarification-reply-form">
          <ElFormItem label="回复内容">
            <ElInput v-model="replyForm.reply" type="textarea" :rows="6" maxlength="8000" show-word-limit />
          </ElFormItem>
          <ElFormItem label="可见范围">
            <ElRadioGroup v-model="replyForm.visibility">
              <ElRadio value="PRIVATE">仅该队可见</ElRadio>
              <ElRadio value="PUBLIC">公开回复</ElRadio>
            </ElRadioGroup>
          </ElFormItem>
          <ElButton type="primary" :loading="replying" :disabled="!replyForm.reply.trim()" @click="reply">
            {{ selected.reply ? '更新回复' : '提交回复' }}
          </ElButton>
        </ElForm>

        <div class="clarification-actions">
          <ElButton
            v-if="selected.status !== 'CLOSED'"
            type="warning"
            plain
            :loading="closing"
            @click="closeClarification"
          >关闭问题</ElButton>
          <ElButton
            v-if="canConvert"
            type="success"
            plain
            @click="openConversion"
          >转为公告</ElButton>
          <ElTag v-if="selected.convertedAnnouncementId" type="success">
            已转公告 #{{ selected.convertedAnnouncementId }}
          </ElTag>
        </div>
      </div>
    </ElDrawer>

    <ElDialog v-model="conversionVisible" title="转为比赛公告" width="min(580px, 92vw)">
      <ElAlert title="仅公开回复可转为公告；留空时后端将使用问题摘要和当前回复。" type="info" show-icon :closable="false" class="form-alert" />
      <ElForm label-position="top">
        <ElFormItem label="公告标题（可选）"><ElInput v-model="conversionForm.title" maxlength="255" show-word-limit /></ElFormItem>
        <ElFormItem label="公告正文（可选）"><ElInput v-model="conversionForm.body" type="textarea" :rows="6" maxlength="16000" show-word-limit /></ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="conversionVisible = false">取消</ElButton>
        <ElButton type="success" :loading="converting" @click="convert">发布公告</ElButton>
      </template>
    </ElDialog>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Refresh } from '@element-plus/icons-vue';
import { clarificationApi, type Clarification, type ClarificationReplyVisibility, type ClarificationStatus } from '../api/clarifications';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import type { Contest } from '../api/types';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';

const route = useRoute();
const router = useRouter();
const contests = ref<Contest[]>([]);
const selectedContestId = ref<number | null>(null);
const statusFilter = ref<ClarificationStatus | 'ALL'>('PENDING');
const clarifications = ref<Clarification[]>([]);
const selected = ref<Clarification | null>(null);
const loading = ref(false);
const detailLoading = ref(false);
const detailVisible = ref(false);
const replying = ref(false);
const closing = ref(false);
const converting = ref(false);
const conversionVisible = ref(false);
const errorMessage = ref('');
const realtimeConnected = ref(false);
const replyForm = reactive<{ reply: string; visibility: ClarificationReplyVisibility }>({ reply: '', visibility: 'PRIVATE' });
const conversionForm = reactive({ title: '', body: '' });
let realtime: ContestRealtimeSubscription | undefined;

const canConvert = computed(() => selected.value?.status === 'ANSWERED'
  && selected.value.replyVisibility === 'PUBLIC'
  && selected.value.convertedAnnouncementId === null);

function statusLabel(status: ClarificationStatus) {
  return { PENDING: '待回复', ANSWERED: '已回复', CLOSED: '已关闭' }[status];
}

function statusType(status: ClarificationStatus): 'warning' | 'success' | 'info' {
  return status === 'PENDING' ? 'warning' : status === 'ANSWERED' ? 'success' : 'info';
}

async function loadClarifications(silent = true) {
  if (!selectedContestId.value) return;
  if (!silent) loading.value = true;
  try {
    clarifications.value = await clarificationApi.listAll(
      selectedContestId.value,
      statusFilter.value === 'ALL' ? undefined : statusFilter.value,
    );
    errorMessage.value = '';
    if (selected.value) {
      const changed = clarifications.value.find((item) => item.id === selected.value?.id);
      if (changed) selected.value = changed;
    }
  } catch (error) {
    if (!silent) errorMessage.value = getErrorMessage(error);
  } finally {
    if (!silent) loading.value = false;
  }
}

function connectRealtime() {
  realtime?.stop();
  realtimeConnected.value = false;
  if (!selectedContestId.value) return;
  realtime = subscribeContestEvents({
    contestId: selectedContestId.value,
    scope: 'STAFF',
    eventTypes: ['CLARIFICATION_UPDATED'],
    onEvent: (event) => {
      void loadClarifications();
      if (selected.value?.id === Number(event.payload.clarificationId)) void refreshDetail(selected.value.id);
    },
    onConnectionChange: (connected) => { realtimeConnected.value = connected; },
    poll: () => loadClarifications(),
  });
}

async function changeContest() {
  await router.replace({ query: selectedContestId.value ? { contestId: String(selectedContestId.value) } : {} });
  selected.value = null;
  detailVisible.value = false;
  connectRealtime();
  await loadClarifications(false);
}

async function refreshDetail(id: number) {
  try {
    selected.value = await clarificationApi.get(id);
    replyForm.reply = selected.value.reply ?? '';
    replyForm.visibility = selected.value.replyVisibility ?? 'PRIVATE';
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

async function openDetail(row: Clarification) {
  detailVisible.value = true;
  detailLoading.value = true;
  try {
    await refreshDetail(row.id);
  } finally {
    detailLoading.value = false;
  }
}

async function reply() {
  if (!selected.value || !replyForm.reply.trim()) return;
  replying.value = true;
  try {
    selected.value = await clarificationApi.reply(selected.value.id, {
      reply: replyForm.reply.trim(),
      visibility: replyForm.visibility,
    });
    ElMessage.success('回复已保存');
    await loadClarifications();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    replying.value = false;
  }
}

async function closeClarification() {
  if (!selected.value) return;
  try {
    await ElMessageBox.confirm('关闭后不能再次回复，确定继续吗？', '关闭答疑', { type: 'warning' });
  } catch {
    return;
  }
  closing.value = true;
  try {
    const id = selected.value.id;
    await clarificationApi.close(id);
    await refreshDetail(id);
    await loadClarifications();
    ElMessage.success('问题已关闭');
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    closing.value = false;
  }
}

function openConversion() {
  conversionForm.title = '';
  conversionForm.body = '';
  conversionVisible.value = true;
}

async function convert() {
  if (!selected.value) return;
  converting.value = true;
  try {
    const announcement = await clarificationApi.convert(selected.value.id, {
      title: conversionForm.title.trim() || null,
      body: conversionForm.body.trim() || null,
    });
    selected.value = { ...selected.value, convertedAnnouncementId: announcement.id };
    conversionVisible.value = false;
    await loadClarifications();
    ElMessage.success(`已发布公告 #${announcement.id}`);
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    converting.value = false;
  }
}

onMounted(async () => {
  loading.value = true;
  try {
    const page = await contestApi.listContests();
    contests.value = page.content;
    const queryContestId = Number(route.query.contestId);
    selectedContestId.value = contests.value.some((contest) => contest.id === queryContestId)
      ? queryContestId
      : (contests.value[0]?.id ?? null);
    if (selectedContestId.value) {
      connectRealtime();
      await loadClarifications();
    }
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => realtime?.stop());
</script>
