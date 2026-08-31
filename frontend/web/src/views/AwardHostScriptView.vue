<template>
  <el-container direction="vertical" class="host-script-page">
    <el-header height="auto" class="host-script-page-header"
      ><div>
        <h1>{{ t('颁奖主持人脚本') }}</h1>
      </div>
      <ElSpace wrap :size="16" class="host-script-toolbar"
        ><ElSelect
          v-model="contestId"
          filterable
          :placeholder="t('选择比赛')"
          @change="changeContest"
          ><ElOption
            v-for="contest in contests"
            :key="contest.id"
            :label="contest.name"
            :value="contest.id" /></ElSelect
        ><ElButton :icon="Refresh" :loading="loading" @click="load(true)">{{ t('刷新') }}</ElButton
        ><ElButton :icon="Printer" :disabled="!script" @click="printScript">{{
          t('打印')
        }}</ElButton
        ><ElButton
          type="primary"
          :icon="Check"
          :disabled="!script || !dirty"
          :loading="saving"
          @click="save"
          >{{ t('保存脚本') }}</ElButton
        ></ElSpace
      ></el-header
    >
    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="warning"
        show-icon
        :closable="false"
      />
      <template v-if="script">
        <ElCard shadow="never" class="host-live-card"
          ><template #header
            ><div class="card-header">
              <div>
                <strong>{{ t('主持人当前提示') }}</strong
                ><small>{{ statusLabel }}</small>
              </div>
              <ElTag>{{ script.presentationStatus }}</ElTag>
            </div></template
          ><ElRow v-if="currentSection" :gutter="24" class="host-live-grid"
            ><ElCol :xs="24" :md="13"
              ><div class="host-current-cue">
                <small>{{ currentSection.code }}</small>
                <h2>{{ currentSection.name }}</h2>
                <p>{{ currentSection.cueText }}</p>
                <span>{{
                  nextSection
                    ? t('下一项：{name}', { name: nextSection.name })
                    : t('这是最后一个奖项')
                }}</span>
              </div></ElCol
            ><ElCol :xs="24" :md="11"
              ><div class="host-current-recipients">
                <strong>{{ t('请宣读') }}</strong>
                <ol>
                  <li v-for="recipient in currentSection.recipients" :key="recipient.id">
                    <span>{{ recipient.teamName }}</span
                    ><small>{{ recipient.school || t('未填写学校') }}</small>
                  </li>
                </ol>
              </div></ElCol
            ></ElRow
          ></ElCard
        >
        <ElCard shadow="never" class="host-script-editor"
          ><template #header
            ><div class="card-header">
              <div>
                <strong>{{ t('口播内容') }}</strong
                ><small>{{
                  t('乐观锁版本 v{version}', { version: script.version ?? t('草稿') })
                }}</small>
              </div>
            </div></template
          ><ElForm label-position="top"
            ><ElFormItem :label="t('开场语')"
              ><ElInput
                v-model="form.openingText"
                type="textarea"
                :rows="3"
                maxlength="4000"
                show-word-limit
                @input="dirty = true" /></ElFormItem
            ><ElRow :gutter="16" class="host-section-editor-list"
              ><ElCol v-for="section in form.sections" :key="section.categoryId" :xs="24" :md="12"
                ><article>
                  <header>
                    <div>
                      <small>{{ section.code }}</small
                      ><strong>{{ section.name }}</strong>
                    </div>
                    <ElTag v-if="section.firstBlood" type="danger">First Blood</ElTag>
                  </header>
                  <ElInput
                    v-model="section.cueText"
                    type="textarea"
                    :rows="3"
                    maxlength="2000"
                    show-word-limit
                    @input="dirty = true"
                  /></article></ElCol></ElRow
            ><ElFormItem :label="t('结束语')"
              ><ElInput
                v-model="form.closingText"
                type="textarea"
                :rows="3"
                maxlength="4000"
                show-word-limit
                @input="dirty = true" /></ElFormItem></ElForm
        ></ElCard>
        <article class="host-script-print">
          <header>
            <p>ProjectBalloon · HOST CUE SHEET</p>
            <h1>{{ t('{contest}颁奖主持人提词稿', { contest: script.contestName }) }}</h1>
          </header>
          <section>
            <h2>{{ t('开场') }}</h2>
            <p>{{ form.openingText }}</p>
          </section>
          <section v-for="(section, index) in form.sections" :key="section.categoryId">
            <h2>{{ index + 1 }}. {{ section.name }}（{{ section.code }}）</h2>
            <p>{{ section.cueText }}</p>
            <ol>
              <li v-for="recipient in section.recipients" :key="recipient.id">
                <strong>{{ recipient.teamName }}</strong
                ><span v-if="recipient.school"> · {{ recipient.school }}</span>
              </li>
            </ol>
          </section>
          <section>
            <h2>{{ t('结束') }}</h2>
            <p>{{ form.closingText }}</p>
          </section>
        </article>
      </template>
      <ElEmpty v-else-if="!loading && !errorMessage" :description="t('请选择比赛')" />
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { Check, Printer, Refresh } from '@element-plus/icons-vue';
import { ElMessage } from 'element-plus';
import { contestApi } from '../api/contest';
import { awardsApi, type AwardHostScript } from '../api/awards';
import { getErrorMessage } from '../api/client';
import type { ContestResponse } from '../api/types';
import {
  subscribeContestEvents,
  type ContestRealtimeSubscription,
} from '../realtime/contest-events';
import { useI18n } from '../i18n';
const { t } = useI18n();
const contests = ref<ContestResponse[]>([]);
const contestId = ref<number | null>(null);
const script = ref<AwardHostScript | null>(null);
const loading = ref(false);
const saving = ref(false);
const dirty = ref(false);
const errorMessage = ref('');
const now = ref(Date.now());
let generation = 0;
let realtime: ContestRealtimeSubscription | undefined;
let clockTimer: number | undefined;
const form = reactive({
  openingText: '',
  closingText: '',
  sections: [] as AwardHostScript['sections'],
});
const effectiveCategoryId = computed(() => {
  const value = script.value;
  if (!value?.sections.length || !value.autoRotate || value.presentationStatus !== 'PRESENTING')
    return value?.currentCategoryId ?? null;
  const base = Math.max(
    0,
    value.sections.findIndex((item) => item.categoryId === value.currentCategoryId),
  );
  const elapsed = Math.max(0, now.value - new Date(value.stateUpdatedAt).getTime());
  return (
    value.sections[
      (base + Math.floor(elapsed / (value.intervalSeconds * 1000))) % value.sections.length
    ]?.categoryId ?? value.currentCategoryId
  );
});
const currentSection = computed(
  () =>
    script.value?.sections.find((item) => item.categoryId === effectiveCategoryId.value) ?? null,
);
const nextSection = computed(() => {
  const index =
    script.value?.sections.findIndex((item) => item.categoryId === effectiveCategoryId.value) ?? -1;
  return index >= 0 ? (script.value?.sections[index + 1] ?? null) : null;
});
const statusLabel = computed(() =>
  t(
    { WAITING: '等待开始', PRESENTING: '颁奖进行中', COMPLETED: '典礼结束' }[
      script.value?.presentationStatus ?? 'WAITING'
    ],
  ),
);
function apply(value: AwardHostScript) {
  script.value = value;
  now.value = new Date(value.serverTime).getTime();
  form.openingText = value.openingText;
  form.closingText = value.closingText;
  form.sections = value.sections.map((item) => ({ ...item }));
  dirty.value = false;
}
async function load(reset: boolean) {
  if (!contestId.value) return;
  const id = contestId.value;
  const token = ++generation;
  loading.value = true;
  try {
    const value = await awardsApi.hostScript(id);
    if (token === generation && id === contestId.value) {
      script.value = value;
      if (reset || !dirty.value) apply(value);
      errorMessage.value = '';
    }
  } catch (error) {
    if (token === generation) {
      script.value = null;
      errorMessage.value = getErrorMessage(error);
    }
  } finally {
    if (token === generation) loading.value = false;
  }
}
function connect() {
  realtime?.stop();
  if (!contestId.value) return;
  realtime = subscribeContestEvents({
    contestId: contestId.value,
    scope: 'PUBLIC',
    eventTypes: ['AWARDS_UPDATED'],
    onEvent: () => void load(false),
    poll: () => load(false),
  });
}
async function changeContest() {
  generation += 1;
  script.value = null;
  dirty.value = false;
  connect();
  await load(true);
}
async function save() {
  if (!contestId.value || !script.value) return;
  saving.value = true;
  try {
    const value = await awardsApi.saveHostScript(contestId.value, {
      openingText: form.openingText,
      closingText: form.closingText,
      sections: form.sections.map(({ categoryId, cueText }) => ({ categoryId, cueText })),
      expectedVersion: script.value.version,
    });
    apply(value);
    ElMessage.success(t('主持人脚本已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
    await load(false);
  } finally {
    saving.value = false;
  }
}
function printScript() {
  window.print();
}
onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    contestId.value = contests.value[0]?.id ?? null;
    connect();
    await load(true);
    clockTimer = window.setInterval(() => {
      now.value += 1000;
    }, 1000);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
onBeforeUnmount(() => {
  realtime?.stop();
  if (clockTimer) window.clearInterval(clockTimer);
});
</script>

<style scoped>
.host-script-page {
  width: min(1500px, 100%);
  margin: 0 auto;
}

.host-script-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
  padding: 38px 38px 0;
  margin-bottom: 24px;
  height: auto;
}

.host-script-page-header h1 {
  margin: 4px 0 8px;
  color: #172033;
  font-size: clamp(30px, 4vw, 44px);
  letter-spacing: -0.035em;
}

.page-body {
  padding: 0 38px 38px;
}

.host-live-card,
.host-script-editor {
  margin-top: 18px;
}

.host-live-card .card-header,
.host-script-editor .card-header,
.host-section-editor-list article > header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.host-live-grid > .el-col {
  align-self: flex-start;
}

.host-current-cue {
  padding: 28px;

  color: #fff8e8;
  background: #18233d;
}

.host-current-cue small {
  color: #f5c451;
  font-weight: 800;
  letter-spacing: 0.12em;
}

.host-current-cue h2 {
  margin: 8px 0 18px;
  font-size: 34px;
}

.host-current-cue p {
  min-height: 90px;
  margin: 0 0 20px;
  font-size: 21px;
  line-height: 1.7;
  white-space: pre-wrap;
}

.host-current-cue > span {
  color: #d7deea;
}

.host-current-recipients {
  max-height: 360px;
  overflow: auto;
  padding: 20px 24px;
  border: 1px solid #e5e7eb;

  background: #fafbfc;
}

.host-current-recipients ol {
  display: grid;
  gap: 10px;
  margin: 14px 0 0;
  padding-left: 24px;
}

.host-current-recipients li span,
.host-current-recipients li small {
  display: block;
}

.host-current-recipients li span {
  font-weight: 800;
}

.host-current-recipients li small {
  margin-top: 2px;
  color: var(--muted);
}

.host-section-editor-list {
  margin-bottom: 20px;
}

.host-section-editor-list article {
  padding: 18px;
  border: 1px solid #e5e7eb;

  background: #fafbfc;
}

.host-section-editor-list article > header {
  margin-bottom: 12px;
}

.host-section-editor-list article > header small,
.host-section-editor-list article > header strong {
  display: block;
}

.host-section-editor-list article > header small {
  color: #b7791f;
  font-weight: 800;
}

.host-script-print {
  display: none;
}

@media (max-width: 1000px) {
  .host-script-page-header {
    align-items: stretch;
    flex-direction: column;
    padding: 24px 16px 0;
  }

  .page-body {
    padding: 0 16px 24px;
  }
}

@media print {
  .host-script-page-header,
  .host-live-card,
  .host-script-editor {
    display: none !important;
  }

  .host-script-page {
    max-width: none;
    margin: 0;
    padding: 0;
    background: #fff;
  }

  .page-body {
    padding: 0;
  }

  .host-script-print {
    display: block;
    color: #111827;
    font-family: 'Noto Serif SC', 'Songti SC', serif;
  }

  .host-script-print > header {
    margin-bottom: 28px;
    padding-bottom: 16px;
    border-bottom: 2px solid #111827;
  }

  .host-script-print > header p {
    margin: 0 0 6px;
    font-size: 11px;
    letter-spacing: 0.16em;
  }

  .host-script-print > header h1 {
    margin: 0;
    font-size: 26px;
  }

  .host-script-print section {
    break-inside: avoid;
    margin-bottom: 24px;
  }

  .host-script-print section h2 {
    margin: 0 0 8px;
    font-size: 18px;
  }

  .host-script-print section p {
    margin: 0 0 10px;
    line-height: 1.8;
    white-space: pre-wrap;
  }

  .host-script-print section ol {
    margin: 0;
    padding-left: 24px;
    line-height: 1.7;
  }
}
</style>
