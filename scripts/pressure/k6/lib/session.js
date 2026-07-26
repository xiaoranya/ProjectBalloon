import http from 'k6/http';
import { check } from 'k6';

export function csrfToken(baseUrl) {
  const response = http.get(`${baseUrl}/api/auth/csrf`, { tags: { operation: 'csrf' } });
  const valid = check(response, {
    'csrf token returned': (result) =>
      result.status === 200 && Boolean(result.json('headerName')) && Boolean(result.json('token')),
  });
  return valid ? response.json() : null;
}

export function login(baseUrl, account) {
  const csrf = csrfToken(baseUrl);
  if (!csrf) return null;
  const response = http.post(
    `${baseUrl}/api/auth/login`,
    JSON.stringify({ username: account.username, password: account.password }),
    {
      headers: { 'Content-Type': 'application/json', [csrf.headerName]: csrf.token },
      tags: { operation: 'login' },
    },
  );
  return check(response, { 'login succeeds': (result) => result.status === 200 }) ? csrf : null;
}

export function checked(response, name, expectedStatuses = [200]) {
  const valid = check(response, {
    [`${name} succeeds`]: (result) => expectedStatuses.includes(result.status),
  });
  if (!valid) {
    console.error(`${name} failed: HTTP ${response.status} ${response.body}`);
  }
  return valid;
}
