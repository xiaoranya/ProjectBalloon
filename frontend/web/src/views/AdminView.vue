<template>
  <el-container direction="vertical" class="admin-page">
    <el-header height="auto" class="page-head">
      <div class="admin-page-header compact">
        <h1>{{ t('健康状态与审计日志') }}</h1>
        <ElButton :icon="Refresh" :loading="refreshing" @click="refreshAll">{{
          t('刷新状态')
        }}</ElButton>
      </div>
    </el-header>

    <el-main class="page-body">
      <ElAlert
        v-if="errorMessage"
        :title="errorMessage"
        type="error"
        show-icon
        :closable="false"
        class="page-alert"
      />

      <section class="operations-health">
        <div class="operations-section-heading">
          <h2>{{ t('系统健康') }}</h2>
          <ElTag v-if="health" :type="health.status === 'up' ? 'success' : 'danger'" size="large">
            {{ health.status === 'up' ? t('运行正常') : t('存在异常') }}
          </ElTag>
        </div>

        <ElRow v-loading="healthLoading" :gutter="14" class="health-component-grid">
          <ElCol
            v-for="component in healthComponents"
            :key="component.name"
            :xs="24"
            :sm="12"
            :md="8"
            :lg="6"
            :xl="6"
          >
            <ElCard
              shadow="never"
              class="health-component-card"
              :class="{
                down: component.status === 'down',
                neutral: component.status === 'neutral',
              }"
            >
              <div class="health-component-title">
                <span>{{ component.name }}</span>
                <ElTag :type="healthTagType(component.status)" effect="light">
                  {{ healthStatusLabel(component.status) }}
                </ElTag>
              </div>
              <el-descriptions :column="1" size="small">
                <el-descriptions-item
                  v-for="detail in component.details"
                  :key="detail.label"
                  :label="detail.label"
                >
                  {{ detail.value }}
                </el-descriptions-item>
              </el-descriptions>
            </ElCard>
          </ElCol>
        </ElRow>
      </section>

      <section v-if="session.isSuperAdmin.value" class="operations-audit">
        <div class="operations-section-heading">
          <h2>{{ t('审计日志') }}</h2>
        </div>

        <ElCard shadow="never">
          <el-form inline class="audit-filters" @submit.prevent="applyFilters">
            <ElSpace wrap :size="10">
              <ElInput
                v-model="filters.action"
                clearable
                :placeholder="t('操作名称')"
                @keyup.enter="applyFilters"
              />
              <ElInputNumber
                v-model="filters.actorUserId"
                :min="1"
                :controls="false"
                :placeholder="t('操作人 ID')"
              />
              <ElSelect v-model="filters.result" clearable :placeholder="t('全部结果')">
                <ElOption :label="t('成功')" value="success" />
                <ElOption :label="t('失败')" value="failed" />
              </ElSelect>
              <ElDatePicker
                v-model="filters.timeRange"
                type="datetimerange"
                :start-placeholder="t('开始时间')"
                :end-placeholder="t('结束时间')"
                :range-separator="t('至')"
              />
              <ElButton type="primary" @click="applyFilters">{{ t('查询') }}</ElButton>
              <ElButton @click="resetFilters">{{ t('重置') }}</ElButton>
            </ElSpace>
          </el-form>

          <ElTable v-loading="auditLoading" :data="auditPage.content" row-key="id">
            <ElTableColumn :label="t('时间')" min-width="175">
              <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
            </ElTableColumn>
            <ElTableColumn prop="action" :label="t('操作')" min-width="210" />
            <ElTableColumn :label="t('操作人')" width="105">
              <template #default="{ row }">{{ row.actorUserId ?? t('系统') }}</template>
            </ElTableColumn>
            <ElTableColumn :label="t('目标')" min-width="180">
              <template #default="{ row }">
                {{ [row.targetType, row.targetId].filter(Boolean).join(' #') || '—' }}
              </template>
            </ElTableColumn>
            <ElTableColumn :label="t('来源 IP')" min-width="135">
              <template #default="{ row }">{{ row.requestIp || '—' }}</template>
            </ElTableColumn>
            <ElTableColumn :label="t('结果')" width="100">
              <template #default="{ row }">
                <ElTag :type="resultTagType(row.result)" effect="light">{{
                  resultLabel(row.result)
                }}</ElTag>
              </template>
            </ElTableColumn>
            <template #empty>
              <ElEmpty :description="t('没有符合条件的审计记录')" />
            </template>
          </ElTable>

          <ElRow justify="end" class="audit-pagination">
            <ElPagination
              v-if="auditPage.totalElements > pageSize"
              v-model:current-page="currentPage"
              :page-size="pageSize"
              :total="auditPage.totalElements"
              layout="prev, pager, next, total"
              @current-change="loadAuditLogs"
            />
          </ElRow>
        </ElCard>
      </section>
    </el-main>
  </el-container>
