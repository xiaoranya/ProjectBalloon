const profiles = {
  smoke: {
    loginUsers: 1,
    readUsers: 1,
    duration: '10s',
    submissionRate: 1,
    printRate: 1,
    screenUsers: 1,
  },
  rehearsal: {
    loginUsers: 300,
    readUsers: 200,
    duration: '10m',
    submissionRate: 120,
    printRate: 20,
    screenUsers: 20,
  },
  full: {
    loginUsers: 1500,
    readUsers: 500,
    duration: '30m',
    submissionRate: 300,
    printRate: 50,
    screenUsers: 100,
  },
};

export const profileName = __ENV.PROFILE || 'smoke';
export const profile = profiles[profileName];
if (!profile) throw new Error(`Unknown PROFILE "${profileName}".`);

export const baseUrl = (__ENV.BASE_URL || 'http://127.0.0.1:8080').replace(/\/$/, '');
export const contestId = Number(__ENV.CONTEST_ID || '1');
export const resolverRunId = Number(__ENV.RESOLVER_RUN_ID || '0');
export const enableWrites = __ENV.ENABLE_WRITES === 'true';
const enableSubmissions = enableWrites && __ENV.ENABLE_SUBMISSIONS !== 'false';
const enablePrints = enableWrites && __ENV.ENABLE_PRINTS !== 'false';
export const sourceCode = __ENV.SUBMISSION_SOURCE || '';
export const sourceLanguage = (__ENV.SUBMISSION_LANGUAGE || 'c').toLowerCase();

const latencyMs = Number(__ENV.P95_LATENCY_MS || '1500');
const failureRate = Number(__ENV.MAX_FAILURE_RATE || '0.01');
const workloadDuration = __ENV.DURATION || profile.duration;

function scenariosFor(selected) {
  const workloadStartTime = profileName === 'smoke' ? '1s' : '1m30s';
  const scenarios = {
    reads: {
      executor: 'constant-vus',
      exec: 'readScenario',
      vus: selected.readUsers,
      duration: workloadDuration,
      startTime: workloadStartTime,
      gracefulStop: '10s',
      tags: { workload: 'read' },
    },
    screens: {
      executor: 'constant-vus',
      exec: 'screenScenario',
      vus: selected.screenUsers,
      duration: workloadDuration,
      startTime: workloadStartTime,
      gracefulStop: '10s',
      tags: { workload: 'screen' },
    },
  };

  scenarios.logins = profileName === 'smoke'
    ? {
        executor: 'shared-iterations', exec: 'loginScenario', vus: 1,
        iterations: 1, maxDuration: '30s', tags: { workload: 'login' },
      }
    : {
        executor: 'constant-arrival-rate', exec: 'loginScenario',
        rate: selected.loginUsers, timeUnit: '1m', duration: '1m',
        preAllocatedVUs: Math.max(10, Math.ceil(selected.loginUsers / 10)),
        maxVUs: selected.loginUsers, tags: { workload: 'login' },
      };

  if (enableWrites) {
    if (profileName === 'smoke') {
      if (enableSubmissions) {
        scenarios.submissions = {
          executor: 'shared-iterations', exec: 'submissionScenario', vus: 1,
          iterations: 1, maxDuration: '30s', startTime: workloadStartTime,
          tags: { workload: 'submission' },
        };
      }
      if (enablePrints) {
        scenarios.prints = {
          executor: 'shared-iterations', exec: 'printScenario', vus: 1,
          iterations: 1, maxDuration: '30s', startTime: workloadStartTime,
          tags: { workload: 'print' },
        };
      }
    } else {
      if (enableSubmissions) {
        scenarios.submissions = {
          executor: 'constant-arrival-rate', exec: 'submissionScenario',
          rate: selected.submissionRate, timeUnit: '1m', duration: workloadDuration,
          preAllocatedVUs: Math.max(2, Math.ceil(selected.submissionRate / 5)),
          maxVUs: Math.max(10, selected.submissionRate), startTime: workloadStartTime,
          tags: { workload: 'submission' },
        };
      }
      if (enablePrints) {
        scenarios.prints = {
          executor: 'constant-arrival-rate', exec: 'printScenario',
          rate: selected.printRate, timeUnit: '1m', duration: workloadDuration,
          preAllocatedVUs: Math.max(1, Math.ceil(selected.printRate / 5)),
          maxVUs: Math.max(5, selected.printRate), startTime: workloadStartTime,
          tags: { workload: 'print' },
        };
      }
    }
  }
  return scenarios;
}

export const options = {
  noCookiesReset: true,
  scenarios: scenariosFor(profile),
  thresholds: {
    checks: ['rate>0.99'],
    http_req_failed: [`rate<${failureRate}`],
    'http_req_duration{workload:login}': [`p(95)<${latencyMs}`],
    'http_req_duration{workload:read}': [`p(95)<${latencyMs}`],
    'http_req_duration{workload:submission}': [`p(95)<${latencyMs * 2}`],
    dropped_iterations: ['count==0'],
  },
  summaryTrendStats: ['avg', 'min', 'med', 'p(90)', 'p(95)', 'p(99)', 'max'],
};
