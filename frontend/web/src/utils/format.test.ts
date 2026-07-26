import { describe, expect, it } from 'vitest';
import {
  isFinalSubmissionStatus,
  statusTagType,
  submissionStatusLabel,
} from './format';

describe('Rust submission status helpers', () => {
  it('keeps active states polling and stops on final verdicts', () => {
    expect(isFinalSubmissionStatus('PENDING')).toBe(false);
    expect(isFinalSubmissionStatus('JUDGING')).toBe(false);
    expect(isFinalSubmissionStatus('ACCEPTED')).toBe(true);
    expect(isFinalSubmissionStatus('COMPILE_ERROR')).toBe(true);
  });

  it('renders full Rust verdict names', () => {
    expect(submissionStatusLabel('ACCEPTED')).toBe('答案正确');
    expect(submissionStatusLabel('TIME_LIMIT_EXCEEDED')).toBe('超出时间限制');
    expect(submissionStatusLabel('SYSTEM_ERROR')).toBe('系统错误');
    expect(statusTagType('ACCEPTED')).toBe('success');
    expect(statusTagType('WRONG_ANSWER')).toBe('danger');
  });
});
