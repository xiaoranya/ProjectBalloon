<template>
  <el-container direction="vertical" class="live-manage">
    <el-header height="auto" class="page-head"
      ><div>
        <p class="eyebrow">Live / OBS</p>
        <h1>{{ t('直播展示控制台') }}</h1>
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
      <ElCard v-if="contestId" shadow="never">
        <template #header
          ><div class="card-header">
            <strong>{{ t('展示配置') }}</strong
            ><ElButton type="primary" :loading="saving" @click="save">{{ t('保存') }}</ElButton>
          </div></template
        >
        <ElForm label-position="top"
          ><ElRow :gutter="18" class="grid"
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('发布状态')"
                ><ElSwitch v-model="form.enabled" :active-text="t('已发布')" /></ElFormItem></ElCol
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
                    value="SPLIT" /><ElOption
                    v-for="item in templates"
                    :key="item.id"
                    :label="t('自定义：{name}', { name: item.name })"
                    value="CUSTOM" /></ElSelect></ElFormItem></ElCol
            ><ElCol v-if="form.template === 'CUSTOM'" :xs="24" :md="8"
              ><ElFormItem :label="t('自定义模板')"
                ><ElSelect v-model="form.customTemplateId"
                  ><ElOption
                    v-for="item in templates"
                    :key="item.id"
                    :label="item.name"
                    :value="item.id" /></ElSelect></ElFormItem></ElCol></ElRow
          ><ElCheckbox v-model="form.showAnnouncements">{{ t('展示公告') }}</ElCheckbox></ElForm
        >
      </ElCard>
      <ElCard v-if="contestId" shadow="never">
        <template #header
          ><div class="card-header">
            <strong>{{ t('自定义模板') }}</strong
            ><ElButton type="primary" :loading="templateSaving" @click="saveTemplate">{{
              t('保存模板')
            }}</ElButton>
          </div></template
        >
        <ElForm label-position="top"
          ><ElRow :gutter="18" class="grid"
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('名称')"
                ><ElInput v-model="templateForm.name" maxlength="120" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('字体')"
                ><ElInput v-model="templateForm.fontFamily" maxlength="120" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('背景色')"
                ><ElColorPicker v-model="templateForm.backgroundColor" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('前景色')"
                ><ElColorPicker v-model="templateForm.foregroundColor" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('强调色')"
                ><ElColorPicker v-model="templateForm.accentColor" /></ElFormItem></ElCol
            ><ElCol :xs="24" :md="8"
              ><ElFormItem :label="t('密度')"
                ><ElSelect v-model="templateForm.density"
                  ><ElOption :label="t('紧凑')" value="COMPACT" /><ElOption
                    :label="t('舒适')"
                    value="COMFORTABLE" /><ElOption
                    :label="t('宽松')"
                    value="SPACIOUS" /></ElSelect></ElFormItem></ElCol></ElRow
          ><ElCheckbox v-model="templateForm.showClock">{{ t('显示时钟') }}</ElCheckbox
          ><ElCheckbox v-model="templateForm.showLogo">{{ t('显示品牌标识') }}</ElCheckbox></ElForm
        >
      </ElCard>
      <ElCard v-if="contestId" shadow="never">
        <template #header
          ><div class="card-header">
            <div>
              <strong>{{ t('广播 Token') }}</strong
              ><small>{{ t('原始 Token 只显示一次，数据库仅保存摘要') }}</small>
            </div>
            <ElButton :loading="loading" @click="loadTokens">{{ t('刷新') }}</ElButton>
          </div></template
        >
        <ElForm inline
          ><ElFormItem :label="t('用途')"
            ><ElInput
              v-model="label"
              maxlength="120"
              :placeholder="t('OBS 主直播流')" /></ElFormItem
          ><ElFormItem :label="t('有效期')"
            ><ElInput v-model="expiresAt" type="datetime-local" /></ElFormItem
          ><ElButton type="primary" @click="createToken">{{ t('生成链接') }}</ElButton></ElForm
        >
        <ElAlert
          v-if="links.length"
          type="success"
          :closable="false"
          :title="t('请立即复制这些链接')"
          ><div v-for="item in links" :key="item.url" class="link">
            <span>{{ item.label }}</span
            ><ElInput :model-value="item.url" readonly /><ElButton @click="copy(item.url)">{{
              t('复制')
            }}</ElButton>
          </div></ElAlert
        >
        <ElTable :data="tokens"
          ><ElTableColumn prop="label" :label="t('用途')" /><ElTableColumn
            prop="expiresAt"
            :label="t('有效期')"
          /><ElTableColumn prop="lastUsedAt" :label="t('最近使用')" /><ElTableColumn
            :label="t('状态')"
            width="100"
            ><template #default="{ row }"
              ><ElTag :type="active(row.revokedAt, row.expiresAt) ? 'success' : 'info'">{{
                active(row.revokedAt, row.expiresAt) ? t('有效') : t('失效')
              }}</ElTag></template
            ></ElTableColumn
          ><ElTableColumn width="90"
            ><template #default="{ row }"
              ><ElButton
                link
                type="danger"
                :disabled="!active(row.revokedAt, row.expiresAt)"
                @click="revoke(row.id)"
                >{{ t('撤销') }}</ElButton
              ></template
            ></ElTableColumn
          ></ElTable
        >
      </ElCard>
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { onMounted, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { getErrorMessage } from '../api/client';
import {
  presentationApi,
  type BroadcastToken,
  type PresentationTemplate,
} from '../api/presentation';
import { useI18n } from '../i18n';
const { t } = useI18n();
const contests = ref<Contest[]>([]);
const contestId = ref<number | null>(null);
const tokens = ref<BroadcastToken[]>([]);
const templates = ref<PresentationTemplate[]>([]);
const loading = ref(false);
const saving = ref(false);
const templateSaving = ref(false);
const errorMessage = ref('');
const label = ref('');
const expiresAt = ref(defaultExpiry());
const links = ref<Array<{ label: string; url: string }>>([]);
const form = reactive({
  enabled: false,
  title: '',
  subtitle: '',
  accentColor: '#22c55e',
  rowLimit: 12,
  showAnnouncements: true,
  announcementIntervalSeconds: 10,
  template: 'DEFAULT' as 'DEFAULT' | 'CINEMATIC' | 'MINIMAL' | 'SPLIT' | 'CUSTOM',
  customTemplateId: null as number | null,
});
const templateForm = reactive({
  id: null as number | null,
  name: '赛事主题',
  description: '',
  backgroundColor: '#07111f',
  foregroundColor: '#ffffff',
  accentColor: '#22c55e',
  fontFamily: 'Inter',
  density: 'COMFORTABLE' as 'COMPACT' | 'COMFORTABLE' | 'SPACIOUS',
  showClock: true,
  showLogo: false,
  logoObjectKey: null as string | null,
});
function defaultExpiry() {
  const date = new Date(Date.now() + 24 * 3600_000);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
}
async function load() {
  if (!contestId.value) return;
  try {
    const value = await presentationApi.config(contestId.value, 'LIVE');
    Object.assign(form, {
      ...value,
      title: value.title ?? '',
      subtitle: value.subtitle ?? '',
      customTemplateId: value.customTemplateId,
    });
    await loadTokens();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
}
async function loadTokens() {
  if (!contestId.value) return;
  loading.value = true;
  try {
    tokens.value = await presentationApi.tokens(contestId.value);
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}
async function save() {
  if (!contestId.value) return;
  saving.value = true;
  try {
    await presentationApi.update(contestId.value, 'LIVE', {
      ...form,
      title: form.title.trim() || null,
      subtitle: form.subtitle.trim() || null,
    });
    ElMessage.success(t('直播配置已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    saving.value = false;
  }
}
async function createToken() {
  if (!contestId.value || !label.value.trim()) {
    ElMessage.warning(t('请填写用途'));
    return;
  }
  try {
    const value = await presentationApi.createToken(contestId.value, {
      label: label.value.trim(),
      expiresAt: new Date(expiresAt.value).toISOString(),
    });
    const fragment = new URLSearchParams({ token: value.token }).toString();
    links.value = [
      [t('主榜'), '/live'],
      [t('首杀'), '/live/first-blood'],
      [t('气球'), '/live/balloons'],
      [t('封榜倒计时'), '/live/freeze-countdown'],
      [t('赛事统计'), '/live/statistics'],
    ].map(([name, path]) => ({
      label: name,
      url: `${location.origin}${path}?contestId=${contestId.value}#${fragment}`,
    }));
    label.value = '';
    await loadTokens();
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  }
}
function active(revokedAt: unknown, expires: unknown) {
  return !revokedAt && typeof expires === 'string' && new Date(expires).getTime() > Date.now();
}
async function revoke(id: number) {
  if (!contestId.value) return;
  try {
    await ElMessageBox.confirm(
      t('撤销后所有使用该 Token 的 OBS 页面会立即失效。'),
      t('撤销 Token'),
    );
    await presentationApi.revokeToken(contestId.value, id);
    links.value = [];
    await loadTokens();
  } catch (error) {
    if (error !== 'cancel') ElMessage.error(getErrorMessage(error));
  }
}
async function copy(value: string) {
  await navigator.clipboard.writeText(value);
  ElMessage.success(t('已复制'));
}
watch(contestId, () => {
  links.value = [];
  void load();
});
async function saveTemplate() {
  templateSaving.value = true;
  try {
    const { id: _id, ...payload } = templateForm;
    const value = templateForm.id
      ? await presentationApi.updateTemplate(templateForm.id, payload)
      : await presentationApi.createTemplate(payload);
    templateForm.id = value.id;
    templates.value = [value, ...templates.value.filter((item) => item.id !== value.id)];
    form.template = 'CUSTOM';
    form.customTemplateId = value.id;
    ElMessage.success(t('自定义模板已保存'));
  } catch (error) {
    ElMessage.error(getErrorMessage(error));
  } finally {
    templateSaving.value = false;
  }
}
onMounted(async () => {
  try {
    [contests.value, templates.value] = await Promise.all([
      (await contestApi.listContests()).content,
      presentationApi.templates(),
    ]);
    const first = templates.value[0];
    if (first) Object.assign(templateForm, { ...first });
    contestId.value = contests.value[0]?.id ?? null;
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  }
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
.page-body {
  padding: 20px 28px 28px;
}
.page-body > :not(:last-child) {
  margin-bottom: 20px;
}
.card-header,
.link {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}
.card-header > div {
  display: grid;
}
.card-header small {
  color: #8492a6;
}
.link {
  align-items: center;
  margin-top: 10px;
}
.link span {
  width: 80px;
}
@media (max-width: 900px) {
  .page-head {
    flex-direction: column;
  }
}
</style>
