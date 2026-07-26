<template>
  <section class="resolver-page">
    <header class="resolver-page-heading">
      <div><p class="eyebrow">Official Resolver</p><h1>Resolver 运行控制台</h1><p>以不可变封榜快照和最终榜快照生成揭晓计划，并通过版本锁安全控制。</p></div>
      <div class="resolver-heading-actions">
        <ElSelect v-model="contestId" filterable placeholder="选择比赛" @change="changeContest"><ElOption v-for="contest in contests" :key="contest.id" :label="`${contest.name} · #${contest.id}`" :value="contest.id" /></ElSelect>
        <ElButton :icon="Refresh" :loading="loading" @click="loadContext">刷新</ElButton>
        <RouterLink v-if="run?.official && run.status !== 'READY'" :to="{ name: 'resolver-display', params: { runId: run.id } }" target="_blank"><ElButton :icon="Monitor">打开展示端</ElButton></RouterLink>
      </div>
    </header>
    <ElAlert v-if="errorMessage" :title="errorMessage" type="error" show-icon :closable="false" class="page-alert" />

    <template v-if="contestId">
      <div class="resolver-metrics">
        <div><span>运行状态</span><strong :class="`status-${run?.status.toLowerCase() ?? 'none'}`">{{ run ? statusLabel(run.status) : '未选择' }}</strong></div>
        <div><span>揭晓进度</span><strong>{{ run ? `${run.currentStep} / ${run.totalSteps}` : '—' }}</strong></div>
        <div><span>运行类型</span><strong>{{ run ? (run.official ? '正式' : '预演') : '—' }}</strong></div>
        <div><span>同步方式</span><strong>{{ realtimeConnected ? 'SSE 实时' : '轮询校准' }}</strong></div>
      </div>

      <ElCard shadow="never" class="resolver-command-card">
        <div class="resolver-command-bar">
          <div><strong>运行与快照</strong><small v-if="sources">PUBLIC v{{ sources.publicSnapshot.version }} · ADMIN v{{ sources.finalSnapshot.version }}</small><small v-else>尚未找到完整快照来源</small></div>
          <div class="resolver-command-actions">
            <ElSelect v-if="runs.length" v-model="runId" placeholder="选择已有运行" @change="selectRun"><ElOption v-for="item in runs" :key="item.id" :value="item.id" :label="`#${item.id} · ${item.official ? '正式' : '预演'} · ${statusLabel(item.status)}`" /></ElSelect>
            <ElButton :disabled="!sources" :loading="acting" @click="createRun(false)">创建预演</ElButton>
            <ElButton type="danger" plain :disabled="!sources || hasOfficial" :loading="acting" @click="createRun(true)">创建正式运行</ElButton>
          </div>
        </div>
      </ElCard>

      <ElCard v-if="run" shadow="never" class="resolver-command-card">
        <div class="resolver-command-bar">
          <div><strong>单步控制</strong><small>每次命令携带当前 version；完成前必须揭晓全部步骤。</small></div>
          <div class="resolver-command-actions">
            <ElButton v-if="run.status === 'READY'" type="primary" :loading="acting" @click="control('start')">开始</ElButton>
            <ElButton v-if="run.status === 'RUNNING'" :loading="acting" @click="control('pause')">暂停</ElButton>
            <ElButton v-if="run.status === 'PAUSED'" type="success" :loading="acting" @click="control('resume')">恢复</ElButton>
            <ElButton :disabled="!canPrevious" :loading="acting" @click="control('previous')">回退一步</ElButton>
            <ElButton type="primary" :disabled="!canNext" :loading="acting" @click="control('next')">揭晓下一步</ElButton>
            <ElButton type="danger" plain :disabled="!canComplete" :loading="acting" @click="completeRun">完成 Resolver</ElButton>
          </div>
        </div>
        <div class="resolver-command-bar resolver-auto-play">
          <div><strong>自动播放</strong><small>间隔范围 500–60000 ms；暂停、完成或到达末尾会自动关闭。</small></div>
          <div class="resolver-command-actions"><ElInputNumber v-model="autoInterval" :min="500" :max="60000" :step="500" /><ElButton :type="run.autoPlayEnabled ? 'warning' : 'success'" :disabled="run.status !== 'RUNNING' || run.currentStep >= run.totalSteps" :loading="acting" @click="toggleAutoPlay">{{ run.autoPlayEnabled ? '停止自动播放' : '启动自动播放' }}</ElButton></div>
        </div>
      </ElCard>

      <div v-if="run" class="resolver-workspace">
        <ElCard shadow="never" class="resolver-focus-card">
          <template #header><div class="card-header"><div><strong>最近揭晓</strong><small>步骤 {{ run.currentStep }} / {{ run.totalSteps }}</small></div></div></template>
          <div v-if="focusRow && run.state.lastReveal" class="resolver-focus">
            <div class="resolver-team-identity"><span class="resolver-rank">#{{ focusRow.rank }}</span><div><h2>{{ focusRow.teamName }}</h2><p>{{ focusRow.school ?? '学校未填写' }}</p></div></div>
            <div class="resolver-reveal-grid"><div><span>题目</span><strong>{{ focusProblem?.alias ?? run.state.lastReveal.problemId }}</strong></div><div><span>结果</span><strong :class="run.state.lastReveal.after.solved ? 'accepted' : 'rejected'">{{ run.state.lastReveal.after.solved ? 'ACCEPTED' : '未通过' }}</strong></div><div><span>当前成绩</span><strong>{{ focusRow.solvedCount }} 题 / {{ focusRow.penaltyMinutes }} 分钟</strong></div></div>
          </div>
          <ElEmpty v-else description="尚未揭晓步骤" />
        </ElCard>
        <ElCard shadow="never" class="resolver-history-card">
          <template #header><div class="card-header"><div><strong>操作历史</strong><small>最近 {{ events.length }} 条</small></div></div></template>
          <div class="resolver-event-list"><article v-for="event in events.slice(-12).reverse()" :key="event.id"><span>{{ event.sequence }}</span><div><strong>{{ eventLabel(event.eventType) }}</strong><small>{{ formatDateTime(event.createdAt) }}</small></div></article></div>
        </ElCard>
      </div>

      <ElCard v-if="run" shadow="never" class="resolver-table-card">
        <template #header><div class="card-header"><div><strong>当前 Resolver 榜单</strong><small>状态数据由后端快照校验恢复</small></div></div></template>
        <ElTable :data="run.state.board.rows" row-key="teamId" stripe max-height="560"><ElTableColumn prop="rank" label="排名" width="80" /><ElTableColumn label="队伍" min-width="220"><template #default="{ row }"><div class="resolver-table-team"><strong>{{ row.teamName }}</strong><span>{{ row.school ?? '—' }}</span></div></template></ElTableColumn><ElTableColumn prop="solvedCount" label="解题" width="80" /><ElTableColumn prop="penaltyMinutes" label="罚时" width="90" /><ElTableColumn label="题目状态" min-width="300"><template #default="{ row }"><div class="resolver-cell-list"><span v-for="cell in row.problems" :key="cell.problemId" :class="{ solved: cell.solved, attempted: !cell.solved && cell.wrongAttempts > 0 }">{{ problemAlias(cell.problemId) }} {{ cell.solved ? `+${cell.wrongAttempts || ''}` : cell.wrongAttempts ? `-${cell.wrongAttempts}` : '·' }}</span></div></template></ElTableColumn></ElTable>
      </ElCard>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { Monitor, Refresh } from '@element-plus/icons-vue';
