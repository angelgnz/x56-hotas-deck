# AGENTS.md - Multi-Agent Orchestration Rules (Herdr + AGY)

## 1. Role of the Lead Agent (Lead / Orchestrator)
- You act as the **coordinating architect and dispatcher** of the project. Your primary mission is to keep your context window clean, plan systematically, partition work, and validate results.
- **Prohibition of Monolithic Execution:** Do not perform complex tasks, extensive multi-file refactorings, or mass sweeps directly in your primary conversation thread.
- **Delegation Threshold:**
  - **Direct Execution:** Atomic tasks affecting 1 to 2 files, quick status inspections, or trivial localized edits.
  - **Mandatory Delegation (Herdr / Subagents):** Tasks involving 3 or more files, full test suite executions, code scaffolding generation, deep bug diagnostics, or independent parallel workflows.
- You have access to the **Herdr Skill** (`herdr` CLI, `hc`, `herdr-app`) and subagent tools to manage panes, tabs, and dispatch workers while configuring the **AGY** model and reasoning effort level (`low`, `medium`, `high`).

---

## 2. Model Specialization Matrix (AGY Ecosystem)
Delegate each subtask selecting the model and reasoning effort strictly based on the nature of the workload:

### A. AGY - Claude (`medium` / `high`)
- **Role:** Architecture, system design, and code structuring.
- **Use Cases:** System architecture, complex UI/UX layouts, large-scale structural refactoring, design decisions, and technical specifications.
- **Effort Allocation:**
  - `high`: Architecture design from scratch, critical system definitions, multi-module coordination.
  - `medium`: Standard components, tab views, controllers, and technical documentation.

### B. AGY - Argon (`medium` / `high`)
- **Role:** Deep logical reasoning, auditing, and root-cause diagnostics.
- **Use Cases:** Complex debugging, race conditions, performance bottlenecks, critical algorithms, safety audits, and cross-code reviews.
- **Effort Allocation:**
  - `high`: Root Cause Analysis (RCA), security/permission boundary verification, mathematical curve validation, complex algorithmic bugs.
  - `medium`: PR reviews, business logic validation, protocol data contracts verification.

### C. AGY - Flash (`low` / `medium` / `high`)
- **Role:** High-volume operational execution, speed, and large-context synthesis.
- **Use Cases:** Repetitive tasks, large-window reading/synthesis, test suite generation, mechanical migrations, string extraction, and rapid API integrations.
- **Effort Allocation:**
  - `low` *(Local model replacement)*: Repetitive script execution, bulk searches, formatting, linting fixes, basic unit test scaffolding.
  - `medium`: Rapid iteration of standard features, guided multi-file string replacements, translation file generation.
  - `high`: Comprehensive multi-file documentation synthesis and high-precision information extraction across dozens of files.

---

## 3. Execution Protocol and Lifecycle with Herdr

Follow this sequential lifecycle for all delegated tasks:

### Step 1: Planning and Scope Isolation
- Prior to creating panels or launching agents, decompose the problem into independent subtasks.
- **Anti-Collision Rule:** Never assign two concurrent workers to modify the same file at the same time. Explicitly specify which files are read-only and which are write-enabled for each worker.

### Step 2: Workspace Creation
- Create a split pane or dedicated tab for the subtask:
  - Split pane: `herdr pane split --current --direction right --no-focus`
  - New tab (when managing distinct workflows or >2 agents): `herdr tab create --workspace "$HERDR_WORKSPACE_ID"`

### Step 3: Dispatch with Self-Contained Context
- When delegating or prompting in a new pane/agent, declare the model and effort level explicitly.
- **Worker Prompting Rule:** The worker does not share your implicit conversational context. Always pass a structured prompt including:
  1. **Exact Objective** of the subtask.
  2. **File Paths** involved (allowed scope).
  3. **Acceptance Criteria** (e.g., *"Run test X and ensure zero warnings before completion"*).

### Step 4: Monitoring, Verification, and Teardown
- Inspect the output of the worker pane before marking the subtask complete.
- If a critical task was executed by `Claude` or `Flash`, consider a quick validation pass with `Argon (medium)` when sensitive protocol or hardware logic is affected.
- **Teardown & Cleanup:** Once results are verified and integrated, close temporary working panes (`herdr pane close <id>`) to keep the workspace uncluttered.

---

## 4. Project-Specific Invariants & Quality Gates (`x56linux`)

All agents operating in `x56linux` must strictly preserve these invariants:

1. **Rootless Operation:** All CLI and GUI tools must operate without root (`sudo`) via `udev` (`uaccess`) and `logind`.
2. **Interface 0 Isolation:** Interface 0 (`/dev/input/js*`, `/dev/input/event*`) must **NEVER** be detached, unbinded, or blocked. Games and simulators must receive uninterrupted axis/button input.
3. **No Unwraps in Hot Paths:** Library crates (`x56-proto`, `x56-hid`, `x56-core`) must use `thiserror` and explicit error propagation without panicking `unwrap()` or `expect()`.
4. **Four Mandatory Quality Gates:**
   - `cargo check --workspace`
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --all -- --check`
5. **Conventional Commits:** Follow standard conventions (`feat(...)`, `fix(...)`, `docs(...)`, etc.).
