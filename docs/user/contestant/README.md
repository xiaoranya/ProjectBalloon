# Contestant Guide

This guide covers what a team member does during an online contest: accounts,
submitting code, understanding verdicts, reading the scoreboard, asking
Clarifications, and using the printing queue.

## Accounts and Login

- Accounts are usually imported by the organizers; the team leader receives
  credentials. If self-registration is enabled, use `/register`.
- A generated account may require a password change at first login. Until the
  password is changed, only the authentication flow is accessible.
- Logging out or changing your password revokes the current session and other
  sessions; sign in again afterwards.
- Keep your browser session cookie; the default session lifetime is 12 hours
  (`PROJECT_BALLOON_SESSION_TTL_SECONDS`).

## Contest Pages

After login, `/contests` lists contests your team can join. Opening a contest
gives you these pages:

| Page | Route | Purpose |
| --- | --- | --- |
| Problems | `/contests/:contestId/problems` | Problem list and statements |
| Submissions | `/contests/:contestId/submissions` | Submit code and review history |
| Clarifications | `/contests/:contestId/clarifications` | Ask questions and read announcements |
| Printing | `/contests/:contestId/printing` | Send code to the on-site printer |
| Scoreboard | `/contests/:contestId/scoreboard` | Live standings |

## Submitting Code

- Pick a problem, choose the language, and upload or paste your source file.
- Supported languages are the judge runtime images: C, C++, Java, and Python.
- There is a rate limit of 20 submissions per team per minute; exceeding it
  returns a rate-limit error, so wait and retry rather than spamming.
- Submissions remain in `PENDING`/`JUDGING` while queued; the worker pool
  processes them. During high load, results may be delayed.

### Reading Verdicts

| Verdict | Meaning |
| --- | --- |
| `ACCEPTED` | Your solution passed all test cases |
| `WRONG_ANSWER` | Output differs from the expected answer |
| `TIME_LIMIT_EXCEEDED` | Ran longer than the time limit (Java gets 2x, Python 3x) |
| `MEMORY_LIMIT_EXCEEDED` | Exceeded the memory limit |
| `RUNTIME_ERROR` | Crashed or exited abnormally |
| `COMPILE_ERROR` | Source did not compile or pass the syntax check |
| `OUTPUT_LIMIT_EXCEEDED` | Output exceeded the output limit |
| `SYSTEM_ERROR` | Judge infrastructure problem; ask staff to rejudge |
| `CANCELLED` | The judgement was superseded or cancelled |

The checker normalizes CR/LF, trailing spaces/tabs, and trailing blank lines
before comparing output; everything else is byte-exact.

## Scoreboard and Freeze

- The public scoreboard ranks teams by accepted count then penalty time.
- During the freeze interval (after `freeze_at`), the public board hides
  submissions made after the freeze time. Your own submissions and the admin
  board still show the true state.
- After the contest ends, the public board remains frozen; it does not
  automatically reveal hidden submissions.
- First Blood is tracked per problem for official, non-star teams and shown on
  the board and live views.

## Clarifications

- Ask a question about a problem or the contest from the Clarifications page.
- Public announcements are listed there as well.
- Rate limit: one Clarification per team every 5 minutes.
- Staff may reply privately, convert a question into a public announcement, or
  close it. Replies appear on your Clarifications page.

## Printing (On-Site Contests)

- The printing page accepts source code to be printed at the venue.
- Limits: at most 20 KiB of content and 5 pages per request; one request per
  team every 10 minutes; at most 20 requests per team for the whole contest.
- Track your request status (`QUEUED`, `PRINTING`, `DONE`, `FAILED`). If a job
  fails, ask the printing desk to retry it.

## Profile

Use `/profile` to view team and member information, and `/change-password` to
rotate your password. After changing the password, log in again with the new
one.
