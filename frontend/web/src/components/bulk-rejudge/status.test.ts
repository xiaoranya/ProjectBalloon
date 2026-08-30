import { describe, expect, it } from 'vitest';

import type {
  BatchRejudgeItem,
  BatchRejudgeItemStatus,
  BatchRejudgeTask,
  BatchRejudgeTaskStatus,
} from '../../api/bulk-rejudge';
import {
  itemStatusLabel,
  itemStatusType,
  progressPercentage,
  progressStatus,
  taskStatusLabel,
  taskStatusType,
} from './status';

function taskWith(overrides: Partial<BatchRejudgeTask>): BatchRejudgeTask {
  const items: BatchRejudgeItem[] = [];
  return {
    id: 1,
    contestId: 7,
    status: 'RUNNING',
    totalItems: 0,
    processedItems: 0,
    succeededItems: 0,
    failedItems: 0,
    cancelRequested: false,
    createdByUserId: 11,
    startedAt: null,
    completedAt: null,
    createdAt: '2026-08-30T10:00:00Z',
    updatedAt: '2026-08-30T10:00:00Z',
    itemsTruncated: false,
    items,
    ...overrides,
  };
}

describe('progressPercentage', () => {
  it('is zero for empty tasks', () => {
    expect(progressPercentage(taskWith({ totalItems: 0, processedItems: 0 }))).toBe(0);
  });

  it('rounds the processed ratio', () => {
    expect(progressPercentage(taskWith({ totalItems: 3, processedItems: 2 }))).toBe(67);
    expect(progressPercentage(taskWith({ totalItems: 4, processedItems: 4 }))).toBe(100);
  });

  it('never exceeds one hundred percent', () => {
    expect(progressPercentage(taskWith({ totalItems: 2, processedItems: 5 }))).toBe(100);
  });
});

describe('progressStatus', () => {
  it('flags any failure as exception before completion state', () => {
    expect(progressStatus(taskWith({ status: 'RUNNING', failedItems: 1 }))).toBe('exception');
  });

  it('marks completed, paused, and active tasks distinctly', () => {
    expect(progressStatus(taskWith({ status: 'COMPLETED' }))).toBe('success');
    expect(progressStatus(taskWith({ status: 'PAUSED' }))).toBe('warning');
    expect(progressStatus(taskWith({ status: 'PENDING' }))).toBe('');
    expect(progressStatus(taskWith({ status: 'RUNNING' }))).toBe('');
  });
});

describe('status maps', () => {
  it.each<BatchRejudgeTaskStatus>(['PENDING', 'RUNNING', 'PAUSED', 'COMPLETED', 'CANCELLED'])(
    'has a tag type and label for task status %s',
    (status) => {
      expect(taskStatusType(status)).toBeTruthy();
      expect(taskStatusLabel(status)).not.toBe('');
    },
  );

  it.each<BatchRejudgeItemStatus>(['PENDING', 'PROCESSING', 'SUCCEEDED', 'FAILED', 'CANCELLED'])(
    'has a tag type and label for item status %s',
    (status) => {
      expect(itemStatusType(status)).toBeTruthy();
      expect(itemStatusLabel(status)).not.toBe('');
    },
  );

  it('labels the workflow-critical states distinctly', () => {
    expect(taskStatusLabel('PENDING')).toBe('等待执行');
    expect(taskStatusLabel('CANCELLED')).toBe('已终止');
    expect(itemStatusLabel('PROCESSING')).toBe('处理中');
    expect(itemStatusType('SUCCEEDED')).toBe('success');
    expect(itemStatusType('FAILED')).toBe('danger');
  });
});
