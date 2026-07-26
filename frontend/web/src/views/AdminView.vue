<template>
  <section class="admin-page operations-page">
    <header class="admin-page-header">
      <div>
        <p class="eyebrow">System Operations</p>
        <h1>健康状态与审计日志</h1>
        <p>检查 Rust API 的依赖、任务积压、工作节点与管理操作记录。</p>
      </div>
      <ElButton :icon="Refresh" :loading="refreshing" @click="refreshAll">刷新状态</ElButton>
    </header>

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
        <div>
          <h2>系统健康</h2>
          <p>{{ health ? `最近检查：${formatDateTime(health.time)}` : '正在读取就绪状态' }}</p>
        </div>
        <ElTag v-if="health" :type="health.status === 'up' ? 'success' : 'danger'" size="large">
          {{ health.status === 'up' ? '运行正常' : '存在异常' }}
        </ElTag>
      </div>

      <div v-loading="healthLoading" class="health-component-grid">
        <ElCard
          v-for="component in healthComponents"
          :key="component.name"
          shadow="never"
          class="health-component-card"
          :class="{ down: component.status === 'down', neutral: component.status === 'neutral' }"
        >
          <div class="health-component-title">
            <span>{{ component.name }}</span>
            <ElTag :type="healthTagType(component.status)" effect="light">
              {{ healthStatusLabel(component.status) }}
            </ElTag>
          </div>
          <dl class="health-component-details">
            <template v-for="detail in component.details" :key="detail.label">
              <dt>{{ detail.label }}</dt>
              <dd>{{ detail.value }}</dd>
            </template>
          </dl>
        </ElCard>
      </div>
    </section>

    <section v-if="session.isSuperAdmin.value" class="operations-audit">
      <div class="operations-section-heading">
        <div>
          <h2>审计日志</h2>
          <p>共 {{ auditPage.totalElements }} 条记录，按时间倒序排列。</p>
        </div>
      </div>

      <ElCard shadow="never" class="admin-card">
        <div class="audit-filters">
          <ElInput v-model="filters.action" clearable placeholder="操作名称" @keyup.enter="applyFilters" />
          <ElInputNumber
            v-model="filters.actorUserId"
            :min="1"
            :controls="false"
            placeholder="操作人 ID"
          />
          <ElSelect v-model="filters.result" clearable placeholder="全部结果">
            <ElOption label="成功" value="success" />
            <ElOption label="失败" value="failed" />
          </ElSelect>
          <ElDatePicker
            v-model="filters.timeRange"
            type="datetimerange"
            start-placeholder="开始时间"
            end-placeholder="结束时间"
            range-separator="至"
          />
          <ElButton type="primary" @click="applyFilters">查询</ElButton>
          <ElButton @click="resetFilters">重置</ElButton>
        </div>

        <ElTable v-loading="auditLoading" :data="auditPage.content" row-key="id">
          <ElTableColumn label="时间" min-width="175">
            <template #default="{ row }">{{ formatDateTime(row.createdAt) }}</template>
          </ElTableColumn>
          <ElTableColumn prop="action" label="操作" min-width="210" />
          <ElTableColumn label="操作人" width="105">
            <template #default="{ row }">{{ row.actorUserId ?? '系统' }}</template>
          </ElTableColumn>
          <ElTableColumn label="目标" min-width="180">
            <template #default="{ row }">
              {{ [row.targetType, row.targetId].filter(Boolean).join(' #') || '—' }}
            </template>
          </ElTableColumn>
          <ElTableColumn label="来源 IP" min-width="135">
            <template #default="{ row }">{{ row.requestIp || '—' }}</template>
          </ElTableColumn>
          <ElTableColumn label="结果" width="100">
            <template #default="{ row }">
              <ElTag :type="resultTagType(row.result)" effect="light">{{ resultLabel(row.result) }}</ElTag>
            </template>
          </ElTableColumn>
          <template #empty>
            <ElEmpty description="没有符合条件的审计记录" />
          </template>
        </ElTable>

        <ElPagination
          v-if="auditPage.totalElements > pageSize"
          v-model:current-page="currentPage"
          :page-size="pageSize"
          :total="auditPage.totalElements"
          layout="prev, pager, next, total"
          class="audit-pagination"
          @current-change="loadAuditLogs"
        />
      </ElCard>
    </section>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { Refresh } from '@element-plus/icons-vue';
