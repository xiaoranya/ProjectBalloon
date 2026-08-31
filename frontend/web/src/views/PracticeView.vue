<template>
  <el-container direction="vertical" class="practice-page">
    <el-header height="auto" class="page-head"
      ><div>
        <h1>{{ t('日常练习') }}</h1>
      </div>
      <nav>
        <RouterLink to="/problem-bank">{{ t('题库') }}</RouterLink
        ><RouterLink to="/practice/virtual">{{ t('虚拟比赛') }}</RouterLink
        ><RouterLink to="/profile">{{ t('账户') }}</RouterLink
        ><button class="logout-link" type="button" @click="logout">
          {{ t('退出登录') }}
        </button>
      </nav></el-header
    >
    <el-main class="page-body">
      <div class="practice-stats">
        <div>
          <span>{{ t('已通过') }}</span
          ><strong>{{ solvedCount }}</strong
          ><small>/ {{ problems.length }}</small>
        </div>
        <div>
          <span>{{ t('总尝试') }}</span
          ><strong>{{ attemptCount }}</strong>
        </div>
        <div>
          <span>{{ t('已提交') }}</span
          ><strong>{{ submissions.length }}</strong>
        </div>
      </div>
      <ElRow :gutter="24" class="practice-layout">
        <ElCol :xs="24" :md="7"
          ><aside>
            <ElInput v-model="filter" clearable :placeholder="t('搜索题目')" /><ElRadioGroup
              v-model="problemFilter"
              size="small"
              ><ElRadioButton value="all">{{ t('全部') }}</ElRadioButton
              ><ElRadioButton value="favorites">{{ t('收藏') }}</ElRadioButton
              ><ElRadioButton value="unsolved">{{ t('未通过') }}</ElRadioButton></ElRadioGroup
            ><button
              v-for="problem in filteredProblems"
              :key="problem.id"
              :class="{ active: selected?.id === problem.id }"
              @click="select(problem)"
            >
              <strong>{{ problem.slug }}</strong
              ><span>{{ problem.title }}</span
              ><ElTag v-if="status(problem.id)?.solved" type="success" size="small">{{
                t('已通过')
              }}</ElTag>
            </button>
          </aside></ElCol
        >
        <ElCol :xs="24" :md="17"
          ><section v-if="selected" class="workspace">
            <div class="problem-head">
              <div>
                <h2>{{ selected.title }}</h2>
                <ElTag v-for="tag in selected.tags" :key="tag" size="small">{{ tag }}</ElTag
                ><ElTag v-if="selected.difficulty !== null" type="warning" size="small">{{
                  t('难度 {value}', { value: selected.difficulty })
                }}</ElTag>
              </div>
              <div class="problem-actions">
                <span v-if="status(selected.id)">{{
                  t('尝试 {count} 次 · 最佳 {score}', {
                    count: status(selected.id)?.attempts ?? 0,
                    score: status(selected.id)?.bestScore ?? 0,
                  })
                }}</span
                ><ElButton
                  :icon="Star"
                  circle
                  :type="favorites.has(selected.id) ? 'warning' : 'default'"
                  :title="t('收藏')"
                  @click="toggleFavorite"
                /><ElButton @click="openEditorial">{{ t('题解') }}</ElButton>
              </div>
            </div>
            <article class="statement" v-html="selected.statement || `<p>${t('暂无题面')}</p>`" />
            <div class="submit-panel">
              <div class="submit-toolbar">
                <ElSelect v-model="language" style="width: 140px"
                  ><ElOption
                    v-for="item in languages"
                    :key="item"
                    :label="item"
                    :value="item" /></ElSelect
                ><ElButton type="primary" :loading="submitting" @click="submit">{{
                  t('提交练习')
                }}</ElButton>
              </div>
              <CodeEditor v-model="source" :language="language" height="280px" /><ElAlert
                v-if="message"
                :title="message"
                :type="messageType"
                :closable="false"
              />
            </div>
          </section>
          <ElEmpty v-else :description="t('请选择题目')"
        /></ElCol>
      </ElRow>
      <section class="history">
        <div class="history-head">
          <h2>{{ t('提交记录') }}</h2>
          <ElButton link @click="loadSubmissions">{{ t('刷新') }}</ElButton>
        </div>
        <ElTable
          :data="submissions"
          stripe
          :empty-text="t('暂无练习提交')"
          @row-click="openSubmission"
        >
          <ElTableColumn prop="id" label="#" width="80" />
          <ElTableColumn prop="problemTitle" :label="t('题目')" min-width="180" />
          <ElTableColumn prop="language" :label="t('语言')" width="100" />
          <ElTableColumn :label="t('状态')" width="120"
            ><template #default="{ row }"
              ><ElTag
                :type="
                  row.verdict === 'ACCEPTED'
                    ? 'success'
                    : row.status === 'PENDING' || row.status === 'JUDGING'
                      ? 'warning'
                      : 'danger'
                "
                >{{ row.verdict || row.status }}</ElTag
              ></template
            ></ElTableColumn
          >
          <ElTableColumn :label="t('提交时间')" width="190">
            <template #default="{ row }">{{ formatDateTime(row.submittedAt) }}</template>
          </ElTableColumn>
        </ElTable>
      </section>
      <ElDrawer
        v-model="editorialVisible"
        :title="editorial?.title || t('题解')"
        size="min(760px,92vw)"
        ><article v-if="editorial" class="statement" v-html="editorial.bodyHtml"
      /></ElDrawer>
      <ElDrawer v-model="submissionVisible" :title="t('提交详情')" size="min(860px,94vw)"
        ><div v-if="submissionDetail" class="submission-detail">
          <ElDescriptions :column="2" border
            ><ElDescriptionsItem :label="t('题目')">{{
              submissionDetail.problemTitle
            }}</ElDescriptionsItem
            ><ElDescriptionsItem :label="t('语言')">{{
              submissionDetail.language
            }}</ElDescriptionsItem
            ><ElDescriptionsItem :label="t('状态')">{{
              submissionDetail.verdict || submissionDetail.status
            }}</ElDescriptionsItem
            ><ElDescriptionsItem label="SHA-256">{{
              submissionDetail.sourceSha256 || '--'
            }}</ElDescriptionsItem></ElDescriptions
          >
          <h3>{{ t('源码') }}</h3>
          <ElEmpty
            v-if="submissionDetail.language === 'output'"
            :description="t('输出题提交为 ZIP 归档，不支持在线查看')"
          /><CodeEditor
            v-else
            v-model="submissionDetail.source"
            :language="submissionDetail.language"
            readonly
            height="380px"
          />
          <h3>{{ t('判题记录') }}</h3>
          <ElTable :data="submissionDetail.judgements" size="small"
            ><ElTableColumn prop="version" :label="t('版本')" width="70" /><ElTableColumn
              prop="verdict"
              :label="t('结果')" /><ElTableColumn
              prop="totalTimeMs"
              :label="t('时间(ms)')"
              width="100" /><ElTableColumn prop="peakMemoryKb" :label="t('内存(KB)')" width="110"
          /></ElTable></div
      ></ElDrawer>
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { Star } from '@element-plus/icons-vue';
import CodeEditor from '../components/CodeEditor.vue';
import { getErrorMessage } from '../api/client';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';
import { useSession } from '../auth/session';
import {
  trainingApi,
  type BankProblem,
  type Editorial,
  type PracticeProgress,
  type PracticeSubmission,
  type PracticeSubmissionDetail,
} from '../api/training';
const route = useRoute(),
  router = useRouter(),
  session = useSession(),
  virtualSessionId = Number(route.query.virtualSessionId) || undefined,
  initialProblemId = Number(route.query.problemId) || undefined;
