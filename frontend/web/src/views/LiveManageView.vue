<template>
  <main class="live-manage">
    <header><div><p class="eyebrow">Live / OBS</p><h1>直播展示控制台</h1><p>发布直播数据并生成可撤销的 OBS 浏览器源链接。</p></div><ElSelect v-model="contestId" filterable><ElOption v-for="contest in contests" :key="contest.id" :label="contest.name" :value="contest.id" /></ElSelect></header>
    <ElAlert v-if="errorMessage" type="error" :closable="false" show-icon :title="errorMessage" />
    <ElCard v-if="contestId" shadow="never">
      <template #header><div class="card-header"><strong>展示配置</strong><ElButton type="primary" :loading="saving" @click="save">保存</ElButton></div></template>
      <ElForm label-position="top"><div class="grid"><ElFormItem label="发布状态"><ElSwitch v-model="form.enabled" active-text="已发布" /></ElFormItem><ElFormItem label="主标题"><ElInput v-model="form.title" maxlength="160" /></ElFormItem><ElFormItem label="副标题"><ElInput v-model="form.subtitle" maxlength="240" /></ElFormItem><ElFormItem label="强调色"><ElInput v-model="form.accentColor" maxlength="7" /></ElFormItem><ElFormItem label="榜单行数"><ElInputNumber v-model="form.rowLimit" :min="5" :max="30" /></ElFormItem><ElFormItem label="公告间隔"><ElInputNumber v-model="form.announcementIntervalSeconds" :min="5" :max="60" /></ElFormItem></div><ElCheckbox v-model="form.showAnnouncements">展示公告</ElCheckbox></ElForm>
    </ElCard>
    <ElCard v-if="contestId" shadow="never">
      <template #header><div class="card-header"><div><strong>广播 Token</strong><small>原始 Token 只显示一次，数据库仅保存摘要</small></div><ElButton :loading="loading" @click="loadTokens">刷新</ElButton></div></template>
      <ElForm inline><ElFormItem label="用途"><ElInput v-model="label" maxlength="120" placeholder="OBS 主直播流" /></ElFormItem><ElFormItem label="有效期"><ElInput v-model="expiresAt" type="datetime-local" /></ElFormItem><ElButton type="primary" @click="createToken">生成链接</ElButton></ElForm>
      <ElAlert v-if="links.length" type="success" :closable="false" title="请立即复制这些链接"><div v-for="item in links" :key="item.url" class="link"><span>{{ item.label }}</span><ElInput :model-value="item.url" readonly /><ElButton @click="copy(item.url)">复制</ElButton></div></ElAlert>
      <ElTable :data="tokens"><ElTableColumn prop="label" label="用途" /><ElTableColumn prop="expiresAt" label="有效期" /><ElTableColumn prop="lastUsedAt" label="最近使用" /><ElTableColumn label="状态" width="100"><template #default="{ row }"><ElTag :type="active(row.revokedAt, row.expiresAt) ? 'success' : 'info'">{{ active(row.revokedAt, row.expiresAt) ? '有效' : '失效' }}</ElTag></template></ElTableColumn><ElTableColumn width="90"><template #default="{ row }"><ElButton link type="danger" :disabled="!active(row.revokedAt, row.expiresAt)" @click="revoke(row.id)">撤销</ElButton></template></ElTableColumn></ElTable>
    </ElCard>
  </main>
</template>
<script setup lang="ts">
import { onMounted, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { contestApi } from '../api/contest';
import type { Contest } from '../api/types';
import { getErrorMessage } from '../api/client';
import { presentationApi, type BroadcastToken } from '../api/presentation';
const contests = ref<Contest[]>([]); const contestId = ref<number | null>(null); const tokens = ref<BroadcastToken[]>([]); const loading = ref(false); const saving = ref(false); const errorMessage = ref(''); const label = ref(''); const expiresAt = ref(defaultExpiry()); const links = ref<Array<{ label: string; url: string }>>([]);
const form = reactive({ enabled: false, title: '', subtitle: '', accentColor: '#22c55e', rowLimit: 12, showAnnouncements: true, announcementIntervalSeconds: 10 });
function defaultExpiry() { const date = new Date(Date.now() + 24 * 3600_000); return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16); }
async function load() { if (!contestId.value) return; try { const value = await presentationApi.config(contestId.value, 'LIVE'); Object.assign(form, { ...value, title: value.title ?? '', subtitle: value.subtitle ?? '' }); await loadTokens(); } catch (error) { errorMessage.value = getErrorMessage(error); } }
async function loadTokens() { if (!contestId.value) return; loading.value = true; try { tokens.value = await presentationApi.tokens(contestId.value); } catch (error) { errorMessage.value = getErrorMessage(error); } finally { loading.value = false; } }
async function save() { if (!contestId.value) return; saving.value = true; try { await presentationApi.update(contestId.value, 'LIVE', { ...form, title: form.title.trim() || null, subtitle: form.subtitle.trim() || null }); ElMessage.success('直播配置已保存'); } catch (error) { ElMessage.error(getErrorMessage(error)); } finally { saving.value = false; } }
async function createToken() { if (!contestId.value || !label.value.trim()) { ElMessage.warning('请填写用途'); return; } try { const value = await presentationApi.createToken(contestId.value, { label: label.value.trim(), expiresAt: new Date(expiresAt.value).toISOString() }); const query = new URLSearchParams({ contestId: String(contestId.value), token: value.token }).toString(); links.value = [['主榜', '/live'], ['首杀', '/live/first-blood'], ['气球', '/live/balloons'], ['封榜倒计时', '/live/freeze-countdown'], ['赛事统计', '/live/statistics']].map(([name, path]) => ({ label: name, url: `${location.origin}${path}?${query}` })); label.value = ''; await loadTokens(); } catch (error) { ElMessage.error(getErrorMessage(error)); } }
function active(revokedAt: unknown, expires: unknown) { return !revokedAt && typeof expires === 'string' && new Date(expires).getTime() > Date.now(); }
async function revoke(id: number) { if (!contestId.value) return; try { await ElMessageBox.confirm('撤销后所有使用该 Token 的 OBS 页面会立即失效。', '撤销 Token'); await presentationApi.revokeToken(contestId.value, id); links.value = []; await loadTokens(); } catch (error) { if (error !== 'cancel') ElMessage.error(getErrorMessage(error)); } }
async function copy(value: string) { await navigator.clipboard.writeText(value); ElMessage.success('已复制'); }
watch(contestId, () => { links.value = []; void load(); });
onMounted(async () => { try { contests.value = (await contestApi.listContests()).content; contestId.value = contests.value[0]?.id ?? null; } catch (error) { errorMessage.value = getErrorMessage(error); } });
</script>
<style scoped>.live-manage{display:grid;gap:20px;padding:28px}.live-manage>header,.card-header,.link{display:flex;justify-content:space-between;gap:16px}.card-header>div{display:grid}.card-header small{color:#8492a6}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:0 18px}.link{align-items:center;margin-top:10px}.link span{width:80px}@media(max-width:900px){.grid{grid-template-columns:1fr}.live-manage>header{flex-direction:column}}</style>