import { useRoute, useRouter } from 'vue-router';
import { contestApi } from '../api/contest';
import { getErrorMessage } from '../api/client';
import { resolverApi, type ResolverEvent, type ResolverRun, type ResolverRunStatus, type ResolverSources } from '../api/resolver';
import type { Contest } from '../api/types';
import { subscribeContestEvents, type ContestRealtimeSubscription } from '../realtime/contest-events';
import { formatDateTime } from '../utils/format';

const route = useRoute(); const router = useRouter();
const contests = ref<Contest[]>([]); const contestId = ref<number | null>(null); const runs = ref<ResolverRun[]>([]); const runId = ref<number | null>(null); const run = ref<ResolverRun | null>(null); const sources = ref<ResolverSources | null>(null); const events = ref<ResolverEvent[]>([]);
const loading = ref(false); const acting = ref(false); const errorMessage = ref(''); const realtimeConnected = ref(false); const autoInterval = ref(3000); let realtime: ContestRealtimeSubscription | undefined; let generation = 0;
const hasOfficial = computed(() => runs.value.some((item) => item.official));
const canNext = computed(() => run.value?.status === 'RUNNING' && run.value.currentStep < run.value.totalSteps);
const canPrevious = computed(() => !!run.value && ['RUNNING', 'PAUSED'].includes(run.value.status) && run.value.currentStep > 0);
const canComplete = computed(() => !!run.value && ['RUNNING', 'PAUSED'].includes(run.value.status) && run.value.currentStep === run.value.totalSteps);
const focusRow = computed(() => run.value?.state.board.rows.find((row) => row.teamId === run.value?.state.lastReveal?.teamId) ?? null);
const focusProblem = computed(() => run.value?.state.board.problems.find((problem) => problem.problemId === run.value?.state.lastReveal?.problemId) ?? null);
function statusLabel(status: ResolverRunStatus) { return { READY: '就绪', RUNNING: '运行中', PAUSED: '已暂停', COMPLETED: '已完成' }[status]; }
function eventLabel(type: string) { return { CREATED: '创建运行', START: '开始', NEXT: '揭晓下一步', PREVIOUS: '回退', PAUSE: '暂停', RESUME: '恢复', COMPLETE: '完成', AUTO_PLAY: '自动播放设置', AUTO_NEXT: '自动揭晓' }[type] ?? type; }
function problemAlias(id: number) { return run.value?.state.board.problems.find((problem) => problem.problemId === id)?.alias ?? String(id); }

