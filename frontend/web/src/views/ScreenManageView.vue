<template>
  <el-container direction="vertical" class="screen-manage-page">
    <el-header height="auto" class="page-head"
      ><div>
        <p class="eyebrow">Screen Operations</p>
        <h1>{{ t('现场大屏控制') }}</h1>
      </div>
      <ElSelect v-model="contestId" filterable :placeholder="t('选择比赛')" @change="changeContest"
        ><ElOption
          v-for="contest in contests"
          :key="contest.id"
          :label="contest.name"
          :value="contest.id" /></ElSelect
    ></el-header>
    <el-main class="page-body">
      <ElAlert v-if="errorMessage" type="error" show-icon :closable="false" :title="errorMessage" />
      <ElCard v-if="contestId" shadow="never"
        ><template #header
          ><div class="card-header">
            <div>
              <strong>{{ t('发布配置') }}</strong
              ><small>{{ t('启用后大屏客户端才能注册') }}</small>
            </div>
            <ElButton type="primary" :loading="saving" @click="saveConfig">{{
              t('保存配置')
            }}</ElButton>
          </div></template
        ><ElForm label-position="top"
          ><ElRow :gutter="18" class="screen-config-grid"
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('发布状态')"
                ><ElSwitch
                  v-model="form.enabled"
                  :active-text="t('已发布')"
                  :inactive-text="t('未发布')" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('主标题')"
                ><ElInput v-model="form.title" maxlength="160" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('副标题')"
                ><ElInput v-model="form.subtitle" maxlength="240" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('强调色')"
                ><ElInput v-model="form.accentColor" maxlength="7" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('榜单行数')"
                ><ElInputNumber v-model="form.rowLimit" :min="5" :max="30" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('公告间隔')"
                ><ElInputNumber
                  v-model="form.announcementIntervalSeconds"
                  :min="5"
                  :max="60" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('视觉模板')"
                ><ElSelect v-model="form.template"
                  ><ElOption :label="t('默认')" value="DEFAULT" /><ElOption
                    :label="t('电影感')"
                    value="CINEMATIC" /><ElOption :label="t('极简')" value="MINIMAL" /><ElOption
                    :label="t('分栏')"
                    value="SPLIT" /></ElSelect></ElFormItem></ElCol></ElRow
          ><ElCheckbox v-model="form.showAnnouncements">{{ t('显示公告') }}</ElCheckbox></ElForm
        ></ElCard
      >
      <ElCard v-if="contestId" shadow="never" class="presentation-control-card"
        ><template #header
          ><div class="card-header">
            <div>
              <strong>{{ t('大屏实例') }}</strong
              ><small>{{ t('45 秒内有心跳视为在线') }}</small>
            </div>
            <div>
              <ElButton :loading="loading" @click="loadInstances">{{ t('刷新') }}</ElButton
              ><ElButton :disabled="!form.enabled" @click="openClient">{{
                t('打开新大屏')
              }}</ElButton>
            </div>
          </div></template
        ><ElTable :data="instances" row-key="id"
          ><ElTableColumn :label="t('实例')" min-width="180"
            ><template #default="{ row }"
              ><strong>{{ row.name }}</strong
              ><small class="instance-meta"
                >#{{ row.id }} · {{ row.lastIp || t('无地址') }}</small
              ></template
            ></ElTableColumn
          ><ElTableColumn :label="t('状态')" width="100"
            ><template #default="{ row }"
              ><ElTag :type="row.revokedAt ? 'info' : row.online ? 'success' : 'warning'">{{
                row.revokedAt ? t('已撤销') : row.online ? t('在线') : t('离线')
              }}</ElTag></template
            ></ElTableColumn
          ><ElTableColumn prop="currentView" :label="t('当前画面')" width="150" /><ElTableColumn
            :label="t('远程切换')"
            min-width="420"
            ><template #default="{ row }"
              ><ElSelect
                v-model="targets[row.id]"
                :disabled="Boolean(row.revokedAt)"
                style="width: 180px"
                ><ElOption
                  v-for="target in screenTargets"
                  :key="target"
                  :label="viewLabel(target)"
                  :value="target" /></ElSelect
              ><ElButton type="primary" :disabled="Boolean(row.revokedAt)" @click="send(row.id)">{{
                t('发送')
              }}</ElButton
              ><ElButton
                link
                type="danger"
                :disabled="Boolean(row.revokedAt)"
                @click="revoke(row.id)"
                >{{ t('撤销') }}</ElButton
              ></template
            ></ElTableColumn
          ><template #empty><ElEmpty :description="t('尚无大屏实例')" /></template></ElTable
      ></ElCard>
      <ScreenOrchestrationControl
        v-if="contestId"
        :key="contestId"
        :contest-id="contestId"
        :instances="instances"
      />
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { useRoute, useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { getErrorMessage } from '../api/client';
import { presentationApi } from '../api/presentation';
import { screenApi, type ScreenInstance, type ScreenViewTarget } from '../api/screen';
import ScreenOrchestrationControl from '../components/ScreenOrchestrationControl.vue';
import { useI18n } from '../i18n';
const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const contests = ref<Contest[]>([]);
const contestId = ref<number | null>(null);
const instances = ref<ScreenInstance[]>([]);
const loading = ref(false);
const saving = ref(false);
const errorMessage = ref('');
const targets = reactive<Record<number, ScreenViewTarget>>({});
let timer: number | undefined;
let generation = 0;
const screenTargets: ScreenViewTarget[] = [
  'SCOREBOARD',
  'FIRST_BLOOD',
  'BALLOONS',
  'FREEZE_COUNTDOWN',
  'STATISTICS',
  'RESOLVER',
  'AWARDS',
];
const form = reactive({
  enabled: false,
  title: '',
  subtitle: '',
  accentColor: '#22c55e',
  rowLimit: 12,
  showAnnouncements: true,
  announcementIntervalSeconds: 10,
  template: 'DEFAULT' as 'DEFAULT' | 'CINEMATIC' | 'MINIMAL' | 'SPLIT',
});
function viewLabel(value: ScreenViewTarget) {
  return t(
    {
      SCOREBOARD: '榜单',
      FIRST_BLOOD: '首杀',
      BALLOONS: '气球',
      FREEZE_COUNTDOWN: '封榜倒计时',
      STATISTICS: '统计',
      RESOLVER: 'Resolver',
      AWARDS: '颁奖',
    }[value],
  );
}
async function loadConfig() {
  if (!contestId.value) return;
  const id = contestId.value;
  const token = ++generation;
  try {
    const value = await presentationApi.config(id, 'SCREEN');
    if (token !== generation || id !== contestId.value) return;
    Object.assign(form, { ...value, title: value.title ?? '', subtitle: value.subtitle ?? '' });
    errorMessage.value = '';
  } catch (error) {
    if (token === generation) errorMessage.value = getErrorMessage(error);
  }
}
async function loadInstances() {
  if (!contestId.value) return;
  const id = contestId.value;
  loading.value = true;
  try {
    const value = await screenApi.list(id);
    if (id !== contestId.value) return;
    instances.value = value;
    value.forEach((item) => {
      targets[item.id] ??= item.currentView;
    });
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}
async function changeContest() {
  generation += 1;
  instances.value = [];
  await router.replace({ query: contestId.value ? { contestId: String(contestId.value) } : {} });
  await Promise.all([loadConfig(), loadInstances()]);
}
async function saveConfig() {
  if (!contestId.value) return;
  saving.value = true;
  try {
    await presentationApi.update(contestId.value, 'SCREEN', {
      enabled: form.enabled,
      title: form.title.trim() || null,
      subtitle: form.subtitle.trim() || null,
      accentColor: form.accentColor,
      rowLimit: form.rowLimit,
      showAnnouncements: form.showAnnouncements,
      announcementIntervalSeconds: form.announcementIntervalSeconds,
      template: form.template,
    });
    ElMessage.success(t('大屏配置已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}
async function send(instanceId: number) {
  if (!contestId.value) return;
  try {
    await screenApi.command(contestId.value, instanceId, targets[instanceId] ?? 'SCOREBOARD');
    ElMessage.success(t('切换命令已发送'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
async function revoke(instanceId: number) {
  if (!contestId.value) return;
  try {
    await ElMessageBox.confirm(t('撤销后该客户端令牌立即失效，是否继续？'), t('撤销大屏'), {
      type: 'warning',
    });
    await screenApi.revoke(contestId.value, instanceId);
    await loadInstances();
  } catch (error) {
    if (error !== 'cancel') ElMessage.error(getErrorMessage(error));
  }
}
function openClient() {
  if (contestId.value) window.open(`/screen?contestId=${contestId.value}`, '_blank', 'noopener');
}
onMounted(async () => {
  try {
    contests.value = (await contestApi.listContests()).content;
    const requested = Number(route.query.contestId);
    contestId.value = contests.value.some((item) => item.id === requested)
      ? requested
      : (contests.value[0]?.id ?? null);
    await Promise.all([loadConfig(), loadInstances()]);
    timer = window.setInterval(() => {
      if (!document.hidden) void loadInstances();
    }, 15000);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
});
onBeforeUnmount(() => {
  if (timer) window.clearInterval(timer);
});
</script>
<style scoped>
.page-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 24px;
  padding: 28px 28px 0;
  height: auto;
}
.page-body {
  padding: 20px 28px 28px;
}
.page-body > :not(:last-child) {
  margin-bottom: 20px;
}
.page-head h1 {
  margin: 4px 0;
}
.instance-meta {
  display: block;
  color: #8492a6;
  margin-top: 4px;
}
@media (max-width: 900px) {
  .page-head {
    flex-direction: column;
  }
}
</style>