import { adminApi } from '../api/admin';
import { getErrorMessage } from '../api/client';
import type { AuditLog, HealthResponse, PageResponse } from '../api/types';
import { useSession } from '../auth/session';
import { formatDateTime } from '../utils/format';

type CardStatus = 'up' | 'down' | 'neutral';
interface HealthCard {
  name: string;
  status: CardStatus;
  details: Array<{ label: string; value: string | number }>;
}

const session = useSession();
const pageSize = 25;
const health = ref<HealthResponse | null>(null);
const healthLoading = ref(false);
const auditLoading = ref(false);
const refreshing = ref(false);
const errorMessage = ref('');
const currentPage = ref(1);
const auditPage = ref<PageResponse<AuditLog>>({
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
  const cards: HealthCard[] = [{
    name: 'API 与 PostgreSQL',
    status: health.value.status,
    details: [{ label: '服务', value: health.value.service }],
  }];
  const realtime = health.value.realtimeOutbox;
  cards.push({
    name: '实时事件',
    status: realtime?.redisConnected === false ? 'down' : realtime ? 'up' : 'neutral',
    details: realtime
      ? [
          { label: '待投递', value: realtime.pending },
          { label: '失败', value: realtime.failed },
          { label: 'Redis', value: realtime.redisConnected === undefined ? '未配置' : realtime.redisConnected ? '已连接' : '未连接' },
        ]
      : [{ label: '状态', value: '数据库探测不可用' }],
  });
  const judge = health.value.judgeDispatch;
  cards.push({
    name: '判题调度与 Worker',
    status: judge?.rabbitmq?.status ?? (judge ? 'up' : 'neutral'),
    details: judge
      ? [
          { label: '待投递', value: judge.pending },
          { label: '失败', value: judge.failed },
          { label: '在线 / 过期', value: `${judge.workers.online} / ${judge.workers.stale}` },
          { label: '活跃 / 容量', value: `${judge.workers.activeTasks} / ${judge.workers.capacity}` },
        ]
      : [{ label: '状态', value: '数据库探测不可用' }],
  });
  if (judge?.rabbitmq) {
    cards.push({
      name: 'RabbitMQ',
      status: judge.rabbitmq.status,
      details: [
        { label: '任务队列', value: judge.rabbitmq.queuedTasks },
        { label: '结果队列', value: judge.rabbitmq.queuedResults },
        { label: '死信任务', value: judge.rabbitmq.deadTasks },
      ],
    });
  }
  cards.push(dependencyCard('对象存储', health.value.objectStorage));
  cards.push(dependencyCard('CUPS 打印', health.value.cups));
  return cards;
});

function dependencyCard(name: string, dependency: { status: 'up' | 'down' } | undefined): HealthCard {
  return {
    name,
    status: dependency?.status ?? 'neutral',
    details: [{ label: '状态', value: dependency ? healthStatusLabel(dependency.status) : '未配置' }],
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
  if (status === 'up') return '正常';
  if (status === 'down') return '异常';
  return '未配置';
}

function healthTagType(status: CardStatus) {
  if (status === 'up') return 'success';
  if (status === 'down') return 'danger';
  return 'info';
}

function resultLabel(result: string) {
  return result.toLowerCase() === 'success' ? '成功' : result.toLowerCase() === 'failed' ? '失败' : result;
}

function resultTagType(result: string) {
  return result.toLowerCase() === 'success' ? 'success' : result.toLowerCase() === 'failed' ? 'danger' : 'info';
}

onMounted(refreshAll);
</script>
