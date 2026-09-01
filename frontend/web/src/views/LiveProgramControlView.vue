<template>
  <el-container direction="vertical" class="live-program-control">
    <el-header height="auto" class="page-head"
      ><div>
        <h1>{{ t('导播台') }}</h1>
        <small>{{ t('为合成直播页 /live/program 切换播出场景') }}</small>
      </div>
      <ElSelect v-model="contestId" filterable
        ><ElOption
          v-for="contest in contests"
          :key="contest.id"
          :label="contest.name"
          :value="contest.id" /></ElSelect
    ></el-header>
    <el-main class="page-body">
      <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
      <ElAlert
        v-else-if="versionConflict"
        type="warning"
        :closable="false"
        show-icon
        :title="t('导播状态已被他人修改，已刷新为服务器最新状态')"
      />
      <template v-if="contestId && program">
        <ElCard shadow="never" class="on-air-card">
          <template #header
            ><div class="card-header">
              <strong>{{ t('正在播出') }}: {{ SCENE_LABELS[program.currentScene] }}</strong
              ><ElTag>{{ t('版本 {version}', { version: program.version }) }}</ElTag>
            </div></template
          >
          <div class="scene-grid">
            <ElButton
              v-for="scene in SCENES"
              :key="scene"
              size="large"
              :type="program.currentScene === scene ? 'primary' : 'default'"
              :disabled="saving"
              @click="switchScene(scene)"
              >{{ SCENE_LABELS[scene]
              }}<small class="scene-key">{{ sceneHotkey(scene) }}</small></ElButton
            >
          </div>
        </ElCard>
        <ElCard shadow="never">
          <template #header
            ><div class="card-header">
              <strong>{{ t('播出细节') }}</strong
              ><ElButton type="primary" :loading="saving" @click="save">{{
                t('保存全部设置')
              }}</ElButton>
            </div></template
          >
          <ElForm label-position="top"
            ><ElRow :gutter="18"
              ><ElCol :xs="24" :md="8"
                ><ElFormItem :label="t('转场时长 (毫秒)')"
                  ><ElInputNumber
                    v-model="form.transitionMilliseconds"
                    :min="100"
                    :max="5000"
                    :step="100" /></ElFormItem></ElCol
              ><ElCol :xs="24" :md="8"
                ><ElFormItem :label="t('标题卡文案')"
                  ><ElInput
                    v-model="form.titleCardText"
                    maxlength="240"
                    :placeholder="t('默认使用直播页标题')" /></ElFormItem></ElCol
              ><ElCol :xs="24" :md="8"
                ><ElFormItem :label="t('滚榜运行')"
                  ><ElSelect v-model="form.resolverRunId" clearable
                    ><ElOption
                      v-for="run in officialRuns"
                      :key="run.id"
                      :label="`#${run.id} · ${run.status} · ${run.currentStep}/${run.totalSteps}`"
                      :value="run.id" /></ElSelect
                  ><small class="form-hint">{{
                    t('留空时自动使用最新的正式滚榜运行')
                  }}</small></ElFormItem
                ></ElCol
              ></ElRow
            ><ElCheckbox v-model="form.showClock">{{ t('显示时钟') }}</ElCheckbox
            ><ElCheckbox v-model="form.tickerEnabled">{{ t('显示底部公告条') }}</ElCheckbox></ElForm
          >
        </ElCard>
        <ElCard shadow="never">
          <template #header
            ><div class="card-header">
              <strong>{{ t('OBS 源链接') }}</strong
              ><ElButton link @click="goTokenManager">{{ t('在直播控制台生成 Token') }}</ElButton>
            </div></template
          >
          <ElForm inline
            ><ElFormItem :label="t('广播 Token')"
              ><ElInput
                v-model="knownToken"
                :placeholder="t('粘贴已有 Token 以生成页面链接')" /></ElFormItem
            ><ElButton :disabled="!knownToken.trim()" @click="buildLinks">{{
              t('生成链接')
            }}</ElButton></ElForm
          >
          <div v-if="links.length" class="links">
            <div v-for="item in links" :key="item.url" class="link">
              <span>{{ item.label }}</span
              ><ElInput :model-value="item.url" readonly /><ElButton @click="copy(item.url)">{{
                t('复制')
              }}</ElButton>
            </div>
          </div>
        </ElCard>
      </template>
    </el-main>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage } from 'element-plus';
import { contestApi } from '../api/contest';
import type { ContestResponse } from '../api/types';
import { ApiError, getErrorMessage } from '../api/client';
import { presentationApi, type LiveProgramState, type LiveScene } from '../api/presentation';
import { useI18n } from '../i18n';

const { t } = useI18n();
const router = useRouter();

const SCENES: LiveScene[] = [
  'SCOREBOARD',
  'FIRST_BLOOD',
  'BALLOONS',
  'FREEZE_COUNTDOWN',
  'STATISTICS',
  'RESOLVER',
  'AWARDS',
  'TITLE_CARD',
];

