# Transparent Hover Outline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the translation widget transparent when idle, preserve only a rounded outline, and reveal a translucent surface on hover or keyboard focus while keeping translated content opaque.

**Architecture:** Keep the existing React markup, Props, window APIs, and translation flow unchanged. Implement the state transition entirely in `src/styles.css` using hover/focus selectors and independent foreground colors for result content. Remove the stage background and decorative layers that create the extra outer frame and top line.

**Tech Stack:** React, TypeScript, Tauri 2, CSS transitions and `backdrop-filter`.

## Global Constraints

- Do not change public APIs.
- Do not reduce performance by more than 5%; avoid new high-frequency animations.
- Preserve the 375x260 default widget behavior and all existing window interactions.
- Keep Chinese UI text encoded as valid UTF-8 without mojibake.
- Append the change date, change description, and iteration count to `doc/修改记录.md`.

---

### Task 1: Implement idle transparency and hover surface

**Files:**
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: Existing `.desktop-stage`, `.translation-card`, `.translation-card::before`, `.translation-card::after`, `.result-panel`, and `.result-text` selectors.
- Produces: Transparent idle state, rounded outline, hover/focus translucent surface, and opaque translation text.

- [ ] Set `.desktop-stage` and `.translation-card` idle backgrounds to transparent while retaining rounded clipping and a single visible outline.
- [ ] Hide decorative pseudo-elements and card inner highlight while idle so no second frame or top horizontal line remains.
- [ ] Add `.translation-card:hover` and `.translation-card:focus-within` rules that restore a low-alpha background, blur, and restrained shadow.
- [ ] Keep `.result-text` and `.error-message` foreground colors opaque; only show the result separator for active result states.
- [ ] Preserve reduced-motion behavior and existing control hit areas.

### Task 2: Record and verify

**Files:**
- Modify: `doc/修改记录.md`

**Interfaces:**
- Consumes: Completed CSS behavior from Task 1.
- Produces: Iteration record and verified frontend build/test output.

- [ ] Append the implementation change as iteration 18 in Chinese.
- [ ] Run `cmd /c npm.cmd run build` and verify exit code 0.
- [ ] Run `cmd /c npm.cmd run test -- --run` and verify all tests pass.
- [ ] Scan changed text files for replacement characters and confirm public Props/API files are unchanged.