async function loadRun(id: number, silent = false) { try { const [next, history] = await Promise.all([resolverApi.get(id), resolverApi.events(id)]); if (runId.value !== id) return; run.value = next; events.value = history; autoInterval.value = next.autoPlayIntervalMilliseconds; if (!silent) errorMessage.value = ''; } catch (error) { if (!silent) errorMessage.value = getErrorMessage(error); } }
async function loadContext() {
  if (!contestId.value) return; const id = contestId.value; const request = ++generation; loading.value = true;
  try { const [nextRuns, nextSources] = await Promise.all([resolverApi.list(id), resolverApi.sources(id).catch(() => null)]); if (request !== generation || id !== contestId.value) return; runs.value = nextRuns; sources.value = nextSources; const requested = Number(route.query.runId); runId.value = nextRuns.some((item) => item.id === requested) ? requested : (nextRuns[0]?.id ?? null); if (runId.value) await loadRun(runId.value); else { run.value = null; events.value = []; } errorMessage.value = ''; }
  catch (error) { if (request === generation) errorMessage.value = getErrorMessage(error); } finally { if (request === generation) loading.value = false; }
}
function connectRealtime() { realtime?.stop(); realtimeConnected.value = false; if (!contestId.value) return; realtime = subscribeContestEvents({ contestId: contestId.value, scope: 'STAFF', eventTypes: ['RESOLVER_STATE_CHANGED'], onEvent: () => { if (runId.value) void loadRun(runId.value, true); }, onConnectionChange: (value) => { realtimeConnected.value = value; }, poll: async () => { if (runId.value) await loadRun(runId.value, true); } }); }
async function changeContest() { generation += 1; run.value = null; runs.value = []; sources.value = null; runId.value = null; await router.replace({ query: contestId.value ? { contestId: String(contestId.value) } : {} }); connectRealtime(); await loadContext(); }
async function selectRun() { if (!runId.value) return; await router.replace({ query: { contestId: String(contestId.value), runId: String(runId.value) } }); await loadRun(runId.value); }
async function createRun(official: boolean) { if (!contestId.value || !sources.value) return; try { await ElMessageBox.confirm(official ? '正式运行每场比赛只能创建一次，确认快照均已复核？' : '确认使用当前最新快照创建预演运行？', official ? '创建正式 Resolver' : '创建预演', { type: 'warning' }); } catch { return; } acting.value = true; try { const created = await resolverApi.create(contestId.value, sources.value.publicSnapshot.id, sources.value.finalSnapshot.id, official); runs.value = [created, ...runs.value]; runId.value = created.id; run.value = created; events.value = await resolverApi.events(created.id); await selectRun(); ElMessage.success(official ? '正式运行已创建' : '预演已创建'); } catch (error) { ElMessage.error(getErrorMessage(error)); } finally { acting.value = false; } }
async function control(action: 'start' | 'next' | 'previous' | 'pause' | 'resume') { if (!run.value) return; acting.value = true; try { run.value = await resolverApi[action](run.value.id, run.value.version); events.value = await resolverApi.events(run.value.id); } catch (error) { ElMessage.error(getErrorMessage(error)); await loadRun(run.value.id, true); } finally { acting.value = false; } }
async function completeRun() { if (!run.value || !canComplete.value) return; try { await ElMessageBox.confirm('确认所有步骤均已复核并完成 Resolver？', '完成 Resolver', { type: 'warning' }); } catch { return; } acting.value = true; try { run.value = await resolverApi.complete(run.value.id, run.value.version); ElMessage.success('Resolver 已完成'); } catch (error) { ElMessage.error(getErrorMessage(error)); } finally { acting.value = false; } }
async function toggleAutoPlay() { if (!run.value) return; acting.value = true; try { run.value = await resolverApi.autoPlay(run.value.id, run.value.version, !run.value.autoPlayEnabled, autoInterval.value); } catch (error) { ElMessage.error(getErrorMessage(error)); } finally { acting.value = false; } }
onMounted(async () => { try { contests.value = (await contestApi.listContests()).content; const requested = Number(route.query.contestId); contestId.value = contests.value.some((item) => item.id === requested) ? requested : (contests.value[0]?.id ?? null); if (contestId.value) { connectRealtime(); await loadContext(); } } catch (error) { errorMessage.value = getErrorMessage(error); } });
onUnmounted(() => realtime?.stop());
</script>
