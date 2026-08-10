---
title: Daily Practice Guide
description: How contestants browse the public problem bank, join training
  plans, submit daily practice, run personal virtual contests, and read
  editorials.
---

# Daily Practice Guide

This guide is for contestants who use the daily practice area outside official
contests: the public problem bank, training plans, practice submissions,
personal virtual contests, and editorials. The daily area is available in
standard deployment mode; `competition` mode disables it and uses workstation
pairing login instead (see the [Contestant Guide](README.md)).

## Problem Bank (`/problem-bank`)

The problem bank lists published practice problems. You can browse it without
logging in:

- Filter by tag or difficulty.
- Open a problem to read its statement and the languages it accepts.

## Training Plans (`/training`)

Training plans group problems into sets with required and optional items:

- Browse public sets and join one with **Join training**.
- Required problems are marked **Must-do**; the rest are optional.
- Progress is tracked per set and per problem.

## Daily Practice (`/practice`)

The practice page is your workspace for submitting and tracking practice
submissions. You must be logged in.

- Pick a problem, choose a language, and submit code (same judge runtimes as
  contests: C, C++, Java, and Python).
- Use favorites to keep a shortlist; filter by all, favorites, or unsolved.
- Track progress per problem: attempts, best score, and solved status.
- The submission history shows each practice submission and its judgement
  detail (verdict, time, memory, and judgement versions).

Limits are configured by the administrator: daily submission limit (default
200) and concurrent judging limit (default 3). If you hit the daily limit,
wait until the next day.

## Virtual Contests (`/practice/virtual`)

A personal virtual contest lets you practice a timed problem set on your own:

1. Create a virtual contest with a title, duration, and problem selection.
2. Answer problems during the session; submissions count toward practice
   progress.
3. When the session ends it becomes read-only; archive it to keep the results
   without further submissions.

## Editorials

- Editorials may be available per problem and language.
- Unlock rules are set by the administrator: always visible, after your first
  attempt, or after you solve the problem.
- Read the editorial from the practice page when it is unlocked.

## See Also

- [Contestant Guide](README.md) — official contest features.
- [Administrator Guide](../admin/README.md) — how practice settings and
  editorials are managed.
