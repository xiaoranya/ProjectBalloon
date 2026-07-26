import http from 'k6/http';
import exec from 'k6/execution';
import { SharedArray } from 'k6/data';
import { check, sleep } from 'k6';
import {
  baseUrl, contestId, enableWrites, options as configuredOptions, profile,
  profileName, resolverRunId, sourceCode, sourceLanguage,
} from './config.js';
import { checked, csrfToken, login } from './lib/session.js';

export const options = configuredOptions;

const accounts = new SharedArray('pressure test accounts', () => {
  const parsed = JSON.parse(open(__ENV.ACCOUNTS_FILE));
  if (!Array.isArray(parsed) || parsed.length === 0) {
    throw new Error('ACCOUNTS_FILE must contain a non-empty JSON array.');
  }
  for (const account of parsed) {
    if (!account.username || !account.password) {
      throw new Error('Every pressure-test account needs username and password.');
    }
  }
  if (profileName !== 'smoke' && parsed.length < profile.loginUsers) {
    throw new Error(`${profileName} needs ${profile.loginUsers} unique accounts.`);
  }
  return parsed;
});

let session;
let screen;
let screenCsrf;

function accountForVu() {
  return accounts[(exec.vu.idInTest - 1) % accounts.length];
}

function accountForIteration() {
  return accounts[exec.scenario.iterationInTest % accounts.length];
}

function authenticated() {
  if (!session) session = login(baseUrl, accountForVu());
  return session;
}

function loadProblems() {
  const response = http.get(`${baseUrl}/api/contests/${contestId}/problems`, {
    tags: { operation: 'problem_statements' },
  });
  if (!checked(response, 'problem statements')) return [];
  const problems = response.json();
  check(problems, {
    'at least one problem is visible': (items) => Array.isArray(items) && items.length > 0,
    'problem statement is rendered': (items) =>
      Array.isArray(items) && items.some((problem) => Boolean(problem.statement?.renderedHtml)),
  });
  return problems;
}

export function loginScenario() {
  login(baseUrl, accountForIteration());
}

export function readScenario() {
  if (!authenticated()) { sleep(1); return; }
  loadProblems();
  checked(http.get(`${baseUrl}/api/contests/${contestId}/scoreboard`, {
    tags: { operation: 'scoreboard' },
  }), 'scoreboard');
  checked(http.get(`${baseUrl}/api/contests/${contestId}/submissions?page=0&size=20`, {
    tags: { operation: 'submission_list' },
  }), 'submission list');
  if (resolverRunId > 0) {
    checked(http.get(`${baseUrl}/api/public/resolver-runs/${resolverRunId}/state`, {
      tags: { operation: 'resolver' },
    }), 'resolver');
  }
  sleep(profileName === 'smoke' ? 1 : 3);
}

const mixedLanguages = ['c', 'cpp', 'java', 'python'];
const defaultSources = {
  c: '#include <stdio.h>\nint main(void){return 0;}\n',
  cpp: '#include <iostream>\nint main(){return 0;}\n',
  java: 'public class Main { public static void main(String[] args) {} }\n',
  python: '# Intentionally produces no output.\n',
};

function languageForIteration() {
  if (sourceLanguage !== 'mixed') return sourceLanguage;
  return mixedLanguages[exec.scenario.iterationInTest % mixedLanguages.length];
}

function sourceFilename(language) {
  return { c: 'main.c', cpp: 'main.cpp', java: 'Main.java', python: 'main.py' }[language];
}

export function submissionScenario() {
  const csrf = authenticated();
  if (!csrf) return;
  const problems = loadProblems();
  if (problems.length === 0) return;
  const problem = problems[exec.scenario.iterationInTest % problems.length];
  const language = languageForIteration();
  const filename = sourceFilename(language);
  if (!filename) throw new Error(`Unsupported SUBMISSION_LANGUAGE ${sourceLanguage}.`);
  const submissionSource = sourceCode || defaultSources[language];
  const response = http.post(
    `${baseUrl}/api/contests/${contestId}/submissions`,
    {
      metadata: http.file(JSON.stringify({ problemId: problem.problemId, language }),
        'metadata.json', 'application/json'),
      source: http.file(submissionSource, filename, 'text/plain'),
    },
    { headers: { [csrf.headerName]: csrf.token }, tags: { operation: 'submission_create' } },
  );
  checked(response, 'submission', [202]);
}

export function printScenario() {
  const csrf = login(baseUrl, accountForIteration());
  if (!csrf) return;
  const response = http.post(
    `${baseUrl}/api/contests/${contestId}/print-requests`,
    JSON.stringify({ content: `Pressure rehearsal request ${exec.scenario.iterationInTest}` }),
    {
      headers: { 'Content-Type': 'application/json', [csrf.headerName]: csrf.token },
      tags: { operation: 'print_create' },
    },
  );
  checked(response, 'print request', [201]);
}

export function screenScenario() {
  if (!screen) {
    screenCsrf = csrfToken(baseUrl);
    if (!screenCsrf) { sleep(1); return; }
    const response = http.post(
      `${baseUrl}/api/public/screens/register`,
      JSON.stringify({ contestId, name: `k6-${exec.vu.idInTest}` }),
      {
        headers: { 'Content-Type': 'application/json', [screenCsrf.headerName]: screenCsrf.token },
        tags: { operation: 'screen_register' },
      },
    );
    if (!checked(response, 'screen registration', [201])) { sleep(1); return; }
    screen = response.json();
  }
  checked(http.post(
    `${baseUrl}/api/public/screens/${screen.instanceId}/heartbeat`,
    JSON.stringify({ clientToken: screen.clientToken, currentView: 'SCOREBOARD' }),
    {
      headers: { 'Content-Type': 'application/json', [screenCsrf.headerName]: screenCsrf.token },
      tags: { operation: 'screen_heartbeat' },
    },
  ), 'screen heartbeat');
  sleep(profileName === 'smoke' ? 1 : 10);
}

export function handleSummary(data) {
  const directory = __ENV.REPORT_DIR || 'build/reports/k6';
  return {
    [`${directory}/summary.json`]: JSON.stringify(data, null, 2),
    stdout: `k6 ${profileName} profile complete; full summary: ${directory}/summary.json\n`,
  };
}