const problems = ref<BankProblem[]>([]),
  progress = ref<PracticeProgress[]>([]),
  submissions = ref<PracticeSubmission[]>([]),
  favorites = ref(new Set<number>()),
  selected = ref<BankProblem>(),
  filter = ref(''),
  problemFilter = ref<'all' | 'favorites' | 'unsolved'>('all'),
  language = ref('cpp'),
  source = ref(''),
  submitting = ref(false),
  message = ref(''),
  messageType = ref<'success' | 'error'>('success'),
  editorial = ref<Editorial>(),
  editorialVisible = ref(false),
  submissionDetail = ref<PracticeSubmissionDetail>(),
  submissionVisible = ref(false);
const { t } = useI18n();
const defaultLanguages = ['c', 'cpp', 'java', 'python'];
const languages = computed(() => {
  const allowed = selected.value?.languages;
  if (allowed && allowed.length > 0) return allowed;
  return defaultLanguages;
});
function status(id: number) {
  return progress.value.find((item) => item.problemId === id);
}
async function logout() {
  await session.logout();
  ElMessage.success(t('已退出登录'));
  await router.push('/login');
}
const solvedCount = computed(() => progress.value.filter((item) => item.solved).length),
  attemptCount = computed(() => progress.value.reduce((total, item) => total + item.attempts, 0));
