<template>
  <el-container direction="vertical" class="virtual-page">
    <el-header height="auto" class="page-head">
      <div>
        <h1>{{ t('虚拟比赛') }}</h1>
      </div>
      <ElButton type="primary" @click="dialog = true">{{ t('新建虚拟赛') }}</ElButton>
    </el-header>
    <el-main class="page-body">
      <div class="layout">
        <aside class="session-nav">
          <ElMenu :default-active="activeId" class="session-menu" @select="onMenuSelect">
            <ElMenuItem v-for="item in sessions" :key="item.id" :index="String(item.id)">
              <div class="session-item">
                <strong>{{ item.title }}</strong>
                <span
                  >{{ item.solvedProblems }} / {{ item.totalProblems }} · {{ item.status }}</span
                >
              </div>
            </ElMenuItem>
          </ElMenu>
        </aside>
        <section v-if="selected">
          <div class="session-head">
            <div>
              <h2>{{ selected.session.title }}</h2>
              <ElTag v-if="selected.session.status === 'ARCHIVED'" type="info">{{
                t('已归档')
              }}</ElTag>
            </div>
            <div class="session-actions">
              <strong>{{ countdown }}</strong>
              <ElButton
                v-if="selected.session.status !== 'ARCHIVED'"
                link
                type="danger"
                @click="archive"
                >{{ t('归档') }}</ElButton
              >
            </div>
          </div>
          <ElTable :data="selected.problems" row-key="problemId">
            <ElTableColumn prop="position" label="#" width="60" />
            <ElTableColumn prop="title" :label="t('题目')" />
            <ElTableColumn prop="attempts" :label="t('尝试')" width="80" />
            <ElTableColumn :label="t('状态')" width="90">
              <template #default="{ row }">
                <ElTag :type="row.solved ? 'success' : 'info'">{{
                  row.solved ? t('已通过') : t('未通过')
                }}</ElTag>
              </template>
            </ElTableColumn>
            <ElTableColumn width="90">
              <template #default="{ row }">
                <RouterLink
                  v-if="selected?.session.status === 'RUNNING'"
                  :to="`/practice?virtualSessionId=${selected?.session.id}&problemId=${row.problemId}`"
                  >{{ t('作答') }}</RouterLink
                >
                <span v-else>{{ t('只读') }}</span>
              </template>
            </ElTableColumn>
          </ElTable>
        </section>
        <ElEmpty v-else :description="t('暂无虚拟赛')" />
      </div>
    </el-main>
    <ElDialog v-model="dialog" :title="t('新建虚拟赛')" width="min(620px, 92vw)">
      <ElForm label-position="top">
        <ElFormItem :label="t('名称')">
          <ElInput v-model="form.title" maxlength="255" />
        </ElFormItem>
        <ElFormItem :label="t('时长（分钟）')">
          <ElInputNumber v-model="form.durationMinutes" :min="15" :max="10080" />
        </ElFormItem>
        <ElFormItem :label="t('题目')">
          <ElSelect v-model="form.problemIds" multiple filterable style="width: 100%">
            <ElOption
              v-for="item in problems"
              :key="item.id"
              :label="`${item.slug} · ${item.title}`"
              :value="item.id"
            />
          </ElSelect>
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="dialog = false">{{ t('取消') }}</ElButton>
        <ElButton type="primary" @click="create">{{ t('开始') }}</ElButton>
      </template>
    </ElDialog>
  </el-container>
</template>
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { getErrorMessage } from '../api/client';
import { useI18n } from '../i18n';
import {
  trainingApi,
  type BankProblem,
  type VirtualSession,
  type VirtualSessionDetail,
} from '../api/training';
const { t } = useI18n();
const sessions = ref<VirtualSession[]>([]),
  selected = ref<VirtualSessionDetail>(),
  problems = ref<BankProblem[]>([]),
  dialog = ref(false),
  now = ref(Date.now()),
  form = reactive({ title: t('个人虚拟赛'), durationMinutes: 300, problemIds: [] as number[] }),
  activeId = ref('');
let timer: number | undefined;
const countdown = computed(() => {
  if (!selected.value || selected.value.session.status !== 'RUNNING')
    return selected.value?.session.status === 'ARCHIVED' ? t('已归档') : t('已结束');
  const seconds = Math.max(
    0,
    Math.floor((new Date(selected.value.session.endAt).getTime() - now.value) / 1000),
  );
  return [Math.floor(seconds / 3600), Math.floor((seconds % 3600) / 60), seconds % 60]
    .map((v) => String(v).padStart(2, '0'))
    .join(':');
});
function onMenuSelect(index: string) {
  void open(Number(index));
}
async function open(id: number) {
  activeId.value = String(id);
  try {
    selected.value = await trainingApi.virtualSession(id);
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function load() {
  try {
    const [list, bank] = await Promise.all([
      trainingApi.virtualSessions(),
      trainingApi.problemBank(0, 100),
    ]);
    sessions.value = list;
    problems.value = bank.content;
    if (!selected.value && list[0]) await open(list[0].id);
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function archive() {
  if (!selected.value) return;
  try {
    await ElMessageBox.confirm(t('归档后将不能继续提交，只保留查看结果。'), t('归档虚拟赛'), {
      type: 'warning',
      confirmButtonText: t('归档'),
    });
    await trainingApi.archiveVirtualSession(selected.value.session.id);
    await load();
    await open(selected.value.session.id);
    ElMessage.success(t('虚拟赛已归档'));
  } catch (e) {
    if (e !== 'cancel' && e !== 'close') ElMessage.error(getErrorMessage(e));
  }
}
async function create() {
  if (!form.problemIds.length) return;
  try {
    const value = await trainingApi.createVirtualSession(form);
    dialog.value = false;
    await load();
    await open(value.id);
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
onMounted(() => {
  void load();
  timer = window.setInterval(() => (now.value = Date.now()), 1000);
});
onBeforeUnmount(() => {
  if (timer) clearInterval(timer);
});
</script>
<style scoped>
.virtual-page {
  max-width: 1180px;
  margin: 0 auto;
}
.page-head {
  display: flex;
  justify-content: space-between;
  align-items: end;
  border-bottom: 1px solid var(--border);
  padding: 28px 20px 18px;
}
.page-body {
  padding: 0 20px 28px;
}
.page-head h1 {
  margin: 0;
  font-size: 28px;
}
.layout {
  display: grid;
  grid-template-columns: 260px 1fr;
  gap: 24px;
}
.session-nav {
  border-right: 1px solid var(--border);
  padding-right: 12px;
}
.session-menu {
  border-right: none;
}
.session-item {
  display: flex;
  flex-direction: column;
  width: 100%;
}
.session-item span {
  color: var(--muted);
  font-size: 13px;
}
.session-head {
  display: flex;
  justify-content: space-between;
  align-items: end;
  border-bottom: 1px solid var(--border);
  padding-bottom: 18px;
  margin-bottom: 20px;
}
.session-head h2 {
  margin: 0;
}
.session-actions strong {
  font:
    600 28px ui-monospace,
    monospace;
}
@media (max-width: 720px) {
  .layout {
    grid-template-columns: 1fr;
  }
  .session-nav {
    border-right: 0;
    border-bottom: 1px solid var(--border);
  }
}
</style>