</template>
<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Refresh } from '@element-plus/icons-vue';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import type { AuditLogResponse, HealthResponse, PageResponse } from '../api/types';
import { useSession } from '../auth/session';
import { formatDateTime } from '../utils/format';
import { useI18n } from '../i18n';

type CardStatus = 'up' | 'down' | 'neutral';
interface HealthCard {
  name: string;
  status: CardStatus;
  details: Array<{ label: string; value: string | number }>;
}

const session = useSession();
const { t } = useI18n();
const pageSize = 25;
const health = ref<HealthResponse | null>(null);
const healthLoading = ref(false);
const auditLoading = ref(false);
const refreshing = ref(false);
const errorMessage = ref('');
const currentPage = ref(1);
const auditPage = ref<PageResponse<AuditLogResponse>>({
  content: [],
  page: 0,
  size: pageSize,
  totalElements: 0,
  totalPages: 0,
});
const filters = reactive<{
  action: string;
  actorUserId: number | undefined;
  result: string;
  timeRange: [Date, Date] | [];
}>({
  action: '',
  actorUserId: undefined,
  result: '',
  timeRange: [],
});

const healthComponents = computed<HealthCard[]>(() => {
  if (!health.value) return [];
  const cards: HealthCard[] = [
    {
      name: t('API 与 PostgreSQL'),
      status: health.value.status,
      details: [{ label: t('服务'), value: health.value.service }],
    },
  ];
  const realtime = health.value.realtimeOutbox;
  cards.push({
    name: t('实时事件'),
    status: realtime?.redisConnected === false ? 'down' : realtime ? 'up' : 'neutral',
    details: realtime
      ? [
          { label: t('待投递'), value: realtime.pending },
          { label: t('失败'), value: realtime.failed },
          {
            label: 'Redis',
            value:
              realtime.redisConnected === undefined
                ? t('未配置')
                : realtime.redisConnected
                  ? t('已连接')
                  : t('未连接'),
          },
        ]
      : [{ label: t('状态'), value: t('数据库探测不可用') }],
  });
  const judge = health.value.judgeDispatch;
  cards.push({
    name: t('判题调度与 Worker'),
    status: judge?.rabbitmq?.status ?? (judge ? 'up' : 'neutral'),
    details: judge
      ? [
          { label: t('待投递'), value: judge.pending },
          { label: t('失败'), value: judge.failed },
          { label: t('在线 / 过期'), value: `${judge.workers.online} / ${judge.workers.stale}` },
          {
            label: t('活跃 / 容量'),
            value: `${judge.workers.activeTasks} / ${judge.workers.capacity}`,
          },
        ]
      : [{ label: t('状态'), value: t('数据库探测不可用') }],
  });
  if (judge?.rabbitmq) {
    cards.push({
      name: 'RabbitMQ',
      status: judge.rabbitmq.status,
      details: [
        { label: t('任务队列'), value: judge.rabbitmq.queuedTasks },
        { label: t('结果队列'), value: judge.rabbitmq.queuedResults },
        { label: t('死信任务'), value: judge.rabbitmq.deadTasks },
      ],
    });
  }
  cards.push(dependencyCard(t('对象存储'), health.value.objectStorage));
  const cleanup = health.value.objectCleanup;
  cards.push({
    name: t('对象存储一致性'),
    status:
      cleanup && (cleanup.failed > 0 || cleanup.missingReferences > 0)
        ? 'down'
        : cleanup
          ? 'up'
          : 'neutral',
    details: cleanup
      ? [
          { label: t('待清理'), value: cleanup.pending },
          { label: t('清理失败'), value: cleanup.failed },
          { label: t('缺失引用'), value: cleanup.missingReferences },
        ]
      : [{ label: t('状态'), value: t('数据库探测不可用') }],
  });
  cards.push(dependencyCard(t('CUPS 打印'), health.value.cups));
  return cards;
});