const filteredProblems = computed(() =>
  problems.value.filter(
    (item) =>
      (problemFilter.value === 'all' ||
        (problemFilter.value === 'favorites'
          ? favorites.value.has(item.id)
          : !status(item.id)?.solved)) &&
      (!filter.value.trim() ||
        `${item.slug} ${item.title} ${item.tags.join(' ')}`
          .toLowerCase()
          .includes(filter.value.trim().toLowerCase())),
  ),
);
function select(item: BankProblem) {
  selected.value = item;
  source.value = '';
  message.value = '';
  language.value = languages.value.includes(language.value)
    ? language.value
    : (languages.value[0] ?? 'cpp');
}
async function loadSubmissions() {
  try {
    submissions.value = (await trainingApi.submissions()).content;
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function openSubmission(row: PracticeSubmission) {
  try {
    submissionDetail.value = await trainingApi.submission(row.id);
    submissionVisible.value = true;
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function load() {
  try {
    const [bank, states, saved] = await Promise.all([
      trainingApi.problemBank(0, 100),
      trainingApi.progress(),
      trainingApi.favorites(),
    ]);
    problems.value = bank.content;
    progress.value = states;
    favorites.value = new Set(saved.map((item) => item.id));
    await loadSubmissions();
    if (!selected.value) {
      const first =
        problems.value.find((item) => item.id === initialProblemId) ?? problems.value[0];
      if (first) select(first);
    }
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function toggleFavorite() {
  if (!selected.value) return;
  const next = !favorites.value.has(selected.value.id);
  try {
    await trainingApi.favorite(selected.value.id, next);
    const copy = new Set(favorites.value);
    if (next) copy.add(selected.value.id);
    else copy.delete(selected.value.id);
    favorites.value = copy;
  } catch (e) {
    ElMessage.error(getErrorMessage(e));
  }
}
async function openEditorial() {
  if (!selected.value) return;
  try {
    editorial.value = await trainingApi.editorial(selected.value.id);
    editorialVisible.value = true;
  } catch (e) {
    ElMessage.warning(getErrorMessage(e));
  }
}
async function submit() {
  if (!selected.value || !source.value.trim()) return;
  submitting.value = true;
  try {
    await trainingApi.submit(
      selected.value.id,
      language.value,
      source.value,
      undefined,
      virtualSessionId,
    );
    messageType.value = 'success';
    message.value = t('已提交，判题完成后会更新练习进度');
    await load();
  } catch (e) {
    messageType.value = 'error';
    message.value = getErrorMessage(e);
  } finally {
    submitting.value = false;
  }
}
onMounted(load);
</script>
<style scoped>
.practice-page {
  max-width: 1280px;
  margin: 0 auto;
}
.page-head {
  display: flex;
  justify-content: space-between;
  align-items: end;
  border-bottom: 1px solid var(--border);
  padding: 28px 20px 18px;
  margin-bottom: 20px;
  height: auto;
}
.page-body {
  padding: 0 20px 28px;
}
h1,
h2 {
  margin: 0;
}
.page-head p {
  color: var(--muted);
}
.page-head nav {
  display: flex;
  align-items: center;
  gap: 18px;
  font-size: 14px;
}
.page-head nav a {
  color: var(--primary);
  text-decoration: none;
}
.page-head nav a.router-link-active {
  font-weight: 600;
}
.logout-link {
  border: 0;
  background: transparent;
  padding: 0;
  color: var(--danger);
  font-size: 14px;
  cursor: pointer;
}
.logout-link:hover {
  text-decoration: underline;
}
.practice-stats {
  display: flex;
  gap: 12px;
  margin: 18px 0;
}
.practice-stats > div {
  display: flex;
  align-items: baseline;
  gap: 7px;
  background: #f5f7fa;
  border: 1px solid #ebeef5;
  padding: 10px 14px;
}
.practice-stats span,
.practice-stats small {
  color: var(--muted);
  font-size: 12px;
}
.practice-stats strong {
  font-size: 22px;
}
aside {
  display: grid;
  align-content: start;
  gap: 6px;
  border-right: 1px solid var(--border);
  padding-right: 16px;
}
aside > .el-input {
  margin-bottom: 4px;
}
aside button {
  border: 0;
  background: transparent;
  text-align: left;
  padding: 11px;
  display: grid;
  gap: 4px;
  cursor: pointer;
}
aside button.active {
  background: #ecf5ff;
  color: var(--primary);
}
aside button span {
  color: var(--muted);
  font-size: 13px;
}
aside button .el-tag {
  justify-self: start;
}
.workspace {
  min-width: 0;
}
.problem-head,
.problem-actions {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}
.problem-head .el-tag + .el-tag {
  margin-left: 6px;
}
.problem-actions span {
  color: var(--muted);
}
.statement {
  line-height: 1.7;
  border-bottom: 1px solid #ebeef5;
  padding-bottom: 20px;
}
.statement:deep(img) {
  max-width: 100%;
}
.submit-panel {
  margin-top: 20px;
}
.submit-toolbar {
  display: flex;
  justify-content: space-between;
  margin-bottom: 10px;
}
.history {
  margin-top: 28px;
  border-top: 1px solid #ebeef5;
  padding-top: 20px;
}
.history-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}
.submission-detail h3 {
  margin: 24px 0 10px;
}
@media (max-width: 760px) {
  .practice-stats {
    overflow: auto;
  }
  aside {
    border-right: 0;
    border-bottom: 1px solid var(--border);
    padding: 0 0 14px;
    max-height: 280px;
    overflow: auto;
  }
  .problem-head {
    display: block;
  }
  .problem-actions {
    justify-content: flex-start;
    margin-top: 10px;
  }
  .history :deep(.el-table) {
    font-size: 12px;
  }
}
</style>
