---
name: verify
summary: Verify the Vue contestant frontend in a real browser
---

# Frontend verification

1. Install dependencies with `npm ci` when network access is authorized.
2. Start the app with `npm run dev -- --host 127.0.0.1 --port 5173`.
3. Use Playwright Chromium against `http://127.0.0.1:5173`.
4. When the Rust API stack is unavailable, intercept `/api/**` with Rust-shaped DTOs and drive:
   - protected route → login redirect and redirect restoration;
   - login and forced-password-change paths;
   - contest list, problem list/detail, submission list/detail, and scoreboard.
5. Capture screenshots and fail on page errors or console errors other than the expected anonymous `/api/auth/me` 401.
6. For full end-to-end verification, run the Rust API with PostgreSQL, Redis, RabbitMQ, and RustFS, then repeat without request interception.