function dependencyCard(
  name: string,
  dependency: { status: 'up' | 'down' } | undefined,
): HealthCard {
  return {
    name,
    status: dependency?.status ?? 'neutral',
    details: [
      { label: t('状态'), value: dependency ? healthStatusLabel(dependency.status) : t('未配置') },
    ],
  };
}

async function loadHealth() {
  healthLoading.value = true;
  try {
    health.value = await adminApi.getHealth();
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    healthLoading.value = false;
  }
}

async function loadAuditLogs() {
  auditLoading.value = true;
  try {
    const [from, to] = filters.timeRange;
    auditPage.value = await adminApi.listAuditLogs({
      page: currentPage.value - 1,
      size: pageSize,
      actorUserId: filters.actorUserId,
      action: filters.action.trim() || undefined,
      result: filters.result || undefined,
      from: from?.toISOString(),
      to: to?.toISOString(),
    });
  } catch (error) {
    errorMessage.value = getErrorMessage(error);
  } finally {
    auditLoading.value = false;
  }
}

async function refreshAll() {
  refreshing.value = true;
  errorMessage.value = '';
  const requests: Promise<void>[] = [loadHealth()];
  if (session.isSuperAdmin.value) requests.push(loadAuditLogs());
  await Promise.all(requests);
  refreshing.value = false;
}

function applyFilters() {
  currentPage.value = 1;
  void loadAuditLogs();
}

function resetFilters() {
  filters.action = '';
  filters.actorUserId = undefined;
  filters.result = '';
  filters.timeRange = [];
  applyFilters();
}

function healthStatusLabel(status: CardStatus) {
  if (status === 'up') return t('正常');
  if (status === 'down') return t('异常');
  return t('未配置');
}

function healthTagType(status: CardStatus) {
  if (status === 'up') return 'success';
  if (status === 'down') return 'danger';
  return 'info';
}

function resultLabel(result: string) {
  return result.toLowerCase() === 'success'
    ? t('成功')
    : result.toLowerCase() === 'failed'
      ? t('失败')
      : result;
}

function resultTagType(result: string) {
  return result.toLowerCase() === 'success'
    ? 'success'
    : result.toLowerCase() === 'failed'
      ? 'danger'
      : 'info';
}

onMounted(refreshAll);
</script>

<style scoped>
.operations-health,
.operations-audit {
  margin-bottom: 34px;
}
.operations-section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.operations-section-heading h2,
.operations-section-heading p {
  margin: 0;
}
.operations-section-heading h2 {
  color: #13213b;
}
.health-component-grid {
  min-height: 150px;
}
.health-component-grid .el-col {
  display: flex;
  margin-bottom: 14px;
}
.health-component-grid .el-col > .el-card {
  width: 100%;
}
.health-component-card {
  border: 1px solid #dbe8df;
  border-left: 4px solid #16a34a;
}
.health-component-card.down {
  border-color: #fecaca;
  border-left-color: #dc2626;
}
.health-component-card.neutral {
  border-color: #e2e8f0;
  border-left-color: var(--muted-light);
}
.health-component-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
  font-weight: 700;
  color: #13213b;
}
.audit-filters {
  margin-bottom: 20px;
}
.audit-filters :deep(.el-input) {
  width: 240px;
}
.audit-filters :deep(.el-input-number) {
  width: 130px;
}
.audit-filters :deep(.el-select) {
  width: 130px;
}
.audit-filters :deep(.el-date-editor) {
  width: 360px;
}
.audit-pagination {
  margin-top: 20px;
}
@media (max-width: 680px) {
  .admin-page-header,
  .operations-section-heading {
    align-items: stretch;
    flex-direction: column;
    gap: 14px;
  }
}
</style>