const contests = ref<ContestResponse[]>([]);
const contestId = ref<number | null>(null);
const program = ref<LiveProgramState | null>(null);
const resolverRuns = ref<Awaited<ReturnType<typeof presentationApi.program>>['resolverRuns']>([]);
const saving = ref(false);
const errorMessage = ref('');
const versionConflict = ref(false);
const knownToken = ref('');
const links = ref<Array<{ label: string; url: string }>>([]);
const form = reactive({
  currentScene: 'SCOREBOARD' as LiveScene,
  resolverRunId: null as number | null,
  transitionMilliseconds: 800,
  showClock: true,
  tickerEnabled: true,
  titleCardText: '' as string,
});

const officialRuns = computed(() => resolverRuns.value.filter((run) => run.official));

const SCENE_LABELS: Record<LiveScene, string> = {
  SCOREBOARD: t('实时榜单'),
  FIRST_BLOOD: 'FIRST BLOOD',
  BALLOONS: t('气球状态'),
  FREEZE_COUNTDOWN: t('封榜倒计时'),
  STATISTICS: t('比赛统计'),
  RESOLVER: t('滚榜仪式'),
  AWARDS: t('颁奖典礼'),
  TITLE_CARD: t('标题卡'),
};

function sceneHotkey(scene: LiveScene) {
  return `${SCENES.indexOf(scene) + 1}`;
}

async function load() {
  if (!contestId.value) return;
  errorMessage.value = '';
  try {
    const value = await presentationApi.program(contestId.value);
    applyProgram(value.program, value.resolverRuns);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}

function applyProgram(
  value: LiveProgramState,
  runs?: Awaited<ReturnType<typeof presentationApi.program>>['resolverRuns'],
) {
  program.value = value;
  if (runs) resolverRuns.value = runs;
  form.currentScene = value.currentScene;
  form.resolverRunId = value.resolverRunId;
  form.transitionMilliseconds = value.transitionMilliseconds;
  form.showClock = value.showClock;
  form.tickerEnabled = value.tickerEnabled;
  form.titleCardText = value.titleCardText ?? '';
}

async function save() {
  if (!contestId.value || !program.value) return;
  saving.value = true;
  versionConflict.value = false;
  try {
    const next = await presentationApi.updateProgram(contestId.value, {
      currentScene: form.currentScene,
      resolverRunId: form.resolverRunId,
      transitionMilliseconds: form.transitionMilliseconds,
      showClock: form.showClock,
      tickerEnabled: form.tickerEnabled,
      titleCardText: form.titleCardText.trim() || null,
      expectedVersion: program.value.version,
    });
    program.value = next;
    ElMessage.success(t('导播状态已保存'));
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      versionConflict.value = true;
      await load();
    } else {
      ElMessage.error(getErrorMessage(error));
    }
  } finally {
    saving.value = false;
  }
}

async function switchScene(scene: LiveScene) {
  form.currentScene = scene;
  await save();
}

async function toggleFlag(field: 'showClock' | 'tickerEnabled') {
  form[field] = !form[field];
  await save();
}

function onKeydown(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
  if (!contestId.value || saving.value) return;
  const index = Number(event.key);
  if (Number.isInteger(index) && index >= 1 && index <= SCENES.length) {
    event.preventDefault();
    void switchScene(SCENES[index - 1]);
    return;
  }
  const key = event.key.toLowerCase();
  if (key === 't') {
    event.preventDefault();
    void toggleFlag('tickerEnabled');
  } else if (key === 'c') {
    event.preventDefault();
    void toggleFlag('showClock');
  }
}

function buildLinks() {
  if (!contestId.value) return;
  const token = knownToken.value.trim();
  if (!token) return;
  const fragment = new URLSearchParams({ token }).toString();
  links.value = [
    {
      label: t('合成主画面'),
      url: `${location.origin}/live/program?contestId=${contestId.value}#${fragment}`,
    },
    {
      label: t('透明叠加层'),
      url: `${location.origin}/live/overlay?contestId=${contestId.value}#${fragment}`,
    },
  ];
}

function goTokenManager() {
  void router.push({ path: '/live/manage' });
}

async function copy(value: string) {
  await navigator.clipboard.writeText(value);
  ElMessage.success(t('已复制'));
}

watch(contestId, () => {
  program.value = null;
  links.value = [];
  void load();
});

onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    contestId.value = contests.value[0]?.id ?? null;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
  window.addEventListener('keydown', onKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown);
});
</script>

<style scoped>
.page-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding: 28px 28px 0;
  height: auto;
}
.page-head > div {
  display: grid;
}
.page-body {
  padding: 20px 28px 28px;
}
.page-body > :not(:last-child) {
  margin-bottom: 20px;
}
.card-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}
.scene-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 12px;
}
.scene-grid small {
  margin-left: 8px;
  color: var(--muted, #667085);
}
.form-hint {
  display: block;
  color: var(--muted, #667085);
  margin-top: 4px;
}
.links {
  display: grid;
  gap: 10px;
}
.link {
  display: flex;
  align-items: center;
  gap: 12px;
}
.link span {
  width: 96px;
}
@media (max-width: 900px) {
  .page-head {
    flex-direction: column;
  }
  .scene-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
</style>
